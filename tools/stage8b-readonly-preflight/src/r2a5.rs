//! Stage 8B-P R2A5 source-truth, freshness and helper-identity qualification.
//!
//! R2A5 retains the R2A3 bounded GET-only pipeline, but no credential or
//! network client is reached until an independently signed package is bound to
//! the exact manifest, helper, trust set, source generations and account-key
//! generation. The repository intentionally ships no ISSUED package.

use crate::r2a2::{self, ValidatedManifest};
use crate::r2a3::{
    self, R2a3Error, R2a3PipelineInput, R2a3ReadonlyEvidence, SignedAuthorityEnvelope,
    SignedAuthorityReceipt,
};
use crate::Operation;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use zeroize::{Zeroize, Zeroizing};

pub const PACKAGE_SIGNATURE_DOMAIN: &str = "stage8b-p-r2a5-run-package-ed25519-v1";
pub const PUBLIC_KEY_SET_DOMAIN: &str = "stage8b-p-r2a5-public-key-set-v1";
pub const SOURCE_GENERATION_DOMAIN: &str = "stage8b-p-r2a5-source-generation-set-v1";
pub const HELPER_ACCEPTANCE_DOMAIN: &str = "stage8b-p-r2a5-helper-acceptance-ed25519-v1";
pub const PRODUCTION_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a5";
pub const PRODUCTION_ETC: &str = "/etc/moex-trading/stage8b/r2a5";
pub const PRODUCTION_RUN: &str = "/run/moex-trading/stage8b/r2a5";
pub const PRODUCTION_CREDENTIALS: &str = "/run/credentials/moex-trading/stage8b/r2a5";
pub const PRODUCTION_UPSTREAM_ROOT: &str = "/var/lib/moex-trading/operational-authorities";
pub const R2A6_SOURCE_ADAPTER_UID: u32 = 8095;
pub const CONTROLLED_HOST: &str = "stage8b-r2a5.invalid";
const CONTROLLED_CA_PATH: &str = "/run/moex-trading/stage8b/r2a5/controlled-ca.der";
const CONTROLLED_ENDPOINT_PATH: &str = "/run/moex-trading/stage8b/r2a5/controlled-endpoint.txt";
const AUTHORITY: &str = include_str!("../../../docs/stage-8/stage8b-p-r2a5-authority.json");
const CONTROLLED_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r2a5-controlled-authority.json");
const READ_CONTRACT_SNAPSHOT: &[u8] =
    include_bytes!("../../../docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json");
const SOURCE_ADAPTER_AUTHORITY: &[u8] =
    include_bytes!("../../../docs/stage-8/stage8b-p-r2a5-source-adapter-authority.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PinnedPublicKey {
    pub key_id: String,
    pub generation: u64,
    pub public_key_ed25519_hex: String,
    pub public_key_sha256: String,
    pub valid_from_utc: DateTime<Utc>,
    pub valid_until_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustSetManifest {
    pub schema_version: u8,
    pub environment: String,
    pub authorization_key: PinnedPublicKey,
    pub helper_acceptance_key: PinnedPublicKey,
    pub source_keys: BTreeMap<String, PinnedPublicKey>,
    pub public_key_set_sha256: String,
    pub rotation_requires_new_reviewed_package: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptedHelperAuthority {
    pub schema_version: u8,
    pub stage: String,
    pub revision: String,
    pub status: String,
    pub helper_executable_sha256: String,
    pub effect_build_identity_sha256: String,
    pub valid_from_utc: DateTime<Utc>,
    pub valid_until_utc: DateTime<Utc>,
    pub acceptance_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccountKeyEntry {
    pub generation_id: String,
    pub key_sha256: String,
    pub relative_key_path: String,
    pub valid_from_utc: DateTime<Utc>,
    pub valid_until_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccountKeyManifest {
    pub schema_version: u8,
    pub entries: Vec<AccountKeyEntry>,
}

/// Closed, source-specific records emitted by the accepted operational owners.
/// The R2A producer reads these records directly; there is no manually
/// manually populated R2A authoritative-store production seam.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "source_name", deny_unknown_fields)]
pub enum OperationalAuthorityRecord {
    #[serde(rename = "stage7b_current_recovery_seal")]
    Stage7bRecoverySeal {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        stage7b_seal_generation: u64,
        stage6_checkpoint_fingerprint: String,
    },
    #[serde(rename = "stage6_exact_dispatch_ready_command")]
    Stage6DispatchReadyCommand {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        strategy_request_id: String,
        durable_client_order_id: String,
        operation: String,
        request_body_sha256: String,
        cancel_target_broker_order_id: Option<String>,
        cancel_target_lifecycle_fingerprint: Option<String>,
        cancel_target_currently_working_proof_sha256: Option<String>,
    },
    #[serde(rename = "stage8a_root_config_policy_control")]
    Stage8aRootControl {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        config_sha256: String,
        policy_sha256: String,
        config_policy_authority_sha256: String,
    },
    #[serde(rename = "composite_readiness")]
    CompositeReadiness {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        ready: bool,
    },
    #[serde(rename = "kill_switch_run_allowed")]
    KillSwitch {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        run_allowed: bool,
        kill_switch_generation: String,
    },
    #[serde(rename = "single_finam_ownership")]
    SingleFinamOwnership {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        single_owner: bool,
        ownership_lease_fingerprint: String,
    },
    #[serde(rename = "schedule")]
    Schedule {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        eligible: bool,
    },
    #[serde(rename = "instrument_specification")]
    InstrumentSpecification {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        instrument: String,
        eligible: bool,
    },
    #[serde(rename = "ambiguity_orphan_unresolved_lifecycle")]
    LifecycleClarity {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        clear: bool,
    },
    #[serde(rename = "durable_micro_budget")]
    DurableMicroBudget {
        schema_version: u8,
        generation: u64,
        observed_at_utc: DateTime<Utc>,
        available: bool,
        durable_budget_generation: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ControlledSourceAdapterFixture {
    schema_version: u8,
    writer_owner: String,
    writer_api: String,
    source_name: String,
    generation: u64,
    source_observed_at_utc: DateTime<Utc>,
    claims: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct R2a5RunPackage {
    pub package_version: u8,
    pub authorization_status: String,
    pub issued_at_utc: DateTime<Utc>,
    pub expires_at_utc: DateTime<Utc>,
    pub operation: Operation,
    pub run_nonce_sha256: String,
    pub run_identity_sha256: String,
    pub manifest_sha256: String,
    pub keyed_account_binding_hmac_sha256: String,
    pub account_key_generation_id: String,
    pub account_key_manifest_sha256: String,
    pub effect_build_identity_sha256: String,
    pub helper_executable_sha256: String,
    pub contract_snapshot_sha256: String,
    pub source_adapter_authority_sha256: String,
    pub trust_manifest_sha256: String,
    pub public_key_set_sha256: String,
    pub source_generation_commitment_sha256: String,
    pub operator_decision_sha256: String,
    pub authorization_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedR2a5Authority {
    schema_version: u8,
    stage: String,
    revision: String,
    authorization_status: String,
    authorization_public_key_sha256: String,
    trust_manifest_sha256: String,
    public_key_set_sha256: String,
    account_key_manifest_sha256: String,
    source_adapter_authority_sha256: String,
}

pub(crate) struct PreparedR2a5Run {
    package: R2a5RunPackage,
    manifest: Zeroizing<Vec<u8>>,
    receipts: Zeroizing<Vec<u8>>,
    public_keys: BTreeMap<String, VerifyingKey>,
    account_id: Zeroizing<String>,
    account_key: Zeroizing<[u8; 32]>,
    secret: Zeroizing<String>,
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> String {
    lower_hex(&Sha256::digest(bytes))
}

fn decode_hex<const N: usize>(text: &str) -> Result<[u8; N], R2a3Error> {
    if text.len() != N * 2
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(R2a3Error::Authorization);
    }
    let mut output = [0u8; N];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let value = std::str::from_utf8(pair).map_err(|_| R2a3Error::Authorization)?;
        output[index] = u8::from_str_radix(value, 16).map_err(|_| R2a3Error::Authorization)?;
    }
    Ok(output)
}

fn source_names() -> BTreeSet<&'static str> {
    r2a2::required_local_source_names().collect()
}

pub fn public_key_set_digest(manifest: &TrustSetManifest) -> Result<String, R2a3Error> {
    if manifest
        .source_keys
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != source_names()
    {
        return Err(R2a3Error::Authorization);
    }
    let mut parts = vec![
        manifest.helper_acceptance_key.key_id.clone(),
        manifest.helper_acceptance_key.generation.to_string(),
        manifest.helper_acceptance_key.public_key_sha256.clone(),
        r2a2::exact_millis(manifest.helper_acceptance_key.valid_from_utc),
        r2a2::exact_millis(manifest.helper_acceptance_key.valid_until_utc),
    ];
    for (source, key) in &manifest.source_keys {
        parts.push(source.clone());
        parts.push(key.key_id.clone());
        parts.push(key.generation.to_string());
        parts.push(key.public_key_sha256.clone());
        parts.push(r2a2::exact_millis(key.valid_from_utc));
        parts.push(r2a2::exact_millis(key.valid_until_utc));
    }
    Ok(crate::digest_parts(
        PUBLIC_KEY_SET_DOMAIN,
        &parts.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

pub fn source_generation_commitment(
    receipts: &[SignedAuthorityReceipt],
) -> Result<String, R2a3Error> {
    if receipts.len() != source_names().len() {
        return Err(R2a3Error::Authorization);
    }
    let mut sorted = receipts.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|item| item.receipt.source_name.as_str());
    let mut parts = Vec::new();
    for signed in sorted {
        parts.push(signed.receipt.source_name.clone());
        parts.push(signed.source_generation.to_string());
        parts.push(signed.producer_executable_sha256.clone());
        parts.push(signed.issuer_executable_sha256.clone());
        parts.push(signed.authoritative_store_sha256.clone());
        parts.push(signed.source_snapshot_sha256.clone());
        parts.push(signed.issuer_key_id.clone());
    }
    Ok(crate::digest_parts(
        SOURCE_GENERATION_DOMAIN,
        &parts.iter().map(String::as_str).collect::<Vec<_>>(),
    ))
}

fn package_preimage(package: &R2a5RunPackage) -> Result<Vec<u8>, R2a3Error> {
    let mut unsigned = package.clone();
    unsigned.signature_ed25519_hex.zeroize();
    let body = serde_json::to_vec(&unsigned)?;
    let mut preimage = Vec::with_capacity(PACKAGE_SIGNATURE_DOMAIN.len() + 1 + body.len());
    preimage.extend_from_slice(PACKAGE_SIGNATURE_DOMAIN.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&body);
    Ok(preimage)
}

fn helper_acceptance_preimage(authority: &AcceptedHelperAuthority) -> Result<Vec<u8>, R2a3Error> {
    let mut unsigned = authority.clone();
    unsigned.signature_ed25519_hex.zeroize();
    let body = serde_json::to_vec(&unsigned)?;
    let mut preimage = Vec::with_capacity(HELPER_ACCEPTANCE_DOMAIN.len() + 1 + body.len());
    preimage.extend_from_slice(HELPER_ACCEPTANCE_DOMAIN.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&body);
    Ok(preimage)
}

fn sign_helper_acceptance(
    mut authority: AcceptedHelperAuthority,
    signing_key: &SigningKey,
) -> Result<AcceptedHelperAuthority, R2a3Error> {
    authority.signature_ed25519_hex.zeroize();
    authority.signature_ed25519_hex = lower_hex(
        &signing_key
            .sign(&helper_acceptance_preimage(&authority)?)
            .to_bytes(),
    );
    Ok(authority)
}

/// Accepts one exact reviewed helper/effect pair with the independent helper
/// acceptance key. This ceremony is deliberately separate from run-package
/// authorization, and writes no run package or execution capability.
pub fn accept_helper_from_fixed_authority(
    helper_executable_sha256: &str,
    effect_build_identity_sha256: &str,
) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    decode_hex::<32>(helper_executable_sha256)?;
    decode_hex::<32>(effect_build_identity_sha256)?;
    let etc_root = Path::new(PRODUCTION_ETC);
    let credentials_root = Path::new(PRODUCTION_CREDENTIALS);
    let trust: TrustSetManifest = serde_json::from_slice(&read_owned_fd(
        &etc_root.join("trust-manifest.json"),
        128 * 1024,
        0,
        false,
    )?)?;
    let now = Utc::now();
    validate_pinned_key(&trust.helper_acceptance_key, now)?;
    let seed = strict_single_line(
        &read_owned_fd(
            &credentials_root.join("helper-acceptance.ed25519"),
            128,
            0,
            true,
        )?,
        128,
    )?;
    let signing = SigningKey::from_bytes(&decode_hex::<32>(&seed)?);
    if lower_hex(&signing.verifying_key().to_bytes())
        != trust.helper_acceptance_key.public_key_ed25519_hex
    {
        return Err(R2a3Error::Authorization);
    }
    let authority = sign_helper_acceptance(
        AcceptedHelperAuthority {
            schema_version: 1,
            stage: "8B-P".to_owned(),
            revision: "R2A5".to_owned(),
            status: "ACCEPTED".to_owned(),
            helper_executable_sha256: helper_executable_sha256.to_owned(),
            effect_build_identity_sha256: effect_build_identity_sha256.to_owned(),
            valid_from_utc: trust.helper_acceptance_key.valid_from_utc,
            valid_until_utc: trust.helper_acceptance_key.valid_until_utc,
            acceptance_key_id: trust.helper_acceptance_key.key_id,
            signature_ed25519_hex: String::new(),
        },
        &signing,
    )?;
    atomic_write_owned(
        &etc_root.join("accepted-helper-authority.json"),
        &serde_json::to_vec_pretty(&authority)?,
        0,
    )
}

pub fn sign_run_package(
    mut package: R2a5RunPackage,
    signing_key: &SigningKey,
) -> Result<R2a5RunPackage, R2a3Error> {
    package.signature_ed25519_hex.zeroize();
    let signature = signing_key.sign(&package_preimage(&package)?);
    package.signature_ed25519_hex = lower_hex(&signature.to_bytes());
    Ok(package)
}

pub fn issue_run_package_from_fixed_draft() -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    let etc_root = Path::new(PRODUCTION_ETC);
    let state_root = Path::new(PRODUCTION_ROOT);
    let credentials_root = Path::new(PRODUCTION_CREDENTIALS);
    let trust: TrustSetManifest = serde_json::from_slice(&read_owned_fd(
        &etc_root.join("trust-manifest.json"),
        128 * 1024,
        0,
        false,
    )?)?;
    let now = Utc::now();
    let accepted_helper = load_accepted_helper_authority(etc_root, &trust, now)?;
    let draft_bytes = read_owned_fd(
        &state_root.join("r2b-run-package.unsigned.json"),
        128 * 1024,
        0,
        false,
    )?;
    let draft: R2a5RunPackage = serde_json::from_slice(&draft_bytes)?;
    if draft.authorization_status != "ISSUED"
        || !draft.signature_ed25519_hex.is_empty()
        || draft.authorization_key_id != trust.authorization_key.key_id
        || draft.helper_executable_sha256 != accepted_helper.helper_executable_sha256
        || draft.effect_build_identity_sha256 != accepted_helper.effect_build_identity_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    let key_text = strict_single_line(
        &read_owned_fd(
            &credentials_root.join("package-authorization.ed25519"),
            128,
            0,
            true,
        )?,
        128,
    )?;
    let signing = SigningKey::from_bytes(&decode_hex::<32>(&key_text)?);
    let public = signing.verifying_key().to_bytes();
    if lower_hex(&public) != trust.authorization_key.public_key_ed25519_hex
        || sha256(&public) != trust.authorization_key.public_key_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    let signed = sign_run_package(draft, &signing)?;
    atomic_write_owned(
        &etc_root.join("r2b-run-package.json"),
        &serde_json::to_vec(&signed)?,
        0,
    )
}

fn strict_single_line(bytes: &[u8], cap: usize) -> Result<String, R2a3Error> {
    if bytes.is_empty() || bytes.len() > cap || bytes.contains(&0) {
        return Err(R2a3Error::Input);
    }
    let content = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if content.is_empty()
        || content.contains(&b'\n')
        || content.contains(&b'\r')
        || content.first().is_some_and(u8::is_ascii_whitespace)
        || content.last().is_some_and(u8::is_ascii_whitespace)
    {
        return Err(R2a3Error::Input);
    }
    String::from_utf8(content.to_vec()).map_err(|_| R2a3Error::Input)
}

fn read_owned_fd(
    path: &Path,
    cap: usize,
    owner: u32,
    secret: bool,
) -> Result<Zeroizing<Vec<u8>>, R2a3Error> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    let forbidden_mode = if secret { 0o077 } else { 0o022 };
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != owner
        || metadata.mode() & forbidden_mode != 0
        || metadata.len() > cap as u64
    {
        return Err(R2a3Error::Input);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take((cap + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > cap {
        return Err(R2a3Error::Input);
    }
    Ok(bytes)
}

fn validate_pinned_key(
    key: &PinnedPublicKey,
    now: DateTime<Utc>,
) -> Result<VerifyingKey, R2a3Error> {
    if key.generation == 0 || now < key.valid_from_utc || now >= key.valid_until_utc {
        return Err(R2a3Error::Authorization);
    }
    let raw = decode_hex::<32>(&key.public_key_ed25519_hex)?;
    if sha256(&raw) != key.public_key_sha256 {
        return Err(R2a3Error::Authorization);
    }
    VerifyingKey::from_bytes(&raw).map_err(|_| R2a3Error::Authorization)
}

fn load_accepted_helper_authority(
    etc_root: &Path,
    trust: &TrustSetManifest,
    now: DateTime<Utc>,
) -> Result<AcceptedHelperAuthority, R2a3Error> {
    let bytes = read_owned_fd(
        &etc_root.join("accepted-helper-authority.json"),
        64 * 1024,
        0,
        false,
    )?;
    let authority: AcceptedHelperAuthority = serde_json::from_slice(&bytes)?;
    let key = validate_pinned_key(&trust.helper_acceptance_key, now)?;
    if authority.schema_version != 1
        || authority.stage != "8B-P"
        || authority.revision != "R2A5"
        || authority.status != "ACCEPTED"
        || authority.acceptance_key_id != trust.helper_acceptance_key.key_id
        || now < authority.valid_from_utc
        || now >= authority.valid_until_utc
        || decode_hex::<32>(&authority.helper_executable_sha256).is_err()
        || decode_hex::<32>(&authority.effect_build_identity_sha256).is_err()
    {
        return Err(R2a3Error::Authorization);
    }
    let signature = Signature::from_bytes(&decode_hex::<64>(&authority.signature_ed25519_hex)?);
    key.verify(&helper_acceptance_preimage(&authority)?, &signature)
        .map_err(|_| R2a3Error::Authorization)?;
    Ok(authority)
}

fn chown_path(path: &Path, uid: u32, gid: u32) -> Result<(), R2a3Error> {
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| R2a3Error::Input)?;
    if unsafe { libc::chown(path.as_ptr(), uid, gid) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn prepare_directory(path: &Path, uid: u32, mode: u32) -> Result<(), R2a3Error> {
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    chown_path(path, uid, uid)
}

fn write_seed_file(path: &Path, bytes: &[u8], uid: u32, mode: u32) -> Result<(), R2a3Error> {
    let parent = path.parent().ok_or(R2a3Error::Input)?;
    std::fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    chown_path(path, uid, uid)
}

fn controlled_trust_and_account_manifests(
) -> Result<(TrustSetManifest, AccountKeyManifest, SigningKey), R2a3Error> {
    let valid_from = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .map_err(|_| R2a3Error::Input)?
        .with_timezone(&Utc);
    let valid_until = DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .map_err(|_| R2a3Error::Input)?
        .with_timezone(&Utc);
    let authorization_signing = SigningKey::from_bytes(&[99u8; 32]);
    let authorization_public = authorization_signing.verifying_key().to_bytes();
    let authorization_key = PinnedPublicKey {
        key_id: "stage8b-r2a5-controlled-package-authorization-v1".to_owned(),
        generation: 1,
        public_key_ed25519_hex: lower_hex(&authorization_public),
        public_key_sha256: sha256(&authorization_public),
        valid_from_utc: valid_from,
        valid_until_utc: valid_until,
    };
    let helper_acceptance_public = SigningKey::from_bytes(&[98u8; 32])
        .verifying_key()
        .to_bytes();
    let helper_acceptance_key = PinnedPublicKey {
        key_id: "stage8b-r2a5-controlled-helper-acceptance-v1".to_owned(),
        generation: 1,
        public_key_ed25519_hex: lower_hex(&helper_acceptance_public),
        public_key_sha256: sha256(&helper_acceptance_public),
        valid_from_utc: valid_from,
        valid_until_utc: valid_until,
    };
    let mut source_keys = BTreeMap::new();
    for (index, source) in source_names().into_iter().enumerate() {
        let signing = SigningKey::from_bytes(&[index as u8 + 1; 32]);
        let public = signing.verifying_key().to_bytes();
        source_keys.insert(
            source.to_owned(),
            PinnedPublicKey {
                key_id: format!("{source}-ed25519-v1"),
                generation: 1,
                public_key_ed25519_hex: lower_hex(&public),
                public_key_sha256: sha256(&public),
                valid_from_utc: valid_from,
                valid_until_utc: valid_until,
            },
        );
    }
    let mut trust = TrustSetManifest {
        schema_version: 1,
        environment: "production".to_owned(),
        authorization_key,
        helper_acceptance_key,
        source_keys,
        public_key_set_sha256: String::new(),
        rotation_requires_new_reviewed_package: true,
    };
    trust.public_key_set_sha256 = public_key_set_digest(&trust)?;
    let account_key = decode_hex::<32>(&lower_hex(r2a3::CONTROLLED_ACCOUNT_KEY))?;
    let account = AccountKeyManifest {
        schema_version: 1,
        entries: vec![AccountKeyEntry {
            generation_id: "7".to_owned(),
            key_sha256: sha256(&account_key),
            relative_key_path: "generation-7.hex".to_owned(),
            valid_from_utc: valid_from,
            valid_until_utc: valid_until,
        }],
    };
    Ok((trust, account, authorization_signing))
}

pub fn controlled_authority_values() -> Result<BTreeMap<String, String>, R2a3Error> {
    let (trust, account, _) = controlled_trust_and_account_manifests()?;
    let trust_bytes = serde_json::to_vec(&trust)?;
    let account_bytes = serde_json::to_vec(&account)?;
    Ok(BTreeMap::from([
        (
            "authorization_public_key_sha256".to_owned(),
            trust.authorization_key.public_key_sha256,
        ),
        ("trust_manifest_sha256".to_owned(), sha256(&trust_bytes)),
        (
            "public_key_set_sha256".to_owned(),
            trust.public_key_set_sha256,
        ),
        (
            "account_key_manifest_sha256".to_owned(),
            sha256(&account_bytes),
        ),
        (
            "source_adapter_authority_sha256".to_owned(),
            sha256(SOURCE_ADAPTER_AUTHORITY),
        ),
    ]))
}

fn publish_controlled_source_adapter_fixtures(
    fixture_root: &Path,
    upstream_root: &Path,
) -> Result<(), R2a3Error> {
    for source in source_names()
        .into_iter()
        .filter(|source| *source != "trusted_clock")
    {
        let bytes = read_owned_fd(
            &fixture_root.join(format!("{source}.json")),
            128 * 1024,
            0,
            false,
        )?;
        let fixture: ControlledSourceAdapterFixture = serde_json::from_slice(&bytes)?;
        if fixture.schema_version != 1
            || fixture.writer_owner != "finam_gateway::Stage8a1OperationalAuthorityIssuer"
            || fixture.writer_api != "publish_stage8b_r2a5_operational_sources"
            || fixture.source_name != source
            || fixture.generation == 0
        {
            return Err(R2a3Error::Provenance);
        }
        let authority = controlled_operational_authority(
            source,
            fixture.generation,
            fixture.source_observed_at_utc,
            &fixture.claims,
        )?;
        write_seed_file(
            &upstream_root.join(operational_authority_file(source)?),
            &serde_json::to_vec(&authority)?,
            0,
            0o644,
        )?;
    }
    Ok(())
}

fn random_seed() -> Result<Zeroizing<[u8; 32]>, R2a3Error> {
    let mut seed = Zeroizing::new([0u8; 32]);
    File::open("/dev/urandom")?.read_exact(&mut seed[..])?;
    Ok(seed)
}

fn ceremony_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), R2a3Error> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Generates an offline R2A5 key ceremony. Secret material is written only to
/// the caller-selected, newly-created directory; only the two public manifests
/// are intended to enter source control.
pub fn generate_key_ceremony(output: &Path) -> Result<BTreeMap<String, String>, R2a3Error> {
    std::fs::create_dir(output)?;
    std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o700))?;
    let valid_from = DateTime::parse_from_rfc3339("2026-08-26T00:00:00Z")
        .map_err(|_| R2a3Error::Input)?
        .with_timezone(&Utc);
    let valid_until = DateTime::parse_from_rfc3339("2027-08-26T00:00:00Z")
        .map_err(|_| R2a3Error::Input)?
        .with_timezone(&Utc);
    let authorization_seed = random_seed()?;
    let authorization_signing = SigningKey::from_bytes(&authorization_seed);
    let authorization_public = authorization_signing.verifying_key().to_bytes();
    ceremony_write(
        &output.join("package-authorization.ed25519"),
        format!("{}\n", lower_hex(&authorization_seed[..])).as_bytes(),
        0o600,
    )?;
    let authorization_key = PinnedPublicKey {
        key_id: "stage8b-r2a5-production-package-authorization-v1".to_owned(),
        generation: 1,
        public_key_ed25519_hex: lower_hex(&authorization_public),
        public_key_sha256: sha256(&authorization_public),
        valid_from_utc: valid_from,
        valid_until_utc: valid_until,
    };
    let helper_acceptance_seed = random_seed()?;
    let helper_acceptance_signing = SigningKey::from_bytes(&helper_acceptance_seed);
    let helper_acceptance_public = helper_acceptance_signing.verifying_key().to_bytes();
    ceremony_write(
        &output.join("helper-acceptance.ed25519"),
        format!("{}\n", lower_hex(&helper_acceptance_seed[..])).as_bytes(),
        0o600,
    )?;
    let helper_acceptance_key = PinnedPublicKey {
        key_id: "stage8b-r2a5-production-helper-acceptance-v1".to_owned(),
        generation: 1,
        public_key_ed25519_hex: lower_hex(&helper_acceptance_public),
        public_key_sha256: sha256(&helper_acceptance_public),
        valid_from_utc: valid_from,
        valid_until_utc: valid_until,
    };
    let issuer_root = output.join("issuer-private-keys");
    std::fs::create_dir(&issuer_root)?;
    std::fs::set_permissions(&issuer_root, std::fs::Permissions::from_mode(0o700))?;
    let mut source_keys = BTreeMap::new();
    for source in source_names() {
        let directory = issuer_root.join(source);
        std::fs::create_dir(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let seed = random_seed()?;
        let signing = SigningKey::from_bytes(&seed);
        let public = signing.verifying_key().to_bytes();
        ceremony_write(
            &directory.join("key.ed25519"),
            format!("{}\n", lower_hex(&seed[..])).as_bytes(),
            0o600,
        )?;
        source_keys.insert(
            source.to_owned(),
            PinnedPublicKey {
                key_id: format!("{source}-ed25519-v1"),
                generation: 1,
                public_key_ed25519_hex: lower_hex(&public),
                public_key_sha256: sha256(&public),
                valid_from_utc: valid_from,
                valid_until_utc: valid_until,
            },
        );
    }
    let mut trust = TrustSetManifest {
        schema_version: 1,
        environment: "production".to_owned(),
        authorization_key,
        helper_acceptance_key,
        source_keys,
        public_key_set_sha256: String::new(),
        rotation_requires_new_reviewed_package: true,
    };
    trust.public_key_set_sha256 = public_key_set_digest(&trust)?;
    let account_key = random_seed()?;
    ceremony_write(
        &output.join("account-binding-generation-1.hex"),
        format!("{}\n", lower_hex(&account_key[..])).as_bytes(),
        0o600,
    )?;
    let account = AccountKeyManifest {
        schema_version: 1,
        entries: vec![AccountKeyEntry {
            generation_id: "1".to_owned(),
            key_sha256: sha256(&account_key[..]),
            relative_key_path: "generation-1.hex".to_owned(),
            valid_from_utc: valid_from,
            valid_until_utc: valid_until,
        }],
    };
    let trust_bytes = serde_json::to_vec_pretty(&trust)?;
    let account_bytes = serde_json::to_vec_pretty(&account)?;
    ceremony_write(&output.join("trust-manifest.json"), &trust_bytes, 0o644)?;
    ceremony_write(
        &output.join("account-key-manifest.json"),
        &account_bytes,
        0o644,
    )?;
    File::open(output)?.sync_all()?;
    Ok(BTreeMap::from([
        (
            "authorization_public_key_sha256".to_owned(),
            trust.authorization_key.public_key_sha256,
        ),
        ("trust_manifest_sha256".to_owned(), sha256(&trust_bytes)),
        (
            "public_key_set_sha256".to_owned(),
            trust.public_key_set_sha256,
        ),
        (
            "account_key_manifest_sha256".to_owned(),
            sha256(&account_bytes),
        ),
        (
            "source_adapter_authority_sha256".to_owned(),
            sha256(SOURCE_ADAPTER_AUTHORITY),
        ),
    ]))
}

pub fn seed_controlled_fixed_layout(operation: Operation) -> Result<(), R2a3Error> {
    seed_controlled_fixed_layout_inner(operation, None)
}

pub fn seed_controlled_r2a6_layout(operation: Operation) -> Result<(), R2a3Error> {
    seed_controlled_fixed_layout_inner(operation, Some(R2A6_SOURCE_ADAPTER_UID))
}

/// Rebinds only the controlled rehearsal manifest to the exact records emitted
/// by the qualified R2A6 adapter. Production R2B has its own signed operator
/// decision/package path; this helper cannot issue that authority.
pub fn bind_controlled_r2a6_manifest_to_operational_sources() -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    let state_root = Path::new(PRODUCTION_ROOT);
    let manifest_path = state_root.join("run-manifest.json");
    let manifest = read_owned_fd(&manifest_path, 256 * 1024, 0, false)?;
    let mut fields: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    let expected_operation = manifest_field(&fields, "operation")?.to_owned();
    let mut claims_by_source = BTreeMap::new();
    for source in source_names()
        .into_iter()
        .filter(|source| *source != "trusted_clock")
    {
        let source_path =
            Path::new(PRODUCTION_UPSTREAM_ROOT).join(operational_authority_file(source)?);
        let bytes = read_owned_fd(&source_path, 128 * 1024, R2A6_SOURCE_ADAPTER_UID, false)?;
        let record: OperationalAuthorityRecord = serde_json::from_slice(&bytes)?;
        let (_, _, claims) = reduce_operational_authority(source, record)?;
        claims_by_source.insert(source, claims);
    }
    let claim = |source: &str, name: &str| -> Result<String, R2a3Error> {
        claims_by_source
            .get(source)
            .and_then(|claims| claims.get(name))
            .cloned()
            .ok_or(R2a3Error::Input)
    };
    let stage6 = claims_by_source
        .get("stage6_exact_dispatch_ready_command")
        .ok_or(R2a3Error::Input)?;
    if stage6.get("operation").map(String::as_str) != Some(expected_operation.as_str()) {
        return Err(R2a3Error::Authorization);
    }
    for (field, source, source_claim) in [
        (
            "stage7b_seal_generation",
            "stage7b_current_recovery_seal",
            "stage7b_seal_generation",
        ),
        (
            "stage6_checkpoint_fingerprint",
            "stage7b_current_recovery_seal",
            "stage6_checkpoint_fingerprint",
        ),
        (
            "strategy_request_id",
            "stage6_exact_dispatch_ready_command",
            "strategy_request_id",
        ),
        (
            "durable_client_order_id",
            "stage6_exact_dispatch_ready_command",
            "durable_client_order_id",
        ),
        (
            "kill_switch_generation",
            "kill_switch_run_allowed",
            "kill_switch_generation",
        ),
        (
            "ownership_lease_fingerprint",
            "single_finam_ownership",
            "ownership_lease_fingerprint",
        ),
        (
            "durable_budget_generation",
            "durable_micro_budget",
            "durable_budget_generation",
        ),
    ] {
        fields.insert(field.to_owned(), claim(source, source_claim)?);
    }
    let request_body_field = match expected_operation.as_str() {
        "PLACE" => "place_request_body_sha256",
        "CANCEL" => "cancel_request_body_sha256",
        _ => return Err(R2a3Error::Authorization),
    };
    fields.insert(
        request_body_field.to_owned(),
        claim("stage6_exact_dispatch_ready_command", "request_body_sha256")?,
    );
    if expected_operation == "CANCEL" {
        for field in [
            "cancel_target_broker_order_id",
            "cancel_target_lifecycle_fingerprint",
            "cancel_target_currently_working_proof_sha256",
        ] {
            fields.insert(
                field.to_owned(),
                claim("stage6_exact_dispatch_ready_command", field)?,
            );
        }
        fields.insert(
            "cancel_target_durable_client_order_id".to_owned(),
            claim(
                "stage6_exact_dispatch_ready_command",
                "durable_client_order_id",
            )?,
        );
        fields.insert(
            "cancel_target_strategy_request_id".to_owned(),
            claim("stage6_exact_dispatch_ready_command", "strategy_request_id")?,
        );
    }
    for (field, source_claim) in [
        ("config_sha256", "config_sha256"),
        ("policy_sha256", "policy_sha256"),
        (
            "config_policy_authority_sha256",
            "config_policy_authority_sha256",
        ),
    ] {
        if fields.get(field).map(String::as_str)
            != Some(claim("stage8a_root_config_policy_control", source_claim)?.as_str())
        {
            return Err(R2a3Error::Authorization);
        }
    }
    if claim("instrument_specification", "instrument")? != r2a2::TARGET_INSTRUMENT
        || claim("composite_readiness", "ready")? != "true"
        || claim("kill_switch_run_allowed", "run_allowed")? != "true"
        || claim("single_finam_ownership", "single_owner")? != "true"
        || claim("schedule", "eligible")? != "true"
        || claim("instrument_specification", "eligible")? != "true"
        || claim("ambiguity_orphan_unresolved_lifecycle", "clear")? != "true"
        || claim("durable_micro_budget", "available")? != "true"
    {
        return Err(R2a3Error::Authorization);
    }
    fields.insert(
        "run_expires_at_utc".to_owned(),
        r2a2::exact_millis(Utc::now() + chrono::Duration::seconds(30)),
    );
    let run_identity = r2a2::recompute_manifest_run_identity(&fields)?;
    fields.insert("run_identity_sha256".to_owned(), run_identity);
    atomic_write_owned(&manifest_path, &serde_json::to_vec(&fields)?, 0)
}

fn seed_controlled_fixed_layout_inner(
    operation: Operation,
    source_adapter_uid: Option<u32>,
) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    let now = Utc::now();
    let boot_id = strict_single_line(&std::fs::read("/proc/sys/kernel/random/boot_id")?, 128)?;
    let boot_fingerprint = sha256(boot_id.as_bytes());
    let (manifest, envelope, _, nonce) =
        r2a3::controlled_fixture_for_boot(now, operation, Some(&boot_fingerprint))?;
    let signed: SignedAuthorityEnvelope = serde_json::from_slice(&envelope)?;
    let claims = signed
        .receipts
        .into_iter()
        .map(|receipt| (receipt.receipt.source_name, receipt.receipt.claims))
        .collect::<BTreeMap<_, _>>();
    let etc_root = Path::new(PRODUCTION_ETC);
    let state_root = Path::new(PRODUCTION_ROOT);
    let run_root = Path::new(PRODUCTION_RUN);
    let credentials_root = Path::new(PRODUCTION_CREDENTIALS);
    let upstream_root = Path::new(PRODUCTION_UPSTREAM_ROOT);
    let fixture_root = state_root.join("source-adapter-fixtures");
    for root in [etc_root, state_root, run_root, credentials_root] {
        prepare_directory(root, 0, 0o755)?;
    }
    prepare_directory(upstream_root, source_adapter_uid.unwrap_or(0), 0o755)?;
    if let Some(adapter_uid) = source_adapter_uid {
        prepare_directory(
            Path::new("/var/lib/moex-trading/stage8b/r2a6/adapter-work"),
            adapter_uid,
            0o700,
        )?;
    }
    prepare_directory(&state_root.join("used-run-nonces"), 0, 0o700)?;
    prepare_directory(&etc_root.join("authority-public-keys"), 0, 0o755)?;
    prepare_directory(&credentials_root.join("account-binding-keys"), 0, 0o700)?;
    prepare_directory(&credentials_root.join("issuer-private-keys"), 0, 0o711)?;
    prepare_directory(&run_root.join("receipts"), 0, 0o711)?;
    prepare_directory(&fixture_root, 0, 0o700)?;
    write_seed_file(
        &run_root.join("run-nonce.sha256"),
        format!("{nonce}\n").as_bytes(),
        0,
        0o644,
    )?;
    write_seed_file(&state_root.join("run-manifest.json"), &manifest, 0, 0o644)?;
    let (trust, account_manifest, authorization_signing) =
        controlled_trust_and_account_manifests()?;
    write_seed_file(
        &etc_root.join("trust-manifest.json"),
        &serde_json::to_vec(&trust)?,
        0,
        0o644,
    )?;
    write_seed_file(
        &etc_root.join("account-key-manifest.json"),
        &serde_json::to_vec(&account_manifest)?,
        0,
        0o644,
    )?;
    write_seed_file(
        &etc_root.join("operator-decision.json"),
        br#"{"decision":"controlled-r2a5-rehearsal-only","real_finam":false}"#,
        0,
        0o644,
    )?;
    write_seed_file(
        &credentials_root.join("package-authorization.ed25519"),
        format!("{}\n", lower_hex(&authorization_signing.to_bytes())).as_bytes(),
        0,
        0o600,
    )?;
    write_seed_file(
        &credentials_root.join("account-id"),
        format!("{}\n", r2a3::CONTROLLED_ACCOUNT).as_bytes(),
        0,
        0o600,
    )?;
    write_seed_file(
        &credentials_root.join("finam-readonly-secret"),
        b"controlled-secret-not-a-real-credential\n",
        0,
        0o600,
    )?;
    write_seed_file(
        &credentials_root
            .join("account-binding-keys")
            .join("generation-7.hex"),
        format!("{}\n", lower_hex(r2a3::CONTROLLED_ACCOUNT_KEY)).as_bytes(),
        0,
        0o600,
    )?;
    for (index, source) in source_names().into_iter().enumerate() {
        let producer_uid = r2a3::source_producer_uid(source)?;
        let issuer_uid = r2a3::source_issuer_uid(source)?;
        let source_directory = state_root.join("authority-sources").join(source);
        let receipt_directory = run_root.join("receipts").join(source);
        let private_directory = credentials_root.join("issuer-private-keys").join(source);
        prepare_directory(&source_directory, producer_uid, 0o755)?;
        prepare_directory(&source_directory.join("generations"), producer_uid, 0o700)?;
        prepare_directory(&receipt_directory, issuer_uid, 0o755)?;
        prepare_directory(&private_directory, issuer_uid, 0o700)?;
        if source != "trusted_clock" {
            let fixture = ControlledSourceAdapterFixture {
                schema_version: 1,
                writer_owner: "finam_gateway::Stage8a1OperationalAuthorityIssuer".to_owned(),
                writer_api: "publish_stage8b_r2a5_operational_sources".to_owned(),
                source_name: source.to_owned(),
                generation: index as u64 + 1,
                source_observed_at_utc: now,
                claims: claims.get(source).cloned().ok_or(R2a3Error::Input)?,
            };
            write_seed_file(
                &fixture_root.join(format!("{source}.json")),
                &serde_json::to_vec(&fixture)?,
                0,
                0o644,
            )?;
        }
        let signing = SigningKey::from_bytes(&[index as u8 + 1; 32]);
        write_seed_file(
            &private_directory.join("key.ed25519"),
            format!("{}\n", lower_hex(&signing.to_bytes())).as_bytes(),
            issuer_uid,
            0o600,
        )?;
        write_seed_file(
            &etc_root
                .join("authority-public-keys")
                .join(format!("{source}.ed25519.pub")),
            format!("{}\n", trust.source_keys[source].public_key_ed25519_hex).as_bytes(),
            0,
            0o644,
        )?;
    }
    if source_adapter_uid.is_none() {
        publish_controlled_source_adapter_fixtures(&fixture_root, upstream_root)?;
    }
    Ok(())
}

pub fn finalize_controlled_fixed_layout(helper_sha256: &str) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    decode_hex::<32>(helper_sha256)?;
    let etc_root = Path::new(PRODUCTION_ETC);
    let state_root = Path::new(PRODUCTION_ROOT);
    let run_root = Path::new(PRODUCTION_RUN);
    let nonce = strict_single_line(
        &read_owned_fd(&run_root.join("run-nonce.sha256"), 128, 0, false)?,
        128,
    )?;
    let manifest = read_owned_fd(&state_root.join("run-manifest.json"), 256 * 1024, 0, false)?;
    let fields: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    let operation = match manifest_field(&fields, "operation")? {
        "PLACE" => Operation::Place,
        "CANCEL" => Operation::Cancel,
        _ => return Err(R2a3Error::Authorization),
    };
    let trust_bytes = read_owned_fd(&etc_root.join("trust-manifest.json"), 128 * 1024, 0, false)?;
    let trust: TrustSetManifest = serde_json::from_slice(&trust_bytes)?;
    let account_manifest = read_owned_fd(
        &etc_root.join("account-key-manifest.json"),
        64 * 1024,
        0,
        false,
    )?;
    let operator_decision = read_owned_fd(
        &etc_root.join("operator-decision.json"),
        64 * 1024,
        0,
        false,
    )?;
    let receipts = load_receipts(run_root, &nonce)?;
    let envelope: SignedAuthorityEnvelope = serde_json::from_slice(&receipts)?;
    let now = Utc::now();
    let accepted_helper = sign_helper_acceptance(
        AcceptedHelperAuthority {
            schema_version: 1,
            stage: "8B-P".to_owned(),
            revision: "R2A5".to_owned(),
            status: "ACCEPTED".to_owned(),
            helper_executable_sha256: helper_sha256.to_owned(),
            effect_build_identity_sha256: manifest_field(
                &fields,
                "execution_build_identity_sha256",
            )?
            .to_owned(),
            valid_from_utc: now - chrono::Duration::seconds(1),
            valid_until_utc: now + chrono::Duration::minutes(5),
            acceptance_key_id: trust.helper_acceptance_key.key_id.clone(),
            signature_ed25519_hex: String::new(),
        },
        &SigningKey::from_bytes(&[98u8; 32]),
    )?;
    write_seed_file(
        &etc_root.join("accepted-helper-authority.json"),
        &serde_json::to_vec(&accepted_helper)?,
        0,
        0o644,
    )?;
    let package = R2a5RunPackage {
        package_version: 1,
        authorization_status: "ISSUED".to_owned(),
        issued_at_utc: now,
        expires_at_utc: now + chrono::Duration::seconds(30),
        operation,
        run_nonce_sha256: nonce,
        run_identity_sha256: manifest_field(&fields, "run_identity_sha256")?.to_owned(),
        manifest_sha256: sha256(&manifest),
        keyed_account_binding_hmac_sha256: manifest_field(
            &fields,
            "keyed_account_binding_hmac_sha256",
        )?
        .to_owned(),
        account_key_generation_id: manifest_field(&fields, "account_key_generation_id")?.to_owned(),
        account_key_manifest_sha256: sha256(&account_manifest),
        effect_build_identity_sha256: manifest_field(&fields, "execution_build_identity_sha256")?
            .to_owned(),
        helper_executable_sha256: helper_sha256.to_owned(),
        contract_snapshot_sha256: sha256(READ_CONTRACT_SNAPSHOT),
        source_adapter_authority_sha256: sha256(SOURCE_ADAPTER_AUTHORITY),
        trust_manifest_sha256: sha256(&trust_bytes),
        public_key_set_sha256: trust.public_key_set_sha256,
        source_generation_commitment_sha256: source_generation_commitment(&envelope.receipts)?,
        operator_decision_sha256: sha256(&operator_decision),
        authorization_key_id: trust.authorization_key.key_id,
        signature_ed25519_hex: String::new(),
    };
    write_seed_file(
        &state_root.join("r2b-run-package.unsigned.json"),
        &serde_json::to_vec(&package)?,
        0,
        0o600,
    )
}

fn load_receipts(root: &Path, run_nonce: &str) -> Result<Zeroizing<Vec<u8>>, R2a3Error> {
    let mut receipts = Vec::new();
    for source in source_names() {
        let path = root.join("receipts").join(source).join("receipt.json");
        let bytes = read_owned_fd(&path, 128 * 1024, r2a3::source_issuer_uid(source)?, false)?;
        receipts.push(serde_json::from_slice::<SignedAuthorityReceipt>(&bytes)?);
    }
    Ok(Zeroizing::new(serde_json::to_vec(
        &SignedAuthorityEnvelope {
            schema_version: 1,
            run_nonce_sha256: run_nonce.to_owned(),
            receipts,
        },
    )?))
}

fn load_source_keys(
    directory: &Path,
    trust: &TrustSetManifest,
    now: DateTime<Utc>,
) -> Result<BTreeMap<String, VerifyingKey>, R2a3Error> {
    let mut keys = BTreeMap::new();
    for (source, pinned) in &trust.source_keys {
        let path = directory.join(format!("{source}.ed25519.pub"));
        let text = strict_single_line(&read_owned_fd(&path, 128, 0, false)?, 128)?;
        if text != pinned.public_key_ed25519_hex {
            return Err(R2a3Error::Authorization);
        }
        keys.insert(source.clone(), validate_pinned_key(pinned, now)?);
    }
    Ok(keys)
}

fn exact_operation(operation: Operation) -> &'static str {
    match operation {
        Operation::Place => "PLACE",
        Operation::Cancel => "CANCEL",
    }
}

fn source_freshness_budget_ms(source: &str) -> Result<(i64, i64), R2a3Error> {
    let maximum_age = match source {
        "schedule" | "instrument_specification" => 5_000,
        "trusted_clock"
        | "stage7b_current_recovery_seal"
        | "stage6_exact_dispatch_ready_command"
        | "stage8a_root_config_policy_control"
        | "composite_readiness"
        | "kill_switch_run_allowed"
        | "single_finam_ownership"
        | "ambiguity_orphan_unresolved_lifecycle"
        | "durable_micro_budget" => 1_000,
        _ => return Err(R2a3Error::Input),
    };
    Ok((maximum_age, 250))
}

fn validate_source_freshness(
    source: &str,
    source_observed_at_utc: DateTime<Utc>,
    produced_at_utc: DateTime<Utc>,
) -> Result<(), R2a3Error> {
    let (maximum_age_ms, maximum_future_skew_ms) = source_freshness_budget_ms(source)?;
    let age_ms = produced_at_utc
        .signed_duration_since(source_observed_at_utc)
        .num_milliseconds();
    if age_ms > maximum_age_ms || age_ms < -maximum_future_skew_ms {
        return Err(R2a3Error::Freshness);
    }
    Ok(())
}

fn operational_authority_file(source: &str) -> Result<&'static str, R2a3Error> {
    match source {
        "stage7b_current_recovery_seal" => Ok("stage7b-current-recovery-seal.json"),
        "stage6_exact_dispatch_ready_command" => Ok("stage6-dispatch-ready-command.json"),
        "stage8a_root_config_policy_control" => Ok("stage8a-root-control.json"),
        "composite_readiness" => Ok("stage8a-composite-readiness.json"),
        "kill_switch_run_allowed" => Ok("stage8a-kill-switch.json"),
        "single_finam_ownership" => Ok("stage8a-finam-ownership.json"),
        "schedule" => Ok("stage8a-schedule.json"),
        "instrument_specification" => Ok("stage8a-instrument.json"),
        "ambiguity_orphan_unresolved_lifecycle" => Ok("stage8a-lifecycle-clarity.json"),
        "durable_micro_budget" => Ok("stage8a-durable-micro-budget.json"),
        _ => Err(R2a3Error::Input),
    }
}

fn exact_bool(value: bool) -> String {
    if value { "true" } else { "false" }.to_owned()
}

fn validate_sha256(value: &str) -> Result<(), R2a3Error> {
    decode_hex::<32>(value).map(|_| ())
}

type ReducedOperationalAuthority = (u64, DateTime<Utc>, BTreeMap<String, String>);

fn reduce_operational_authority(
    expected_source: &str,
    record: OperationalAuthorityRecord,
) -> Result<ReducedOperationalAuthority, R2a3Error> {
    let (source, schema, generation, observed_at, claims) = match record {
        OperationalAuthorityRecord::Stage7bRecoverySeal {
            schema_version,
            generation,
            observed_at_utc,
            stage7b_seal_generation,
            stage6_checkpoint_fingerprint,
        } => {
            validate_sha256(&stage6_checkpoint_fingerprint)?;
            (
                "stage7b_current_recovery_seal",
                schema_version,
                generation,
                observed_at_utc,
                BTreeMap::from([
                    (
                        "stage7b_seal_generation".to_owned(),
                        stage7b_seal_generation.to_string(),
                    ),
                    (
                        "stage6_checkpoint_fingerprint".to_owned(),
                        stage6_checkpoint_fingerprint,
                    ),
                ]),
            )
        }
        OperationalAuthorityRecord::Stage6DispatchReadyCommand {
            schema_version,
            generation,
            observed_at_utc,
            strategy_request_id,
            durable_client_order_id,
            operation,
            request_body_sha256,
            cancel_target_broker_order_id,
            cancel_target_lifecycle_fingerprint,
            cancel_target_currently_working_proof_sha256,
        } => {
            validate_sha256(&request_body_sha256)?;
            let mut claims = BTreeMap::from([
                ("strategy_request_id".to_owned(), strategy_request_id),
                (
                    "durable_client_order_id".to_owned(),
                    durable_client_order_id,
                ),
                ("operation".to_owned(), operation.clone()),
                ("request_body_sha256".to_owned(), request_body_sha256),
            ]);
            match operation.as_str() {
                "PLACE"
                    if cancel_target_broker_order_id.is_none()
                        && cancel_target_lifecycle_fingerprint.is_none()
                        && cancel_target_currently_working_proof_sha256.is_none() => {}
                "CANCEL" => {
                    let broker_order_id =
                        cancel_target_broker_order_id.ok_or(R2a3Error::Provenance)?;
                    let lifecycle =
                        cancel_target_lifecycle_fingerprint.ok_or(R2a3Error::Provenance)?;
                    let working = cancel_target_currently_working_proof_sha256
                        .ok_or(R2a3Error::Provenance)?;
                    validate_sha256(&lifecycle)?;
                    validate_sha256(&working)?;
                    claims.insert("cancel_target_broker_order_id".to_owned(), broker_order_id);
                    claims.insert("cancel_target_lifecycle_fingerprint".to_owned(), lifecycle);
                    claims.insert(
                        "cancel_target_currently_working_proof_sha256".to_owned(),
                        working,
                    );
                }
                _ => return Err(R2a3Error::Provenance),
            }
            (
                "stage6_exact_dispatch_ready_command",
                schema_version,
                generation,
                observed_at_utc,
                claims,
            )
        }
        OperationalAuthorityRecord::Stage8aRootControl {
            schema_version,
            generation,
            observed_at_utc,
            config_sha256,
            policy_sha256,
            config_policy_authority_sha256,
        } => {
            validate_sha256(&config_sha256)?;
            validate_sha256(&policy_sha256)?;
            validate_sha256(&config_policy_authority_sha256)?;
            (
                "stage8a_root_config_policy_control",
                schema_version,
                generation,
                observed_at_utc,
                BTreeMap::from([
                    ("config_sha256".to_owned(), config_sha256),
                    ("policy_sha256".to_owned(), policy_sha256),
                    (
                        "config_policy_authority_sha256".to_owned(),
                        config_policy_authority_sha256,
                    ),
                ]),
            )
        }
        OperationalAuthorityRecord::CompositeReadiness {
            schema_version,
            generation,
            observed_at_utc,
            ready,
        } => (
            "composite_readiness",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([("ready".to_owned(), exact_bool(ready))]),
        ),
        OperationalAuthorityRecord::KillSwitch {
            schema_version,
            generation,
            observed_at_utc,
            run_allowed,
            kill_switch_generation,
        } => (
            "kill_switch_run_allowed",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([
                ("run_allowed".to_owned(), exact_bool(run_allowed)),
                ("kill_switch_generation".to_owned(), kill_switch_generation),
            ]),
        ),
        OperationalAuthorityRecord::SingleFinamOwnership {
            schema_version,
            generation,
            observed_at_utc,
            single_owner,
            ownership_lease_fingerprint,
        } => {
            validate_sha256(&ownership_lease_fingerprint)?;
            (
                "single_finam_ownership",
                schema_version,
                generation,
                observed_at_utc,
                BTreeMap::from([
                    ("single_owner".to_owned(), exact_bool(single_owner)),
                    (
                        "ownership_lease_fingerprint".to_owned(),
                        ownership_lease_fingerprint,
                    ),
                ]),
            )
        }
        OperationalAuthorityRecord::Schedule {
            schema_version,
            generation,
            observed_at_utc,
            eligible,
        } => (
            "schedule",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([("eligible".to_owned(), exact_bool(eligible))]),
        ),
        OperationalAuthorityRecord::InstrumentSpecification {
            schema_version,
            generation,
            observed_at_utc,
            instrument,
            eligible,
        } => (
            "instrument_specification",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([
                ("instrument".to_owned(), instrument),
                ("eligible".to_owned(), exact_bool(eligible)),
            ]),
        ),
        OperationalAuthorityRecord::LifecycleClarity {
            schema_version,
            generation,
            observed_at_utc,
            clear,
        } => (
            "ambiguity_orphan_unresolved_lifecycle",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([("clear".to_owned(), exact_bool(clear))]),
        ),
        OperationalAuthorityRecord::DurableMicroBudget {
            schema_version,
            generation,
            observed_at_utc,
            available,
            durable_budget_generation,
        } => (
            "durable_micro_budget",
            schema_version,
            generation,
            observed_at_utc,
            BTreeMap::from([
                ("available".to_owned(), exact_bool(available)),
                (
                    "durable_budget_generation".to_owned(),
                    durable_budget_generation,
                ),
            ]),
        ),
    };
    if source != expected_source || schema != 1 || generation == 0 {
        return Err(R2a3Error::Provenance);
    }
    Ok((generation, observed_at, claims))
}

fn controlled_operational_authority(
    source: &str,
    generation: u64,
    observed_at_utc: DateTime<Utc>,
    claims: &BTreeMap<String, String>,
) -> Result<OperationalAuthorityRecord, R2a3Error> {
    let claim = |name: &str| claims.get(name).cloned().ok_or(R2a3Error::Input);
    let boolean = |name: &str| match claims.get(name).map(String::as_str) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(R2a3Error::Input),
    };
    match source {
        "stage7b_current_recovery_seal" => Ok(OperationalAuthorityRecord::Stage7bRecoverySeal {
            schema_version: 1,
            generation,
            observed_at_utc,
            stage7b_seal_generation: claim("stage7b_seal_generation")?
                .parse()
                .map_err(|_| R2a3Error::Input)?,
            stage6_checkpoint_fingerprint: claim("stage6_checkpoint_fingerprint")?,
        }),
        "stage6_exact_dispatch_ready_command" => {
            Ok(OperationalAuthorityRecord::Stage6DispatchReadyCommand {
                schema_version: 1,
                generation,
                observed_at_utc,
                strategy_request_id: claim("strategy_request_id")?,
                durable_client_order_id: claim("durable_client_order_id")?,
                operation: claim("operation")?,
                request_body_sha256: claim("request_body_sha256")?,
                cancel_target_broker_order_id: claims.get("cancel_target_broker_order_id").cloned(),
                cancel_target_lifecycle_fingerprint: claims
                    .get("cancel_target_lifecycle_fingerprint")
                    .cloned(),
                cancel_target_currently_working_proof_sha256: claims
                    .get("cancel_target_currently_working_proof_sha256")
                    .cloned(),
            })
        }
        "stage8a_root_config_policy_control" => {
            Ok(OperationalAuthorityRecord::Stage8aRootControl {
                schema_version: 1,
                generation,
                observed_at_utc,
                config_sha256: claim("config_sha256")?,
                policy_sha256: claim("policy_sha256")?,
                config_policy_authority_sha256: claim("config_policy_authority_sha256")?,
            })
        }
        "composite_readiness" => Ok(OperationalAuthorityRecord::CompositeReadiness {
            schema_version: 1,
            generation,
            observed_at_utc,
            ready: boolean("ready")?,
        }),
        "kill_switch_run_allowed" => Ok(OperationalAuthorityRecord::KillSwitch {
            schema_version: 1,
            generation,
            observed_at_utc,
            run_allowed: boolean("run_allowed")?,
            kill_switch_generation: claim("kill_switch_generation")?,
        }),
        "single_finam_ownership" => Ok(OperationalAuthorityRecord::SingleFinamOwnership {
            schema_version: 1,
            generation,
            observed_at_utc,
            single_owner: boolean("single_owner")?,
            ownership_lease_fingerprint: claim("ownership_lease_fingerprint")?,
        }),
        "schedule" => Ok(OperationalAuthorityRecord::Schedule {
            schema_version: 1,
            generation,
            observed_at_utc,
            eligible: boolean("eligible")?,
        }),
        "instrument_specification" => Ok(OperationalAuthorityRecord::InstrumentSpecification {
            schema_version: 1,
            generation,
            observed_at_utc,
            instrument: claim("instrument")?,
            eligible: boolean("eligible")?,
        }),
        "ambiguity_orphan_unresolved_lifecycle" => {
            Ok(OperationalAuthorityRecord::LifecycleClarity {
                schema_version: 1,
                generation,
                observed_at_utc,
                clear: boolean("clear")?,
            })
        }
        "durable_micro_budget" => Ok(OperationalAuthorityRecord::DurableMicroBudget {
            schema_version: 1,
            generation,
            observed_at_utc,
            available: boolean("available")?,
            durable_budget_generation: claim("durable_budget_generation")?,
        }),
        _ => Err(R2a3Error::Input),
    }
}

fn atomic_write_owned(
    path: &Path,
    bytes: &[u8],
    expected_parent_uid: u32,
) -> Result<(), R2a3Error> {
    let parent = path.parent().ok_or(R2a3Error::Input)?;
    let metadata = parent.metadata()?;
    if !metadata.is_dir() || metadata.uid() != expected_parent_uid || metadata.mode() & 0o022 != 0 {
        return Err(R2a3Error::Input);
    }
    let temporary = parent.join(format!(".r2a5.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn produce_from_store_at(
    source: &str,
    upstream_root: &Path,
    state_root: &Path,
    run_root: &Path,
    executable_sha256: &str,
    expected_uid: u32,
) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != expected_uid
        || expected_uid != r2a3::source_producer_uid(source)?
    {
        return Err(R2a3Error::Provenance);
    }
    decode_hex::<32>(executable_sha256)?;
    let produced_at = Utc::now();
    let (store_bytes, store_generation, source_observed_at_utc, claims) = if source
        == "trusted_clock"
    {
        let boot_id = strict_single_line(&std::fs::read("/proc/sys/kernel/random/boot_id")?, 128)?;
        let boot_sha = sha256(boot_id.as_bytes());
        let generation_bytes = decode_hex::<32>(&boot_sha)?;
        let mut generation = u64::from_be_bytes(
            generation_bytes[..8]
                .try_into()
                .map_err(|_| R2a3Error::Provenance)?,
        );
        if generation == 0 {
            generation = 1;
        }
        let claims = BTreeMap::from([
            (
                "trusted_now_utc".to_owned(),
                r2a2::exact_millis(produced_at),
            ),
            ("process_boot_fingerprint_sha256".to_owned(), boot_sha),
        ]);
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "source_name": "trusted_clock",
            "generation": generation,
            "source_observed_at_utc": r2a2::exact_millis(produced_at),
            "claims": claims,
        }))?;
        (Zeroizing::new(bytes), generation, produced_at, claims)
    } else {
        let store_path = upstream_root.join(operational_authority_file(source)?);
        let bytes = read_owned_fd(&store_path, 128 * 1024, R2A6_SOURCE_ADAPTER_UID, false)?;
        let record: OperationalAuthorityRecord = serde_json::from_slice(&bytes)?;
        let (generation, observed_at, claims) = reduce_operational_authority(source, record)?;
        (bytes, generation, observed_at, claims)
    };
    let nonce = strict_single_line(
        &read_owned_fd(&run_root.join("run-nonce.sha256"), 128, 0, false)?,
        128,
    )?;
    decode_hex::<32>(&nonce)?;
    validate_source_freshness(source, source_observed_at_utc, produced_at)?;
    let snapshot = r2a3::AuthoritySourceSnapshot {
        schema_version: 1,
        source_name: source.to_owned(),
        producer_service: format!("moex-stage8b-r2a5-source-{source}.service"),
        producer_uid: expected_uid,
        source_generation: store_generation,
        producer_executable_sha256: executable_sha256.to_owned(),
        authoritative_store_sha256: sha256(&store_bytes),
        run_nonce_sha256: nonce,
        source_observed_at_utc,
        produced_at_utc: produced_at,
        claims,
    };
    let generation_path = state_root
        .join("authority-sources")
        .join(source)
        .join("generations")
        .join(store_generation.to_string());
    let mut generation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(generation_path)
        .map_err(|_| R2a3Error::Provenance)?;
    generation.write_all(b"stage8b-r2a5-source-generation-consumed-v1\n")?;
    generation.sync_all()?;
    atomic_write_owned(
        &state_root
            .join("authority-sources")
            .join(source)
            .join("source.json"),
        &serde_json::to_vec(&snapshot)?,
        expected_uid,
    )
}

pub fn produce_from_fixed_store(source: &str) -> Result<(), R2a3Error> {
    if !source_names().contains(source) {
        return Err(R2a3Error::Input);
    }
    produce_from_store_at(
        source,
        Path::new(PRODUCTION_UPSTREAM_ROOT),
        Path::new(PRODUCTION_ROOT),
        Path::new(PRODUCTION_RUN),
        &current_linux_executable_sha256()?,
        r2a3::source_producer_uid(source)?,
    )
}

pub fn produce_for_effective_uid(requested_source: Option<&str>) -> Result<(), R2a3Error> {
    let effective_uid = unsafe { libc::geteuid() };
    let source = source_names()
        .into_iter()
        .find(|source| r2a3::source_producer_uid(source).ok() == Some(effective_uid))
        .ok_or(R2a3Error::Provenance)?;
    if requested_source.is_some_and(|requested| requested != source) {
        return Err(R2a3Error::Provenance);
    }
    produce_from_fixed_store(source)
}

fn validate_r2a5_source_snapshot(
    snapshot: &r2a3::AuthoritySourceSnapshot,
    source: &str,
    nonce: &str,
) -> Result<(), R2a3Error> {
    let expected_uid = r2a3::source_producer_uid(source)?;
    let mut expected_claims = r2a3::expected_claim_names(source)?;
    if source == "stage6_exact_dispatch_ready_command"
        && snapshot.claims.get("operation").map(String::as_str) == Some("PLACE")
    {
        expected_claims.remove("cancel_target_broker_order_id");
        expected_claims.remove("cancel_target_lifecycle_fingerprint");
        expected_claims.remove("cancel_target_currently_working_proof_sha256");
    }
    if snapshot.schema_version != 1
        || snapshot.source_name != source
        || snapshot.producer_service != format!("moex-stage8b-r2a5-source-{source}.service")
        || snapshot.producer_uid != expected_uid
        || snapshot.source_generation == 0
        || snapshot.run_nonce_sha256 != nonce
        || decode_hex::<32>(&snapshot.producer_executable_sha256).is_err()
        || decode_hex::<32>(&snapshot.authoritative_store_sha256).is_err()
        || snapshot
            .claims
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected_claims
    {
        return Err(R2a3Error::Provenance);
    }
    validate_source_freshness(
        source,
        snapshot.source_observed_at_utc,
        snapshot.produced_at_utc,
    )?;
    Ok(())
}

fn issue_from_source_at(
    source: &str,
    etc_root: &Path,
    state_root: &Path,
    run_root: &Path,
    credentials_root: &Path,
    expected_uid: u32,
) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != expected_uid
        || expected_uid != r2a3::source_issuer_uid(source)?
    {
        return Err(R2a3Error::Provenance);
    }
    let trust_bytes = read_owned_fd(&etc_root.join("trust-manifest.json"), 128 * 1024, 0, false)?;
    let trust: TrustSetManifest = serde_json::from_slice(&trust_bytes)?;
    let pinned = trust
        .source_keys
        .get(source)
        .ok_or(R2a3Error::Authorization)?;
    let key_text = strict_single_line(
        &read_owned_fd(
            &credentials_root
                .join("issuer-private-keys")
                .join(source)
                .join("key.ed25519"),
            128,
            expected_uid,
            true,
        )?,
        128,
    )?;
    let signing = SigningKey::from_bytes(&decode_hex::<32>(&key_text)?);
    let verifying = signing.verifying_key().to_bytes();
    if lower_hex(&verifying) != pinned.public_key_ed25519_hex
        || sha256(&verifying) != pinned.public_key_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    let nonce = strict_single_line(
        &read_owned_fd(&run_root.join("run-nonce.sha256"), 128, 0, false)?,
        128,
    )?;
    let source_path = state_root
        .join("authority-sources")
        .join(source)
        .join("source.json");
    let source_bytes = read_owned_fd(
        &source_path,
        128 * 1024,
        r2a3::source_producer_uid(source)?,
        false,
    )?;
    let snapshot: r2a3::AuthoritySourceSnapshot = serde_json::from_slice(&source_bytes)?;
    validate_r2a5_source_snapshot(&snapshot, source, &nonce)?;
    let manifest = read_owned_fd(&state_root.join("run-manifest.json"), 256 * 1024, 0, false)?;
    let fields: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    let receipt = r2a2::LocalAuthorityReceipt {
        source_name: source.to_owned(),
        issuer: match source {
            "trusted_clock" => "Stage8bTrustedClockIssuer",
            "stage7b_current_recovery_seal" => "Stage7bRecoverySealReader",
            "stage6_exact_dispatch_ready_command" => "Stage6DispatchReadyCommandReader",
            "stage8a_root_config_policy_control" => "Stage8aCurrentControlIssuer",
            "composite_readiness" => "Stage8aCompositeReadinessIssuer",
            "kill_switch_run_allowed" => "Stage8aPersistentKillSwitchIssuer",
            "single_finam_ownership" => "Stage8aSingleFinamOwnershipIssuer",
            "schedule" => "Stage8aScheduleIssuer",
            "instrument_specification" => "Stage8aInstrumentIssuer",
            "ambiguity_orphan_unresolved_lifecycle" => "Stage8aLifecycleAmbiguityIssuer",
            "durable_micro_budget" => "Stage8aDurableMicroBudgetIssuer",
            _ => return Err(R2a3Error::Input),
        }
        .to_owned(),
        evidence_schema: match source {
            "trusted_clock" => "stage8b-trusted-clock-v1",
            "stage7b_current_recovery_seal" => "stage7b-current-recovery-seal-v1",
            "stage6_exact_dispatch_ready_command" => "stage6-dispatch-ready-command-v1",
            "stage8a_root_config_policy_control" => "stage8a-root-config-policy-control-v1",
            "composite_readiness" => "stage8a-composite-readiness-v1",
            "kill_switch_run_allowed" => "stage8a-kill-switch-run-allowed-v1",
            "single_finam_ownership" => "stage8a-single-finam-ownership-v1",
            "schedule" => "stage8a-schedule-window-v1",
            "instrument_specification" => "stage8a-instrument-specification-v1",
            "ambiguity_orphan_unresolved_lifecycle" => "stage8a-lifecycle-ambiguity-v1",
            "durable_micro_budget" => "stage8a-durable-micro-budget-v1",
            _ => return Err(R2a3Error::Input),
        }
        .to_owned(),
        observed_at_utc: snapshot.source_observed_at_utc,
        key_generation_id: pinned.generation.to_string(),
        run_identity_sha256: manifest_field(&fields, "run_identity_sha256")?.to_owned(),
        keyed_account_binding_hmac_sha256: manifest_field(
            &fields,
            "keyed_account_binding_hmac_sha256",
        )?
        .to_owned(),
        execution_build_identity_sha256: manifest_field(
            &fields,
            "execution_build_identity_sha256",
        )?
        .to_owned(),
        claims: snapshot.claims,
        authentication_tag_hmac_sha256: String::new(),
    };
    let signed = r2a3::sign_authority_receipt(
        SignedAuthorityReceipt {
            receipt,
            run_nonce_sha256: nonce,
            source_snapshot_sha256: sha256(&source_bytes),
            source_generation: snapshot.source_generation,
            producer_executable_sha256: snapshot.producer_executable_sha256,
            issuer_executable_sha256: current_linux_executable_sha256()?,
            authoritative_store_sha256: snapshot.authoritative_store_sha256,
            source_observed_at_utc: snapshot.source_observed_at_utc,
            produced_at_utc: snapshot.produced_at_utc,
            issuer_key_id: pinned.key_id.clone(),
            signature_ed25519_hex: String::new(),
        },
        &signing,
    )?;
    atomic_write_owned(
        &run_root.join("receipts").join(source).join("receipt.json"),
        &serde_json::to_vec(&signed)?,
        expected_uid,
    )
}

pub fn issue_from_fixed_source(source: &str) -> Result<(), R2a3Error> {
    if !source_names().contains(source) {
        return Err(R2a3Error::Input);
    }
    issue_from_source_at(
        source,
        Path::new(PRODUCTION_ETC),
        Path::new(PRODUCTION_ROOT),
        Path::new(PRODUCTION_RUN),
        Path::new(PRODUCTION_CREDENTIALS),
        r2a3::source_issuer_uid(source)?,
    )
}

pub fn issue_for_effective_uid(requested_source: Option<&str>) -> Result<(), R2a3Error> {
    let effective_uid = unsafe { libc::geteuid() };
    let source = source_names()
        .into_iter()
        .find(|source| r2a3::source_issuer_uid(source).ok() == Some(effective_uid))
        .ok_or(R2a3Error::Provenance)?;
    if requested_source.is_some_and(|requested| requested != source) {
        return Err(R2a3Error::Provenance);
    }
    issue_from_fixed_source(source)
}

fn current_linux_executable_sha256() -> Result<String, R2a3Error> {
    #[cfg(target_os = "linux")]
    {
        Ok(sha256(&std::fs::read("/proc/self/exe")?))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(R2a3Error::Authorization)
    }
}

fn claim_nonce(directory: &Path, nonce: &str) -> Result<(), R2a3Error> {
    decode_hex::<32>(nonce)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err(R2a3Error::Authorization);
    }
    let path = directory.join(nonce);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| R2a3Error::Authorization)?;
    file.write_all(b"stage8b-p-r2a5-run-nonce-consumed-v1\n")?;
    file.sync_all()?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn manifest_field<'a>(
    fields: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, R2a3Error> {
    fields
        .get(name)
        .map(String::as_str)
        .ok_or(R2a3Error::Authorization)
}

fn validate_local_package_at(
    etc_root: &Path,
    state_root: &Path,
    run_root: &Path,
    credentials_root: &Path,
    now: DateTime<Utc>,
    executable_sha256: &str,
    accepted: &AcceptedR2a5Authority,
) -> Result<PreparedR2a5Run, R2a3Error> {
    if accepted.schema_version != 1
        || accepted.stage != "8B-P"
        || accepted.revision != "R2A5"
        || accepted.authorization_status != "NOT_ISSUED"
    {
        return Err(R2a3Error::Authorization);
    }
    let trust_bytes = read_owned_fd(&etc_root.join("trust-manifest.json"), 128 * 1024, 0, false)?;
    let trust: TrustSetManifest = serde_json::from_slice(&trust_bytes)?;
    let accepted_helper = load_accepted_helper_authority(etc_root, &trust, now)?;
    if executable_sha256 != accepted_helper.helper_executable_sha256 {
        return Err(R2a3Error::Authorization);
    }
    let package_bytes =
        read_owned_fd(&etc_root.join("r2b-run-package.json"), 128 * 1024, 0, false)?;
    let package: R2a5RunPackage = serde_json::from_slice(&package_bytes)?;
    if package.package_version != 1
        || package.authorization_status != "ISSUED"
        || now < package.issued_at_utc
        || now >= package.expires_at_utc
        || package
            .expires_at_utc
            .signed_duration_since(package.issued_at_utc)
            .num_seconds()
            > 60
        || package.helper_executable_sha256 != executable_sha256
        || package.effect_build_identity_sha256 != accepted_helper.effect_build_identity_sha256
        || package.contract_snapshot_sha256 != sha256(READ_CONTRACT_SNAPSHOT)
        || package.source_adapter_authority_sha256 != sha256(SOURCE_ADAPTER_AUTHORITY)
        || package.source_adapter_authority_sha256 != accepted.source_adapter_authority_sha256
    {
        return Err(R2a3Error::Authorization);
    }

    if trust.schema_version != 1
        || trust.environment != "production"
        || !trust.rotation_requires_new_reviewed_package
        || sha256(&trust_bytes) != package.trust_manifest_sha256
        || package.trust_manifest_sha256 != accepted.trust_manifest_sha256
        || public_key_set_digest(&trust)? != trust.public_key_set_sha256
        || package.public_key_set_sha256 != trust.public_key_set_sha256
        || package.public_key_set_sha256 != accepted.public_key_set_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    let authorization_key = validate_pinned_key(&trust.authorization_key, now)?;
    if trust.authorization_key.public_key_sha256 != accepted.authorization_public_key_sha256
        || package.authorization_key_id != trust.authorization_key.key_id
    {
        return Err(R2a3Error::Authorization);
    }
    let signature = Signature::from_bytes(&decode_hex::<64>(&package.signature_ed25519_hex)?);
    authorization_key
        .verify(&package_preimage(&package)?, &signature)
        .map_err(|_| R2a3Error::Authorization)?;

    let manifest = read_owned_fd(&state_root.join("run-manifest.json"), 256 * 1024, 0, false)?;
    let fields: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    if sha256(&manifest) != package.manifest_sha256
        || manifest_field(&fields, "run_identity_sha256")? != package.run_identity_sha256
        || manifest_field(&fields, "keyed_account_binding_hmac_sha256")?
            != package.keyed_account_binding_hmac_sha256
        || manifest_field(&fields, "account_key_generation_id")?
            != package.account_key_generation_id
        || manifest_field(&fields, "execution_build_identity_sha256")?
            != package.effect_build_identity_sha256
        || manifest_field(&fields, "operation")? != exact_operation(package.operation)
    {
        return Err(R2a3Error::Authorization);
    }

    let receipts = load_receipts(run_root, &package.run_nonce_sha256)?;
    let envelope: SignedAuthorityEnvelope = serde_json::from_slice(&receipts)?;
    if source_generation_commitment(&envelope.receipts)?
        != package.source_generation_commitment_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    for signed in &envelope.receipts {
        let pinned = trust
            .source_keys
            .get(&signed.receipt.source_name)
            .ok_or(R2a3Error::Authorization)?;
        if signed.issuer_key_id != pinned.key_id
            || signed.receipt.key_generation_id != pinned.generation.to_string()
        {
            return Err(R2a3Error::Authorization);
        }
    }
    let public_keys = load_source_keys(&etc_root.join("authority-public-keys"), &trust, now)?;
    let validated: (ValidatedManifest, _) = r2a3::validate_signed_authorities(
        &manifest,
        &receipts,
        &public_keys,
        &package.run_nonce_sha256,
        now,
    )?;
    if validated.0.run_identity_sha256 != package.run_identity_sha256 {
        return Err(R2a3Error::Authorization);
    }

    let account_manifest_bytes = read_owned_fd(
        &etc_root.join("account-key-manifest.json"),
        64 * 1024,
        0,
        false,
    )?;
    let account_manifest: AccountKeyManifest = serde_json::from_slice(&account_manifest_bytes)?;
    if account_manifest.schema_version != 1
        || sha256(&account_manifest_bytes) != package.account_key_manifest_sha256
        || package.account_key_manifest_sha256 != accepted.account_key_manifest_sha256
    {
        return Err(R2a3Error::Authorization);
    }
    let key_entry = account_manifest
        .entries
        .iter()
        .find(|entry| entry.generation_id == package.account_key_generation_id)
        .ok_or(R2a3Error::Authorization)?;
    if now < key_entry.valid_from_utc
        || now >= key_entry.valid_until_utc
        || key_entry.relative_key_path.contains('/')
        || key_entry.relative_key_path.contains("..")
    {
        return Err(R2a3Error::Authorization);
    }
    let account_key_text = strict_single_line(
        &read_owned_fd(
            &credentials_root
                .join("account-binding-keys")
                .join(&key_entry.relative_key_path),
            128,
            unsafe { libc::geteuid() },
            true,
        )?,
        128,
    )?;
    let account_key = decode_hex::<32>(&account_key_text)?;
    if sha256(&account_key) != key_entry.key_sha256 {
        return Err(R2a3Error::Authorization);
    }
    let operator_decision = read_owned_fd(
        &etc_root.join("operator-decision.json"),
        64 * 1024,
        0,
        false,
    )?;
    if sha256(&operator_decision) != package.operator_decision_sha256 {
        return Err(R2a3Error::Authorization);
    }
    let account_id = Zeroizing::new(strict_single_line(
        &read_owned_fd(
            &credentials_root.join("account-id"),
            4096,
            unsafe { libc::geteuid() },
            true,
        )?,
        4096,
    )?);
    let secret = Zeroizing::new(strict_single_line(
        &read_owned_fd(
            &credentials_root.join("finam-readonly-secret"),
            4096,
            unsafe { libc::geteuid() },
            true,
        )?,
        4096,
    )?);
    r2a2::verify_account_binding(
        &validated.0,
        &account_id,
        &package.account_key_generation_id,
        &account_key,
    )?;
    Ok(PreparedR2a5Run {
        package,
        manifest,
        receipts,
        public_keys,
        account_id,
        account_key: Zeroizing::new(account_key),
        secret,
    })
}

pub async fn run_r2b_one_shot() -> Result<R2a3ReadonlyEvidence, R2a3Error> {
    let executable = current_linux_executable_sha256()?;
    let accepted: AcceptedR2a5Authority = serde_json::from_str(AUTHORITY)?;
    let prepared = validate_local_package_at(
        Path::new(PRODUCTION_ETC),
        Path::new(PRODUCTION_ROOT),
        Path::new(PRODUCTION_RUN),
        Path::new(PRODUCTION_CREDENTIALS),
        Utc::now(),
        &executable,
        &accepted,
    )?;
    // The nonce is consumed only after every local package, manifest, receipt,
    // trust, account-key and credential check and immediately before clients.
    claim_nonce(
        &Path::new(PRODUCTION_ROOT).join("used-run-nonces"),
        &prepared.package.run_nonce_sha256,
    )?;
    let (auth_client, broker_client) =
        crate::production_clients().map_err(|_| R2a3Error::Network)?;
    r2a3::execute_r2a3_pipeline(
        &auth_client,
        &broker_client,
        r2a3::PRODUCTION_BASE_URL,
        R2a3PipelineInput {
            manifest: &prepared.manifest,
            signed_authorities: &prepared.receipts,
            public_keys: &prepared.public_keys,
            run_nonce_sha256: &prepared.package.run_nonce_sha256,
            account_id: &prepared.account_id,
            account_key: &prepared.account_key[..],
            secret: &prepared.secret,
            authorization_status: "ISSUED",
        },
    )
    .await
}

fn controlled_client_from_fixed_files() -> Result<(reqwest::Client, String), R2a3Error> {
    let endpoint = strict_single_line(
        &read_owned_fd(Path::new(CONTROLLED_ENDPOINT_PATH), 256, 0, false)?,
        256,
    )?;
    let url = reqwest::Url::parse(&endpoint).map_err(|_| R2a3Error::Input)?;
    if url.scheme() != "https"
        || url.host_str() != Some(CONTROLLED_HOST)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(R2a3Error::Authorization);
    }
    let port = url.port().ok_or(R2a3Error::Authorization)?;
    let root_der = read_owned_fd(Path::new(CONTROLLED_CA_PATH), 64 * 1024, 0, false)?;
    let root = reqwest::Certificate::from_der(&root_der).map_err(|_| R2a3Error::Input)?;
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let client = crate::hardened_client_builder(true, Duration::from_secs(2))
        .tls_built_in_root_certs(false)
        .add_root_certificate(root)
        .resolve(CONTROLLED_HOST, address)
        .build()
        .map_err(|_| R2a3Error::Input)?;
    Ok((client, endpoint))
}

/// Controlled-only exact fixed-layout entry used by the Linux namespace
/// rehearsal. It is intentionally separate from `--r2b-one-shot` and trusts
/// only the compile-time controlled authority plus loopback TLS endpoint.
pub async fn run_controlled_fixed_layout() -> Result<R2a3ReadonlyEvidence, R2a3Error> {
    let executable = current_linux_executable_sha256()?;
    let accepted: AcceptedR2a5Authority = serde_json::from_str(CONTROLLED_AUTHORITY)?;
    let prepared = validate_local_package_at(
        Path::new(PRODUCTION_ETC),
        Path::new(PRODUCTION_ROOT),
        Path::new(PRODUCTION_RUN),
        Path::new(PRODUCTION_CREDENTIALS),
        Utc::now(),
        &executable,
        &accepted,
    )?;
    claim_nonce(
        &Path::new(PRODUCTION_ROOT).join("used-run-nonces"),
        &prepared.package.run_nonce_sha256,
    )?;
    let (client, endpoint) = controlled_client_from_fixed_files()?;
    r2a3::execute_r2a3_pipeline(
        &client,
        &client,
        &endpoint,
        R2a3PipelineInput {
            manifest: &prepared.manifest,
            signed_authorities: &prepared.receipts,
            public_keys: &prepared.public_keys,
            run_nonce_sha256: &prepared.package.run_nonce_sha256,
            account_id: &prepared.account_id,
            account_key: &prepared.account_key[..],
            secret: &prepared.secret,
            authorization_status: "ISSUED",
        },
    )
    .await
}

/// Serves one complete controlled PLACE or CANCEL sequence and publishes its
/// loopback-only endpoint and synthetic CA at fixed root-owned paths.
pub async fn serve_controlled_tls_once(operation: Operation) -> Result<(), R2a3Error> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(R2a3Error::Authorization);
    }
    let manifest = read_owned_fd(
        &Path::new(PRODUCTION_ROOT).join("run-manifest.json"),
        256 * 1024,
        0,
        false,
    )?;
    let manifest_fields: BTreeMap<String, String> = serde_json::from_slice(&manifest)?;
    if manifest_field(&manifest_fields, "operation")? != exact_operation(operation) {
        return Err(R2a3Error::Authorization);
    }
    let controlled_cancel = if operation == Operation::Cancel {
        Some(r2a3::controlled_cancel_order_for(
            manifest_field(&manifest_fields, "cancel_target_broker_order_id")?,
            manifest_field(&manifest_fields, "cancel_target_durable_client_order_id")?,
        ))
    } else {
        None
    };
    let controlled_cancel_path = controlled_cancel.as_ref().map(|_| {
        format!(
            "/orders/{}",
            manifest_fields["cancel_target_broker_order_id"]
        )
    });
    let now = Utc::now();
    let (root_der, tls_config) = r2a3::controlled_tls_configuration(CONTROLLED_HOST)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    write_seed_file(Path::new(CONTROLLED_CA_PATH), &root_der, 0, 0o644)?;
    write_seed_file(
        Path::new(CONTROLLED_ENDPOINT_PATH),
        format!("https://{CONTROLLED_HOST}:{}/\n", address.port()).as_bytes(),
        0,
        0o644,
    )?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let request_count = match operation {
        Operation::Place => 5,
        Operation::Cancel => 6,
    };
    for _ in 0..request_count {
        let (socket, _) = listener.accept().await?;
        let mut tls = acceptor
            .accept(socket)
            .await
            .map_err(|_| R2a3Error::Network)?;
        let mut bytes = [0u8; 16 * 1024];
        let count = tls.read(&mut bytes).await?;
        let request = String::from_utf8_lossy(&bytes[..count]);
        let first = request.lines().next().ok_or(R2a3Error::Network)?;
        let body = if first.starts_with("POST /v1/sessions/details ") {
            serde_json::json!({
                "created_at": (now - chrono::Duration::minutes(1)).to_rfc3339(),
                "expires_at": (now + chrono::Duration::minutes(5)).to_rfc3339(),
                "md_permissions": [],
                "account_ids": [r2a3::CONTROLLED_ACCOUNT],
                "readonly": true
            })
            .to_string()
        } else if first.starts_with("POST /v1/sessions ") {
            serde_json::json!({"token":"controlled-readonly-token"}).to_string()
        } else if first.contains("/trades?") {
            serde_json::json!({"trades":[]}).to_string()
        } else if controlled_cancel_path
            .as_ref()
            .is_some_and(|path| first.contains(path))
        {
            controlled_cancel
                .as_ref()
                .ok_or(R2a3Error::Authorization)?
                .to_string()
        } else if first.ends_with("/orders HTTP/1.1") {
            match operation {
                Operation::Place => serde_json::json!({"orders":[]}).to_string(),
                Operation::Cancel => serde_json::json!({
                    "orders":[controlled_cancel.as_ref().ok_or(R2a3Error::Authorization)?]
                })
                .to_string(),
            }
        } else {
            r2a3::controlled_account_body()
        };
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        tls.write_all(response.as_bytes()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_secret_grammar_allows_only_one_terminal_lf() {
        assert_eq!(strict_single_line(b"exact\n", 16).unwrap(), "exact");
        for invalid in [b" exact".as_slice(), b"exact ", b"exact\n\n", b"exact\r\n"] {
            assert!(strict_single_line(invalid, 16).is_err());
        }
    }

    #[test]
    fn source_generation_commitment_covers_producer_and_store() {
        let observed_at = Utc::now();
        let mut receipt = SignedAuthorityReceipt {
            receipt: r2a2::LocalAuthorityReceipt {
                source_name: "trusted_clock".to_owned(),
                issuer: String::new(),
                evidence_schema: String::new(),
                observed_at_utc: observed_at,
                key_generation_id: "1".to_owned(),
                run_identity_sha256: "1".repeat(64),
                keyed_account_binding_hmac_sha256: "2".repeat(64),
                execution_build_identity_sha256: "3".repeat(64),
                claims: BTreeMap::new(),
                authentication_tag_hmac_sha256: String::new(),
            },
            run_nonce_sha256: "4".repeat(64),
            source_snapshot_sha256: "5".repeat(64),
            source_generation: 1,
            producer_executable_sha256: "6".repeat(64),
            issuer_executable_sha256: "7".repeat(64),
            authoritative_store_sha256: "8".repeat(64),
            source_observed_at_utc: observed_at,
            produced_at_utc: observed_at,
            issuer_key_id: "trusted_clock-ed25519-v1".to_owned(),
            signature_ed25519_hex: "8".repeat(128),
        };
        let mut receipts = Vec::new();
        for source in source_names() {
            receipt.receipt.source_name = source.to_owned();
            receipts.push(receipt.clone());
        }
        let baseline = source_generation_commitment(&receipts).unwrap();
        receipts[0].authoritative_store_sha256 = "9".repeat(64);
        assert_ne!(source_generation_commitment(&receipts).unwrap(), baseline);
    }

    #[test]
    fn stale_source_cannot_be_laundered_by_fresh_producer_time() {
        let produced = Utc::now();
        assert!(matches!(
            validate_source_freshness(
                "composite_readiness",
                produced - chrono::Duration::milliseconds(1_001),
                produced,
            ),
            Err(R2a3Error::Freshness)
        ));
        assert!(validate_source_freshness(
            "composite_readiness",
            produced - chrono::Duration::milliseconds(1_000),
            produced,
        )
        .is_ok());
    }

    #[test]
    fn future_source_beyond_budget_is_rejected() {
        let produced = Utc::now();
        assert!(matches!(
            validate_source_freshness(
                "schedule",
                produced + chrono::Duration::milliseconds(251),
                produced,
            ),
            Err(R2a3Error::Freshness)
        ));
    }

    #[test]
    fn source_timestamp_substitution_fails_even_with_valid_source_signature() {
        let now = Utc::now();
        let (manifest, envelope, keys, nonce) =
            r2a3::controlled_fixture_for(now, Operation::Place).unwrap();
        let mut signed: SignedAuthorityEnvelope = serde_json::from_slice(&envelope).unwrap();
        let mut substituted = signed.receipts.remove(0);
        substituted.source_observed_at_utc += chrono::Duration::milliseconds(1);
        substituted.signature_ed25519_hex.clear();
        signed.receipts.insert(
            0,
            r2a3::sign_authority_receipt(substituted, &SigningKey::from_bytes(&[1u8; 32])).unwrap(),
        );
        assert!(r2a3::validate_signed_authorities(
            &manifest,
            &serde_json::to_vec(&signed).unwrap(),
            &keys,
            &nonce,
            now,
        )
        .is_err());
    }

    #[test]
    fn source_adapter_rejects_wrong_variant_schema_and_generation() {
        let now = Utc::now();
        let wrong_variant = OperationalAuthorityRecord::Schedule {
            schema_version: 1,
            generation: 1,
            observed_at_utc: now,
            eligible: true,
        };
        assert!(reduce_operational_authority("instrument_specification", wrong_variant).is_err());
        for (schema_version, generation) in [(2, 1), (1, 0)] {
            let record = OperationalAuthorityRecord::Schedule {
                schema_version,
                generation,
                observed_at_utc: now,
                eligible: true,
            };
            assert!(reduce_operational_authority("schedule", record).is_err());
        }
    }

    #[test]
    fn signed_package_rejects_selected_run_substitution() {
        let issued = Utc::now();
        let key = SigningKey::from_bytes(&[42u8; 32]);
        let package = sign_run_package(
            R2a5RunPackage {
                package_version: 1,
                authorization_status: "ISSUED".to_owned(),
                issued_at_utc: issued,
                expires_at_utc: issued + chrono::Duration::seconds(30),
                operation: Operation::Cancel,
                run_nonce_sha256: "1".repeat(64),
                run_identity_sha256: "2".repeat(64),
                manifest_sha256: "3".repeat(64),
                keyed_account_binding_hmac_sha256: "4".repeat(64),
                account_key_generation_id: "1".to_owned(),
                account_key_manifest_sha256: "5".repeat(64),
                effect_build_identity_sha256: "6".repeat(64),
                helper_executable_sha256: "7".repeat(64),
                contract_snapshot_sha256: "8".repeat(64),
                source_adapter_authority_sha256: "d".repeat(64),
                trust_manifest_sha256: "9".repeat(64),
                public_key_set_sha256: "a".repeat(64),
                source_generation_commitment_sha256: "b".repeat(64),
                operator_decision_sha256: "c".repeat(64),
                authorization_key_id: "package-key-v1".to_owned(),
                signature_ed25519_hex: String::new(),
            },
            &key,
        )
        .unwrap();
        let signature =
            Signature::from_bytes(&decode_hex::<64>(&package.signature_ed25519_hex).unwrap());
        key.verifying_key()
            .verify(&package_preimage(&package).unwrap(), &signature)
            .unwrap();
        type PackageMutation = Box<dyn Fn(&mut R2a5RunPackage)>;
        let mutations: Vec<PackageMutation> = vec![
            Box::new(|value| value.manifest_sha256 = "d".repeat(64)),
            Box::new(|value| value.run_identity_sha256 = "d".repeat(64)),
            Box::new(|value| value.keyed_account_binding_hmac_sha256 = "d".repeat(64)),
            Box::new(|value| value.account_key_generation_id = "2".to_owned()),
            Box::new(|value| value.public_key_set_sha256 = "d".repeat(64)),
            Box::new(|value| value.helper_executable_sha256 = "d".repeat(64)),
            Box::new(|value| value.source_adapter_authority_sha256 = "e".repeat(64)),
            Box::new(|value| value.source_generation_commitment_sha256 = "d".repeat(64)),
            Box::new(|value| value.operator_decision_sha256 = "d".repeat(64)),
            Box::new(|value| value.operation = Operation::Place),
            Box::new(|value| value.expires_at_utc += chrono::Duration::seconds(1)),
        ];
        for mutate in mutations {
            let mut forged = package.clone();
            mutate(&mut forged);
            assert!(key
                .verifying_key()
                .verify(&package_preimage(&forged).unwrap(), &signature)
                .is_err());
        }
    }
}
