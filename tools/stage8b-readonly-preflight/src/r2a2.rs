//! Stage 8B-P R2A2 semantic/provenance qualification.
//!
//! This module deliberately grants no arm, dispatch, transport, or order
//! authority. It validates an accepted R1B run against authenticated local
//! receipts, then reduces fresh read-only FINAM responses to redacted facts.

use crate::{digest_parts, is_lower_sha256, Operation, PreflightError};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

const AUTHORIZATION_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r1a-authorization-authority.json");
const RUN_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r1b-run-identity-authority.json");
const ENDPOINT_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json");

pub const TARGET_INSTRUMENT: &str = "IMOEXF@RTSX";
pub const LOCAL_RECEIPT_SCHEMA: u8 = 1;
pub const LOCAL_RECEIPT_DOMAIN: &str = "stage8b-p-r2a2-local-authority-receipt-v1";
pub const LOCAL_RECEIPT_KEY_GENERATION_ID: &str = "1";
pub const ACCOUNT_HMAC_DOMAIN: &[u8] = b"moex-stage8b-account-binding-v1";
pub const ENDPOINT_IDENTITY_DOMAIN: &str = "stage8b-i-r2-endpoint-identity-v1";
pub const MAX_RUN_AHEAD_MS: i64 = 60_000;
pub const MIN_HMAC_KEY_BYTES: usize = 32;

pub const AUTH_BODY_CAP: usize = 64 * 1024;
pub const EXACT_ORDER_BODY_CAP: usize = 256 * 1024;
pub const ORDERS_BODY_CAP: usize = 4 * 1024 * 1024;
pub const TRADES_BODY_CAP: usize = 16 * 1024 * 1024;
pub const ACCOUNT_BODY_CAP: usize = 4 * 1024 * 1024;
pub const PRODUCTION_LOCAL_KEY_DIR: &str =
    "/var/lib/moex-trading/stage8b/r2a2/local-authority-keys";
pub const PRODUCTION_ACCOUNT_KEY_DIR: &str =
    "/var/lib/moex-trading/stage8b/r2a2/account-binding-keys";

const LOCAL_SOURCES: &[(&str, &str, &str, i64)] = &[
    (
        "trusted_clock",
        "Stage8bTrustedClockIssuer",
        "stage8b-trusted-clock-v1",
        1_000,
    ),
    (
        "stage7b_current_recovery_seal",
        "Stage7bRecoverySealReader",
        "stage7b-current-recovery-seal-v1",
        1_000,
    ),
    (
        "stage6_exact_dispatch_ready_command",
        "Stage6DispatchReadyCommandReader",
        "stage6-dispatch-ready-command-v1",
        1_000,
    ),
    (
        "stage8a_root_config_policy_control",
        "Stage8aCurrentControlIssuer",
        "stage8a-root-config-policy-control-v1",
        1_000,
    ),
    (
        "composite_readiness",
        "Stage8aCompositeReadinessIssuer",
        "stage8a-composite-readiness-v1",
        1_000,
    ),
    (
        "kill_switch_run_allowed",
        "Stage8aPersistentKillSwitchIssuer",
        "stage8a-kill-switch-run-allowed-v1",
        1_000,
    ),
    (
        "single_finam_ownership",
        "Stage8aSingleFinamOwnershipIssuer",
        "stage8a-single-finam-ownership-v1",
        1_000,
    ),
    (
        "schedule",
        "Stage8aScheduleIssuer",
        "stage8a-schedule-window-v1",
        5_000,
    ),
    (
        "instrument_specification",
        "Stage8aInstrumentIssuer",
        "stage8a-instrument-specification-v1",
        5_000,
    ),
    (
        "ambiguity_orphan_unresolved_lifecycle",
        "Stage8aLifecycleAmbiguityIssuer",
        "stage8a-lifecycle-ambiguity-v1",
        1_000,
    ),
    (
        "durable_micro_budget",
        "Stage8aDurableMicroBudgetIssuer",
        "stage8a-durable-micro-budget-v1",
        1_000,
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAuthorityReceipt {
    pub source_name: String,
    pub issuer: String,
    pub evidence_schema: String,
    pub observed_at_utc: DateTime<Utc>,
    pub key_generation_id: String,
    pub run_identity_sha256: String,
    pub keyed_account_binding_hmac_sha256: String,
    pub execution_build_identity_sha256: String,
    pub claims: BTreeMap<String, String>,
    pub authentication_tag_hmac_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAuthorityEnvelope {
    pub schema_version: u8,
    pub receipts: Vec<LocalAuthorityReceipt>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalAuthoritySummary {
    pub schema_version: u8,
    pub receipt_count: usize,
    pub receipt_sha256: BTreeMap<String, String>,
    pub broker_derived_sources_accepted_pre_network: bool,
    pub caller_selected_key_path_allowed: bool,
}

#[derive(Debug, Clone)]
pub struct ValidatedManifest {
    fields: BTreeMap<String, String>,
    pub operation: Operation,
    pub run_identity_sha256: String,
    pub broker_order_id: Option<String>,
    pub keyed_account_binding_hmac_sha256: String,
    pub account_key_generation_id: String,
    pub approved_pre_run_position: Decimal,
}

impl ValidatedManifest {
    pub fn field(&self, name: &str) -> &str {
        self.fields.get(name).map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidatedLocalAuthorities {
    pub summary: LocalAuthoritySummary,
    pub trusted_now_utc: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum R2a2Error {
    #[error("accepted R1B authority is malformed")]
    Authority,
    #[error("manifest is self-consistent but is not authorized by R1B")]
    UnauthorizedManifest,
    #[error("local authority receipt provenance is invalid")]
    InvalidReceipt,
    #[error("account binding is invalid")]
    AccountBinding,
    #[error("broker response exceeded its frozen endpoint cap")]
    OversizeResponse,
    #[error("broker response schema or semantic facts are invalid")]
    BrokerTruth,
    #[error("json boundary failed")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct AuthorizationAuthority {
    accepted_execution_build: AcceptedExecutionBuild,
    bound_authorities: BoundAuthorities,
}

#[derive(Debug, Deserialize)]
struct AcceptedExecutionBuild {
    execution_build_identity_sha256: String,
    source_ref: String,
    source_archive_sha256: String,
    executable_sha256: String,
}

#[derive(Debug, Deserialize)]
struct BoundAuthorities {
    freshness_budget_authority_sha256: String,
}

#[derive(Debug, Deserialize)]
struct RunAuthority {
    run_identity: RunIdentity,
    golden_vectors: BTreeMap<String, GoldenVector>,
}

#[derive(Debug, Deserialize)]
struct RunIdentity {
    domain_utf8: String,
    common_fields_in_exact_order_excluding_run_identity: Vec<String>,
    place_fields_in_exact_order: Vec<String>,
    cancel_fields_in_exact_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GoldenVector {
    manifest_without_run_identity_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct EndpointAuthority {
    accepted_endpoint_renderer_sha256: String,
    operations: BTreeMap<String, EndpointOperation>,
}

#[derive(Debug, Deserialize)]
struct EndpointOperation {
    method: String,
    route_template_id: String,
}

fn exact_millis(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn decode_lower_hex(value: &str) -> Result<Vec<u8>, R2a2Error> {
    if !value.len().is_multiple_of(2)
        || value
            .bytes()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(R2a2Error::InvalidReceipt);
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte| match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                _ => 0,
            };
            Ok((nibble(pair[0]) << 4) | nibble(pair[1]))
        })
        .collect()
}

fn decode_key_file(bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, R2a2Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| R2a2Error::InvalidReceipt)?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.contains(['\r', '\n']) {
        return Err(R2a2Error::InvalidReceipt);
    }
    let decoded = decode_lower_hex(text)?;
    if decoded.len() < MIN_HMAC_KEY_BYTES {
        return Err(R2a2Error::InvalidReceipt);
    }
    Ok(Zeroizing::new(decoded))
}

fn required_local_source_names() -> impl Iterator<Item = &'static str> {
    LOCAL_SOURCES.iter().map(|source| source.0)
}

#[allow(dead_code)] // consumed by the independently reviewed R2B fixed-path entry.
pub(crate) fn manifest_account_key_generation(bytes: &[u8]) -> Result<String, R2a2Error> {
    let value: Value = serde_json::from_slice(bytes)?;
    let generation = value
        .as_object()
        .and_then(|object| object.get("account_key_generation_id"))
        .and_then(Value::as_str)
        .filter(|value| canonical_positive_generation(value))
        .ok_or(R2a2Error::UnauthorizedManifest)?;
    Ok(generation.to_owned())
}

fn read_secure_key_file(path: &Path) -> Result<Zeroizing<Vec<u8>>, R2a2Error> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| R2a2Error::InvalidReceipt)?;
    let metadata = file.metadata().map_err(|_| R2a2Error::InvalidReceipt)?;
    // SAFETY: geteuid has no preconditions and does not access Rust memory.
    let effective_uid = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != effective_uid
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 129
    {
        return Err(R2a2Error::InvalidReceipt);
    }
    let mut encoded = Zeroizing::new(Vec::with_capacity(metadata.len() as usize));
    file.read_to_end(&mut encoded)
        .map_err(|_| R2a2Error::InvalidReceipt)?;
    decode_key_file(&encoded)
}

#[allow(dead_code)] // R2A2 keeps production entry closed; R2B must use this reader.
pub(crate) fn load_production_source_keys(
) -> Result<BTreeMap<String, Zeroizing<Vec<u8>>>, R2a2Error> {
    required_local_source_names()
        .map(|source| {
            let path = Path::new(PRODUCTION_LOCAL_KEY_DIR).join(format!("{source}.key"));
            Ok((source.to_owned(), read_secure_key_file(&path)?))
        })
        .collect()
}

#[allow(dead_code)] // R2A2 keeps production entry closed; R2B must use this reader.
pub(crate) fn load_production_account_key(
    generation: &str,
) -> Result<Zeroizing<Vec<u8>>, R2a2Error> {
    if !canonical_positive_generation(generation) {
        return Err(R2a2Error::AccountBinding);
    }
    let path = PathBuf::from(PRODUCTION_ACCOUNT_KEY_DIR).join(format!("{generation}.key"));
    read_secure_key_file(&path).map_err(|_| R2a2Error::AccountBinding)
}

fn canonical_positive_generation(value: &str) -> bool {
    !value.is_empty()
        && value != "0"
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn canonical_decimal(value: &str, allow_negative: bool) -> Option<Decimal> {
    if value.is_empty() || value.starts_with('+') || value.ends_with('.') {
        return None;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if value.starts_with('-') && !allow_negative {
        return None;
    }
    let mut split = unsigned.split('.');
    let whole = split.next()?;
    let fraction = split.next();
    if split.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.is_some_and(|part| {
            part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) || part.ends_with('0')
        })
    {
        return None;
    }
    let parsed = Decimal::from_str(value).ok()?;
    if parsed.is_zero() && value != "0" {
        return None;
    }
    Some(parsed)
}

fn receipt_preimage(receipt: &LocalAuthorityReceipt) -> Vec<u8> {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(LOCAL_RECEIPT_DOMAIN.as_bytes());
    let mut add = |value: &str| {
        preimage.extend_from_slice(&(value.len() as u64).to_be_bytes());
        preimage.extend_from_slice(value.as_bytes());
    };
    add(&receipt.source_name);
    add(&receipt.issuer);
    add(&receipt.evidence_schema);
    add(&exact_millis(receipt.observed_at_utc));
    add(&receipt.key_generation_id);
    add(&receipt.run_identity_sha256);
    add(&receipt.keyed_account_binding_hmac_sha256);
    add(&receipt.execution_build_identity_sha256);
    for (name, value) in &receipt.claims {
        add(name);
        add(value);
    }
    preimage
}

#[cfg(test)]
fn authenticate_receipt_for_test(
    receipt: &mut LocalAuthorityReceipt,
    key: &[u8],
) -> Result<(), R2a2Error> {
    if key.len() < MIN_HMAC_KEY_BYTES {
        return Err(R2a2Error::InvalidReceipt);
    }
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| R2a2Error::InvalidReceipt)?;
    mac.update(&receipt_preimage(receipt));
    receipt.authentication_tag_hmac_sha256 = format!("{:x}", mac.finalize().into_bytes());
    Ok(())
}

pub fn keyed_account_binding(account_id: &str, key: &[u8]) -> Result<String, R2a2Error> {
    if account_id.is_empty() || key.len() < MIN_HMAC_KEY_BYTES {
        return Err(R2a2Error::AccountBinding);
    }
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| R2a2Error::AccountBinding)?;
    mac.update(ACCOUNT_HMAC_DOMAIN);
    mac.update(&[0]);
    let len = u32::try_from(account_id.len()).map_err(|_| R2a2Error::AccountBinding)?;
    mac.update(&len.to_be_bytes());
    mac.update(account_id.as_bytes());
    Ok(format!("{:x}", mac.finalize().into_bytes()))
}

pub fn verify_account_binding(
    manifest: &ValidatedManifest,
    account_id: &str,
    account_key_generation: &str,
    account_key: &[u8],
) -> Result<(), R2a2Error> {
    if account_key_generation != manifest.account_key_generation_id {
        return Err(R2a2Error::AccountBinding);
    }
    let asserted = decode_lower_hex(&manifest.keyed_account_binding_hmac_sha256)
        .map_err(|_| R2a2Error::AccountBinding)?;
    let mut mac = HmacSha256::new_from_slice(account_key).map_err(|_| R2a2Error::AccountBinding)?;
    mac.update(ACCOUNT_HMAC_DOMAIN);
    mac.update(&[0]);
    let len = u32::try_from(account_id.len()).map_err(|_| R2a2Error::AccountBinding)?;
    mac.update(&len.to_be_bytes());
    mac.update(account_id.as_bytes());
    mac.verify_slice(&asserted)
        .map_err(|_| R2a2Error::AccountBinding)
}

fn endpoint_identity(
    operation: Operation,
    account_binding: &str,
    renderer: &str,
) -> Result<String, R2a2Error> {
    let authority: EndpointAuthority = serde_json::from_str(ENDPOINT_AUTHORITY)?;
    let name = match operation {
        Operation::Place => "PLACE",
        Operation::Cancel => "CANCEL",
    };
    let endpoint = authority.operations.get(name).ok_or(R2a2Error::Authority)?;
    if renderer != authority.accepted_endpoint_renderer_sha256 {
        return Err(R2a2Error::UnauthorizedManifest);
    }
    Ok(digest_parts(
        ENDPOINT_IDENTITY_DOMAIN,
        &[
            &endpoint.method,
            &endpoint.route_template_id,
            account_binding,
            renderer,
        ],
    ))
}

fn exact_claim<'a>(
    receipts: &'a BTreeMap<&str, &LocalAuthorityReceipt>,
    source: &str,
    claim: &str,
) -> Result<&'a str, R2a2Error> {
    receipts
        .get(source)
        .and_then(|receipt| receipt.claims.get(claim))
        .map(String::as_str)
        .ok_or(R2a2Error::InvalidReceipt)
}

#[allow(dead_code)] // R2A2 qualifies this for the future fixed R2B entry.
pub(crate) fn validate_manifest_and_local_authorities(
    manifest_bytes: &[u8],
    receipt_bytes: &[u8],
    source_keys: &BTreeMap<String, Zeroizing<Vec<u8>>>,
    system_now: DateTime<Utc>,
) -> Result<(ValidatedManifest, ValidatedLocalAuthorities), R2a2Error> {
    let raw: Map<String, Value> = serde_json::from_slice(manifest_bytes)?;
    let operation = match raw.get("operation").and_then(Value::as_str) {
        Some("PLACE") => Operation::Place,
        Some("CANCEL") => Operation::Cancel,
        _ => return Err(R2a2Error::UnauthorizedManifest),
    };
    let run_authority: RunAuthority = serde_json::from_str(RUN_AUTHORITY)?;
    let auth_authority: AuthorizationAuthority = serde_json::from_str(AUTHORIZATION_AUTHORITY)?;
    let variant = match operation {
        Operation::Place => &run_authority.run_identity.place_fields_in_exact_order,
        Operation::Cancel => &run_authority.run_identity.cancel_fields_in_exact_order,
    };
    let ordered: Vec<&String> = run_authority
        .run_identity
        .common_fields_in_exact_order_excluding_run_identity
        .iter()
        .chain(variant)
        .collect();
    let mut expected: BTreeSet<&str> = ordered.iter().map(|field| field.as_str()).collect();
    expected.insert("run_identity_sha256");
    if raw.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected {
        return Err(R2a2Error::UnauthorizedManifest);
    }
    let fields: BTreeMap<String, String> = raw
        .iter()
        .map(|(name, value)| {
            let text = value
                .as_str()
                .filter(|text| text.is_ascii())
                .ok_or(R2a2Error::UnauthorizedManifest)?;
            Ok((name.clone(), text.to_owned()))
        })
        .collect::<Result<_, R2a2Error>>()?;
    let values: Vec<&str> = ordered
        .iter()
        .map(|field| fields.get(field.as_str()).map(String::as_str))
        .collect::<Option<_>>()
        .ok_or(R2a2Error::UnauthorizedManifest)?;
    let run_identity = digest_parts(&run_authority.run_identity.domain_utf8, &values);
    if fields.get("run_identity_sha256") != Some(&run_identity) {
        return Err(R2a2Error::UnauthorizedManifest);
    }

    let golden = run_authority
        .golden_vectors
        .get(match operation {
            Operation::Place => "PLACE",
            Operation::Cancel => "CANCEL",
        })
        .ok_or(R2a2Error::Authority)?;
    let immutable_names = [
        "execution_build_identity_sha256",
        "source_ref",
        "source_archive_sha256",
        "executable_sha256",
        "config_sha256",
        "policy_sha256",
        "config_policy_authority_sha256",
        "instrument_contract_sha256",
        "api_contract_sha256",
        "endpoint_renderer_sha256",
        "network_policy_sha256",
        "freshness_budget_authority_sha256",
    ];
    for name in immutable_names {
        if fields.get(name) != golden.manifest_without_run_identity_sha256.get(name) {
            return Err(R2a2Error::UnauthorizedManifest);
        }
    }
    if fields.get("execution_build_identity_sha256")
        != Some(
            &auth_authority
                .accepted_execution_build
                .execution_build_identity_sha256,
        )
        || fields.get("source_ref") != Some(&auth_authority.accepted_execution_build.source_ref)
        || fields.get("source_archive_sha256")
            != Some(
                &auth_authority
                    .accepted_execution_build
                    .source_archive_sha256,
            )
        || fields.get("executable_sha256")
            != Some(&auth_authority.accepted_execution_build.executable_sha256)
        || fields.get("freshness_budget_authority_sha256")
            != Some(
                &auth_authority
                    .bound_authorities
                    .freshness_budget_authority_sha256,
            )
    {
        return Err(R2a2Error::UnauthorizedManifest);
    }
    for name in [
        "process_boot_fingerprint_sha256",
        "keyed_account_binding_hmac_sha256",
        "stage6_checkpoint_fingerprint",
        "ownership_lease_fingerprint",
        "place_request_body_sha256",
        "cancel_target_lifecycle_fingerprint",
        "cancel_target_currently_working_proof_sha256",
        "cancel_request_body_sha256",
    ] {
        if let Some(value) = fields.get(name) {
            if !is_lower_sha256(value) {
                return Err(R2a2Error::UnauthorizedManifest);
            }
        }
    }
    if fields.get("source_ref").is_none_or(|value| {
        value.len() != 40
            || !value
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    }) {
        return Err(R2a2Error::UnauthorizedManifest);
    }
    for name in [
        "account_key_generation_id",
        "stage7b_seal_generation",
        "durable_budget_generation",
        "kill_switch_generation",
    ] {
        if fields
            .get(name)
            .is_none_or(|value| !canonical_positive_generation(value))
        {
            return Err(R2a2Error::UnauthorizedManifest);
        }
    }
    let expiry = DateTime::parse_from_rfc3339(
        fields
            .get("run_expires_at_utc")
            .ok_or(R2a2Error::UnauthorizedManifest)?,
    )
    .map_err(|_| R2a2Error::UnauthorizedManifest)?
    .with_timezone(&Utc);
    if exact_millis(expiry) != fields["run_expires_at_utc"]
        || expiry <= system_now
        || expiry > system_now + ChronoDuration::milliseconds(MAX_RUN_AHEAD_MS)
    {
        return Err(R2a2Error::UnauthorizedManifest);
    }
    let approved_position = canonical_decimal(&fields["approved_pre_run_position"], true)
        .ok_or(R2a2Error::UnauthorizedManifest)?;
    match operation {
        Operation::Place => {
            if fields["instrument"] != TARGET_INSTRUMENT
                || fields["quantity"] != "1"
                || fields["order_type"] != "ORDER_TYPE_LIMIT"
                || fields["time_in_force"] != "TIME_IN_FORCE_DAY"
                || !matches!(fields["side"].as_str(), "BUY" | "SELL")
            {
                return Err(R2a2Error::UnauthorizedManifest);
            }
            let price = canonical_decimal(&fields["limit_price_canonical_decimal"], false)
                .filter(|value| value.is_sign_positive() && !value.is_zero())
                .ok_or(R2a2Error::UnauthorizedManifest)?;
            let maximum = canonical_decimal(&fields["max_notional_canonical_decimal"], false)
                .filter(|value| value.is_sign_positive() && !value.is_zero())
                .ok_or(R2a2Error::UnauthorizedManifest)?;
            let quantity = Decimal::ONE;
            let notional = price
                .checked_mul(quantity)
                .ok_or(R2a2Error::UnauthorizedManifest)?;
            if notional > maximum {
                return Err(R2a2Error::UnauthorizedManifest);
            }
        }
        Operation::Cancel => {
            let order_id = &fields["cancel_target_broker_order_id"];
            if order_id.is_empty()
                || order_id.to_ascii_lowercase().starts_with("synthetic")
                || fields["cancel_target_strategy_request_id"] != fields["strategy_request_id"]
                || fields["cancel_target_durable_client_order_id"]
                    != fields["durable_client_order_id"]
            {
                return Err(R2a2Error::UnauthorizedManifest);
            }
        }
    }
    let computed_endpoint = endpoint_identity(
        operation,
        &fields["keyed_account_binding_hmac_sha256"],
        &fields["endpoint_renderer_sha256"],
    )?;
    if computed_endpoint != fields["endpoint_identity_sha256"] {
        return Err(R2a2Error::UnauthorizedManifest);
    }

    let envelope: LocalAuthorityEnvelope = serde_json::from_slice(receipt_bytes)?;
    if envelope.schema_version != LOCAL_RECEIPT_SCHEMA
        || envelope.receipts.len() != LOCAL_SOURCES.len()
    {
        return Err(R2a2Error::InvalidReceipt);
    }
    let receipts: BTreeMap<&str, &LocalAuthorityReceipt> = envelope
        .receipts
        .iter()
        .map(|receipt| (receipt.source_name.as_str(), receipt))
        .collect();
    if receipts.len() != LOCAL_SOURCES.len()
        || receipts.keys().copied().collect::<BTreeSet<_>>()
            != LOCAL_SOURCES.iter().map(|item| item.0).collect()
        || source_keys
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != LOCAL_SOURCES.iter().map(|item| item.0).collect()
    {
        return Err(R2a2Error::InvalidReceipt);
    }
    let mut receipt_hashes = BTreeMap::new();
    for (source, issuer, schema, max_age_ms) in LOCAL_SOURCES {
        let receipt = receipts.get(source).ok_or(R2a2Error::InvalidReceipt)?;
        let key = source_keys.get(*source).ok_or(R2a2Error::InvalidReceipt)?;
        if key.len() < MIN_HMAC_KEY_BYTES
            || receipt.issuer != *issuer
            || receipt.evidence_schema != *schema
            || receipt.key_generation_id != LOCAL_RECEIPT_KEY_GENERATION_ID
            || receipt.run_identity_sha256 != run_identity
            || receipt.keyed_account_binding_hmac_sha256
                != fields["keyed_account_binding_hmac_sha256"]
            || receipt.execution_build_identity_sha256 != fields["execution_build_identity_sha256"]
        {
            return Err(R2a2Error::InvalidReceipt);
        }
        let age = system_now
            .signed_duration_since(receipt.observed_at_utc)
            .num_milliseconds();
        if age > *max_age_ms || age < -250 {
            return Err(R2a2Error::InvalidReceipt);
        }
        let asserted = decode_lower_hex(&receipt.authentication_tag_hmac_sha256)?;
        let mut mac = HmacSha256::new_from_slice(key).map_err(|_| R2a2Error::InvalidReceipt)?;
        let preimage = receipt_preimage(receipt);
        mac.update(&preimage);
        mac.verify_slice(&asserted)
            .map_err(|_| R2a2Error::InvalidReceipt)?;
        receipt_hashes.insert(
            receipt.source_name.clone(),
            format!("{:x}", Sha256::digest(preimage)),
        );
    }
    let trusted_now =
        DateTime::parse_from_rfc3339(exact_claim(&receipts, "trusted_clock", "trusted_now_utc")?)
            .map_err(|_| R2a2Error::InvalidReceipt)?
            .with_timezone(&Utc);
    if exact_millis(trusted_now) != exact_claim(&receipts, "trusted_clock", "trusted_now_utc")?
        || system_now
            .signed_duration_since(trusted_now)
            .num_milliseconds()
            .abs()
            > 1_000
        || exact_claim(
            &receipts,
            "trusted_clock",
            "process_boot_fingerprint_sha256",
        )? != fields["process_boot_fingerprint_sha256"]
        || exact_claim(
            &receipts,
            "stage7b_current_recovery_seal",
            "stage7b_seal_generation",
        )? != fields["stage7b_seal_generation"]
        || exact_claim(
            &receipts,
            "stage7b_current_recovery_seal",
            "stage6_checkpoint_fingerprint",
        )? != fields["stage6_checkpoint_fingerprint"]
        || exact_claim(
            &receipts,
            "stage6_exact_dispatch_ready_command",
            "strategy_request_id",
        )? != fields["strategy_request_id"]
        || exact_claim(
            &receipts,
            "stage6_exact_dispatch_ready_command",
            "durable_client_order_id",
        )? != fields["durable_client_order_id"]
        || exact_claim(
            &receipts,
            "stage6_exact_dispatch_ready_command",
            "operation",
        )? != fields["operation"]
        || exact_claim(
            &receipts,
            "stage6_exact_dispatch_ready_command",
            "request_body_sha256",
        )? != fields[match operation {
            Operation::Place => "place_request_body_sha256",
            Operation::Cancel => "cancel_request_body_sha256",
        }]
        || exact_claim(
            &receipts,
            "stage8a_root_config_policy_control",
            "config_sha256",
        )? != fields["config_sha256"]
        || exact_claim(
            &receipts,
            "stage8a_root_config_policy_control",
            "policy_sha256",
        )? != fields["policy_sha256"]
        || exact_claim(
            &receipts,
            "stage8a_root_config_policy_control",
            "config_policy_authority_sha256",
        )? != fields["config_policy_authority_sha256"]
        || exact_claim(&receipts, "composite_readiness", "ready")? != "true"
        || exact_claim(&receipts, "kill_switch_run_allowed", "run_allowed")? != "true"
        || exact_claim(
            &receipts,
            "kill_switch_run_allowed",
            "kill_switch_generation",
        )? != fields["kill_switch_generation"]
        || exact_claim(&receipts, "single_finam_ownership", "single_owner")? != "true"
        || exact_claim(
            &receipts,
            "single_finam_ownership",
            "ownership_lease_fingerprint",
        )? != fields["ownership_lease_fingerprint"]
        || exact_claim(&receipts, "schedule", "eligible")? != "true"
        || exact_claim(&receipts, "instrument_specification", "instrument")? != TARGET_INSTRUMENT
        || exact_claim(&receipts, "instrument_specification", "eligible")? != "true"
        || exact_claim(&receipts, "ambiguity_orphan_unresolved_lifecycle", "clear")? != "true"
        || exact_claim(&receipts, "durable_micro_budget", "available")? != "true"
        || exact_claim(
            &receipts,
            "durable_micro_budget",
            "durable_budget_generation",
        )? != fields["durable_budget_generation"]
    {
        return Err(R2a2Error::InvalidReceipt);
    }
    if operation == Operation::Cancel
        && (exact_claim(
            &receipts,
            "stage6_exact_dispatch_ready_command",
            "cancel_target_broker_order_id",
        )? != fields["cancel_target_broker_order_id"]
            || exact_claim(
                &receipts,
                "stage6_exact_dispatch_ready_command",
                "cancel_target_lifecycle_fingerprint",
            )? != fields["cancel_target_lifecycle_fingerprint"]
            || exact_claim(
                &receipts,
                "stage6_exact_dispatch_ready_command",
                "cancel_target_currently_working_proof_sha256",
            )? != fields["cancel_target_currently_working_proof_sha256"])
    {
        return Err(R2a2Error::InvalidReceipt);
    }

    Ok((
        ValidatedManifest {
            fields,
            operation,
            run_identity_sha256: run_identity,
            broker_order_id: match operation {
                Operation::Place => None,
                Operation::Cancel => Some(
                    raw["cancel_target_broker_order_id"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
            },
            keyed_account_binding_hmac_sha256: raw["keyed_account_binding_hmac_sha256"]
                .as_str()
                .unwrap()
                .to_owned(),
            account_key_generation_id: raw["account_key_generation_id"]
                .as_str()
                .unwrap()
                .to_owned(),
            approved_pre_run_position: approved_position,
        },
        ValidatedLocalAuthorities {
            summary: LocalAuthoritySummary {
                schema_version: 1,
                receipt_count: receipt_hashes.len(),
                receipt_sha256: receipt_hashes,
                broker_derived_sources_accepted_pre_network: false,
                caller_selected_key_path_allowed: false,
            },
            trusted_now_utc: trusted_now,
        },
    ))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrictAuthResponse {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrictTokenDetails {
    pub account_ids: Vec<String>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    pub md_permissions: Vec<Value>,
    pub readonly: bool,
}

#[cfg(test)]
fn validate_token_details(details: StrictTokenDetails, account_id: &str) -> Result<(), R2a2Error> {
    if !details.readonly
        || details.account_ids.as_slice() != [account_id]
        || details.expires_at.as_deref().is_none_or(str::is_empty)
        || details.created_at.as_deref().is_none_or(str::is_empty)
    {
        return Err(R2a2Error::AccountBinding);
    }
    let _ = details.md_permissions;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictDecimal {
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictOrderRequest {
    account_id: String,
    client_order_id: Option<String>,
    comment: Option<String>,
    legs: Vec<Value>,
    limit_price: Option<StrictDecimal>,
    quantity: Option<StrictDecimal>,
    side: String,
    stop_condition: Option<String>,
    symbol: String,
    time_in_force: Option<String>,
    #[serde(rename = "type")]
    order_type: String,
    valid_before: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictOrder {
    exec_id: Option<String>,
    executed_quantity: Option<StrictDecimal>,
    initial_quantity: Option<StrictDecimal>,
    order: StrictOrderRequest,
    order_id: Option<String>,
    remaining_quantity: Option<StrictDecimal>,
    status: String,
    transact_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictOrdersResponse {
    orders: Vec<StrictOrder>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictTrade {
    trade_id: Option<String>,
    order_id: Option<String>,
    client_order_id: Option<String>,
    account_id: Option<String>,
    symbol: Option<String>,
    side: Option<String>,
    price: Option<StrictDecimal>,
    quantity: Option<StrictDecimal>,
    size: Option<StrictDecimal>,
    amount: Option<StrictDecimal>,
    commission: Option<Value>,
    timestamp: Option<String>,
    transact_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictTradesResponse {
    trades: Vec<StrictTrade>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictPosition {
    asset_type: Option<String>,
    average_price: Option<StrictDecimal>,
    avg_price: Option<StrictDecimal>,
    balance: Option<StrictDecimal>,
    current_price: Option<StrictDecimal>,
    quantity: Option<StrictDecimal>,
    symbol: Option<String>,
    unrealized_profit: Option<StrictDecimal>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictAccountResponse {
    account_id: String,
    cash: Vec<Value>,
    equity: Option<StrictDecimal>,
    first_non_trade_date: Option<String>,
    open_account_date: Option<String>,
    portfolio_mc: Option<Value>,
    positions: Vec<StrictPosition>,
    status: Option<String>,
    #[serde(rename = "type")]
    account_type: Option<String>,
    unrealized_profit: Option<StrictDecimal>,
}

#[derive(Debug, Clone)]
pub struct BrokerTruthBodies<'a> {
    pub exact_order: Option<&'a [u8]>,
    pub orders: &'a [u8],
    pub trades: &'a [u8],
    pub account: &'a [u8],
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrokerTruthSummary {
    pub schema_version: u8,
    pub operation: Operation,
    pub target_instrument: &'static str,
    pub target_position_canonical_decimal: String,
    pub approved_position_matches: bool,
    pub target_order_count: usize,
    pub target_trade_count: usize,
    pub account_wide_active_order_count: usize,
    pub exact_cancel_order_id_sha256: Option<String>,
    pub exact_cancel_working: Option<bool>,
    pub semantic_receipt_sha256: String,
    pub raw_bodies_exported: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SemanticAttemptEvidence {
    pub ordinal: usize,
    pub network_class: crate::NetworkClass,
    pub method: &'static str,
    pub route_template: &'static str,
    pub status: u16,
    pub response_body_len: usize,
    pub semantic_receipt_sha256: String,
    pub raw_body_sha256_exported: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QualifiedReadonlyEvidence {
    pub schema_version: u8,
    pub operation: Operation,
    pub run_identity_sha256: String,
    pub local_authorities: LocalAuthoritySummary,
    pub broker_truth: BrokerTruthSummary,
    pub request_order: Vec<SemanticAttemptEvidence>,
    pub auth_request_count: usize,
    pub broker_get_count: usize,
    pub operator_arm_issued: bool,
    pub dispatch_attempt_recorded: bool,
    pub effect_transport_entered: bool,
    pub finam_order_post_delete_sent: bool,
    pub authorization_status: &'static str,
}

fn parse_decimal(value: &StrictDecimal, allow_negative: bool) -> Result<Decimal, R2a2Error> {
    canonical_decimal(&value.value, allow_negative).ok_or(R2a2Error::BrokerTruth)
}

fn canonical_decimal_text(value: Decimal) -> String {
    value.normalize().to_string()
}

fn status_class(status: &str) -> Option<bool> {
    let normalized = status.strip_prefix("ORDER_STATUS_").unwrap_or(status);
    match normalized {
        "NEW" | "ACCEPTED" | "ACTIVE" | "WORKING" | "MATCHING" | "WAIT" | "FORWARDING"
        | "WATCHING" | "PENDING_NEW" | "PENDING_CANCEL" | "PARTIALLY_FILLED" => Some(true),
        "FILLED"
        | "EXECUTED"
        | "SL_EXECUTED"
        | "TP_EXECUTED"
        | "CANCELED"
        | "CANCELLED"
        | "REJECTED"
        | "FAILED"
        | "DENIED_BY_BROKER"
        | "REJECTED_BY_EXCHANGE"
        | "EXPIRED" => Some(false),
        _ => None,
    }
}

fn validate_order_identity(
    order: &StrictOrder,
    manifest: &ValidatedManifest,
    account_id: &str,
) -> Result<bool, R2a2Error> {
    if order.order.account_id != account_id
        || order.order.symbol.is_empty()
        || order.order_id.as_deref().is_none_or(str::is_empty)
        || order.order.side.is_empty()
        || order.order.order_type.is_empty()
        || order.order.legs.iter().any(|_| true)
        || order.order.quantity.is_none()
    {
        return Err(R2a2Error::BrokerTruth);
    }
    let _ = parse_decimal(order.order.quantity.as_ref().unwrap(), false)?;
    if let Some(price) = &order.order.limit_price {
        let _ = parse_decimal(price, false)?;
    }
    if let Some(value) = &order.executed_quantity {
        let _ = parse_decimal(value, false)?;
    }
    if let Some(value) = &order.initial_quantity {
        let _ = parse_decimal(value, false)?;
    }
    if let Some(value) = &order.remaining_quantity {
        let _ = parse_decimal(value, false)?;
    }
    let _ = (
        &order.exec_id,
        &order.order.comment,
        &order.order.stop_condition,
    );
    let _ = (
        &order.order.time_in_force,
        &order.order.valid_before,
        &order.transact_at,
    );
    status_class(&order.status).ok_or(R2a2Error::BrokerTruth)?;
    Ok(order.order.symbol == TARGET_INSTRUMENT
        && order.order.client_order_id.as_deref()
            == Some(manifest.field("durable_client_order_id")))
}

fn position_quantity(
    account: &StrictAccountResponse,
    account_id: &str,
) -> Result<Decimal, R2a2Error> {
    if account.account_id != account_id {
        return Err(R2a2Error::BrokerTruth);
    }
    if account
        .positions
        .iter()
        .any(|position| position.symbol.as_deref().is_none_or(str::is_empty))
    {
        return Err(R2a2Error::BrokerTruth);
    }
    let matches: Vec<&StrictPosition> = account
        .positions
        .iter()
        .filter(|position| position.symbol.as_deref() == Some(TARGET_INSTRUMENT))
        .collect();
    if matches.len() > 1 {
        return Err(R2a2Error::BrokerTruth);
    }
    let _ = (
        &account.cash,
        &account.equity,
        &account.first_non_trade_date,
    );
    let _ = (
        &account.open_account_date,
        &account.portfolio_mc,
        &account.status,
    );
    let _ = (&account.account_type, &account.unrealized_profit);
    let Some(position) = matches.first() else {
        return Ok(Decimal::ZERO);
    };
    let quantity = match (&position.quantity, &position.balance) {
        (Some(quantity), Some(balance)) => {
            let quantity = parse_decimal(quantity, true)?;
            let balance = parse_decimal(balance, true)?;
            if quantity != balance {
                return Err(R2a2Error::BrokerTruth);
            }
            quantity
        }
        (Some(quantity), None) => parse_decimal(quantity, true)?,
        (None, Some(balance)) => parse_decimal(balance, true)?,
        (None, None) => return Err(R2a2Error::BrokerTruth),
    };
    if let Some(value) = &position.average_price {
        let _ = parse_decimal(value, true)?;
    }
    if let Some(value) = &position.avg_price {
        let _ = parse_decimal(value, true)?;
    }
    if let Some(value) = &position.current_price {
        let _ = parse_decimal(value, true)?;
    }
    if let Some(value) = &position.unrealized_profit {
        let _ = parse_decimal(value, true)?;
    }
    let _ = &position.asset_type;
    Ok(quantity)
}

fn check_cap(bytes: &[u8], cap: usize) -> Result<(), R2a2Error> {
    if bytes.len() > cap {
        Err(R2a2Error::OversizeResponse)
    } else {
        Ok(())
    }
}

#[allow(dead_code)] // R2A2 qualifies this for the future fixed R2B entry.
pub(crate) fn reduce_broker_truth(
    manifest: &ValidatedManifest,
    account_id: &str,
    bodies: BrokerTruthBodies<'_>,
) -> Result<BrokerTruthSummary, R2a2Error> {
    check_cap(bodies.orders, ORDERS_BODY_CAP)?;
    check_cap(bodies.trades, TRADES_BODY_CAP)?;
    check_cap(bodies.account, ACCOUNT_BODY_CAP)?;
    if let Some(exact) = bodies.exact_order {
        check_cap(exact, EXACT_ORDER_BODY_CAP)?;
    }
    let orders: StrictOrdersResponse = serde_json::from_slice(bodies.orders)?;
    let trades: StrictTradesResponse = serde_json::from_slice(bodies.trades)?;
    let account: StrictAccountResponse = serde_json::from_slice(bodies.account)?;
    if trades.trades.len() >= crate::TRADES_LIMIT {
        return Err(R2a2Error::BrokerTruth);
    }
    let target_position = position_quantity(&account, account_id)?;
    if target_position != manifest.approved_pre_run_position {
        return Err(R2a2Error::BrokerTruth);
    }
    let mut target_order_count = 0usize;
    let mut account_wide_active = 0usize;
    let mut target_order: Option<&StrictOrder> = None;
    for order in &orders.orders {
        let active = status_class(&order.status).ok_or(R2a2Error::BrokerTruth)?;
        if active {
            account_wide_active += 1;
        }
        if validate_order_identity(order, manifest, account_id)? {
            target_order_count += 1;
            if manifest.broker_order_id.as_deref() == order.order_id.as_deref() {
                if target_order.is_some() {
                    return Err(R2a2Error::BrokerTruth);
                }
                target_order = Some(order);
            }
        }
    }
    let mut target_trade_count = 0usize;
    for trade in &trades.trades {
        if trade.account_id.as_deref() != Some(account_id)
            || trade.symbol.as_deref().is_none_or(str::is_empty)
        {
            return Err(R2a2Error::BrokerTruth);
        }
        let relevant = trade.account_id.as_deref() == Some(account_id)
            && trade.symbol.as_deref() == Some(TARGET_INSTRUMENT)
            && trade.client_order_id.as_deref() == Some(manifest.field("durable_client_order_id"));
        if relevant {
            if trade.trade_id.as_deref().is_none_or(str::is_empty)
                || trade.side.as_deref().is_none_or(str::is_empty)
                || trade.price.as_ref().is_none()
                || (trade.quantity.is_none() && trade.size.is_none())
            {
                return Err(R2a2Error::BrokerTruth);
            }
            let _ = parse_decimal(trade.price.as_ref().unwrap(), false)?;
            if let Some(value) = &trade.quantity {
                let _ = parse_decimal(value, false)?;
            }
            if let Some(value) = &trade.size {
                let _ = parse_decimal(value, false)?;
            }
            if let Some(value) = &trade.amount {
                let _ = parse_decimal(value, true)?;
            }
            let _ = (&trade.commission, &trade.timestamp, &trade.transact_at);
            let _ = &trade.order_id;
            target_trade_count += 1;
        }
    }

    let (exact_order_hash, exact_working) = match manifest.operation {
        Operation::Place => {
            if bodies.exact_order.is_some() || account_wide_active != 0 || target_order_count != 0 {
                return Err(R2a2Error::BrokerTruth);
            }
            (None, None)
        }
        Operation::Cancel => {
            let exact: StrictOrder =
                serde_json::from_slice(bodies.exact_order.ok_or(R2a2Error::BrokerTruth)?)?;
            if !validate_order_identity(&exact, manifest, account_id)?
                || exact.order_id.as_deref() != manifest.broker_order_id.as_deref()
                || exact.order.client_order_id.as_deref()
                    != Some(manifest.field("cancel_target_durable_client_order_id"))
                || target_order.is_none()
                || target_order.unwrap().status != exact.status
                || target_order.unwrap().order.client_order_id != exact.order.client_order_id
            {
                return Err(R2a2Error::BrokerTruth);
            }
            let working = status_class(&exact.status).ok_or(R2a2Error::BrokerTruth)?;
            if !working {
                return Err(R2a2Error::BrokerTruth);
            }
            if account_wide_active != 1 || target_order_count != 1 {
                return Err(R2a2Error::BrokerTruth);
            }
            (
                Some(format!(
                    "{:x}",
                    Sha256::digest(exact.order_id.as_ref().unwrap().as_bytes())
                )),
                Some(true),
            )
        }
    };
    let position_text = canonical_decimal_text(target_position);
    let semantic_receipt = digest_parts(
        "stage8b-p-r2a2-broker-truth-semantic-receipt-v1",
        &[
            match manifest.operation {
                Operation::Place => "PLACE",
                Operation::Cancel => "CANCEL",
            },
            &manifest.run_identity_sha256,
            TARGET_INSTRUMENT,
            &position_text,
            &target_order_count.to_string(),
            &target_trade_count.to_string(),
            &account_wide_active.to_string(),
            exact_order_hash.as_deref().unwrap_or("NONE"),
        ],
    );
    Ok(BrokerTruthSummary {
        schema_version: 1,
        operation: manifest.operation,
        target_instrument: TARGET_INSTRUMENT,
        target_position_canonical_decimal: position_text,
        approved_position_matches: true,
        target_order_count,
        target_trade_count,
        account_wide_active_order_count: account_wide_active,
        exact_cancel_order_id_sha256: exact_order_hash,
        exact_cancel_working: exact_working,
        semantic_receipt_sha256: semantic_receipt,
        raw_bodies_exported: false,
    })
}

pub fn semantic_attempt_receipt(
    ordinal: usize,
    method: &str,
    route_template: &str,
    status: u16,
    body_len: usize,
) -> String {
    digest_parts(
        "stage8b-p-r2a2-redacted-attempt-receipt-v1",
        &[
            &ordinal.to_string(),
            method,
            route_template,
            &status.to_string(),
            &body_len.to_string(),
        ],
    )
}

pub fn bounded_content_length(
    declared: Option<u64>,
    observed: usize,
    cap: usize,
) -> Result<(), R2a2Error> {
    if declared.is_some_and(|value| value > cap as u64) || observed > cap {
        Err(R2a2Error::OversizeResponse)
    } else {
        Ok(())
    }
}

#[cfg(test)]
async fn read_bounded_response(
    response: reqwest::Response,
    cap: usize,
) -> Result<(u16, Zeroizing<Vec<u8>>), R2a2Error> {
    bounded_content_length(response.content_length(), 0, cap)?;
    let status = response.status().as_u16();
    let mut response = response;
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| R2a2Error::BrokerTruth)? {
        let next = body
            .len()
            .checked_add(chunk.len())
            .ok_or(R2a2Error::OversizeResponse)?;
        if next > cap {
            return Err(R2a2Error::OversizeResponse);
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

#[cfg(test)]
async fn execute_controlled_pipeline(
    client: &reqwest::Client,
    base: &str,
    manifest: ValidatedManifest,
    local: ValidatedLocalAuthorities,
    account_id: &str,
    account_key: &[u8],
    secret: &str,
) -> Result<QualifiedReadonlyEvidence, R2a2Error> {
    verify_account_binding(
        &manifest,
        account_id,
        &manifest.account_key_generation_id,
        account_key,
    )?;
    if secret.is_empty() {
        return Err(R2a2Error::AccountBinding);
    }
    let base_url = reqwest::Url::parse(base).map_err(|_| R2a2Error::BrokerTruth)?;
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(R2a2Error::BrokerTruth);
    }
    let mut attempts = Vec::new();
    let auth_url = base_url
        .join("v1/sessions")
        .map_err(|_| R2a2Error::BrokerTruth)?;
    let (status, body) = read_bounded_response(
        client
            .post(auth_url)
            .json(&serde_json::json!({"secret": secret}))
            .send()
            .await
            .map_err(|_| R2a2Error::BrokerTruth)?,
        AUTH_BODY_CAP,
    )
    .await?;
    attempts.push(SemanticAttemptEvidence {
        ordinal: 1,
        network_class: crate::NetworkClass::AuthService,
        method: "POST",
        route_template: "/v1/sessions",
        status,
        response_body_len: body.len(),
        semantic_receipt_sha256: semantic_attempt_receipt(
            1,
            "POST",
            "/v1/sessions",
            status,
            body.len(),
        ),
        raw_body_sha256_exported: false,
    });
    if status != 200 {
        return Err(R2a2Error::BrokerTruth);
    }
    let auth: StrictAuthResponse = serde_json::from_slice(&body)?;
    let token = Zeroizing::new(auth.token);
    if token.is_empty() {
        return Err(R2a2Error::BrokerTruth);
    }

    let details_url = base_url
        .join("v1/sessions/details")
        .map_err(|_| R2a2Error::BrokerTruth)?;
    let (status, body) = read_bounded_response(
        client
            .post(details_url)
            .json(&serde_json::json!({"token": token.as_str()}))
            .send()
            .await
            .map_err(|_| R2a2Error::BrokerTruth)?,
        AUTH_BODY_CAP,
    )
    .await?;
    attempts.push(SemanticAttemptEvidence {
        ordinal: 2,
        network_class: crate::NetworkClass::AuthService,
        method: "POST",
        route_template: "/v1/sessions/details",
        status,
        response_body_len: body.len(),
        semantic_receipt_sha256: semantic_attempt_receipt(
            2,
            "POST",
            "/v1/sessions/details",
            status,
            body.len(),
        ),
        raw_body_sha256_exported: false,
    });
    if status != 200 {
        return Err(R2a2Error::BrokerTruth);
    }
    let details: StrictTokenDetails = serde_json::from_slice(&body)?;
    validate_token_details(details, account_id)?;

    let plan = crate::ReadPlan {
        operation: manifest.operation,
        run_identity_sha256: manifest.run_identity_sha256.clone(),
        broker_order_id: manifest.broker_order_id.clone(),
        sources: match manifest.operation {
            Operation::Place => vec![
                crate::Source::OrdersSnapshot,
                crate::Source::TradesSnapshot,
                crate::Source::PositionSnapshot,
            ],
            Operation::Cancel => vec![
                crate::Source::GetOrder,
                crate::Source::OrdersSnapshot,
                crate::Source::TradesSnapshot,
                crate::Source::PositionSnapshot,
            ],
        },
    };
    let mut exact_order = None;
    let mut orders = None;
    let mut trades = None;
    let mut account = None;
    for (index, source) in plan.sources.iter().copied().enumerate() {
        let (url, template) = crate::route(
            &base_url,
            source,
            account_id,
            plan.broker_order_id.as_deref(),
            local.trusted_now_utc,
        )
        .map_err(|_| R2a2Error::BrokerTruth)?;
        let cap = match source {
            crate::Source::GetOrder => EXACT_ORDER_BODY_CAP,
            crate::Source::OrdersSnapshot => ORDERS_BODY_CAP,
            crate::Source::TradesSnapshot => TRADES_BODY_CAP,
            crate::Source::PositionSnapshot => ACCOUNT_BODY_CAP,
        };
        let (status, body) = read_bounded_response(
            client
                .get(url)
                .bearer_auth(token.as_str())
                .send()
                .await
                .map_err(|_| R2a2Error::BrokerTruth)?,
            cap,
        )
        .await?;
        let ordinal = crate::AUTH_REQUEST_BUDGET + index + 1;
        attempts.push(SemanticAttemptEvidence {
            ordinal,
            network_class: crate::NetworkClass::BrokerTruth,
            method: "GET",
            route_template: template,
            status,
            response_body_len: body.len(),
            semantic_receipt_sha256: semantic_attempt_receipt(
                ordinal,
                "GET",
                template,
                status,
                body.len(),
            ),
            raw_body_sha256_exported: false,
        });
        if status != 200 {
            return Err(R2a2Error::BrokerTruth);
        }
        match source {
            crate::Source::GetOrder => exact_order = Some(body),
            crate::Source::OrdersSnapshot => orders = Some(body),
            crate::Source::TradesSnapshot => trades = Some(body),
            crate::Source::PositionSnapshot => account = Some(body),
        }
    }
    let broker_truth = reduce_broker_truth(
        &manifest,
        account_id,
        BrokerTruthBodies {
            exact_order: exact_order.as_ref().map(|body| body.as_slice()),
            orders: orders.as_deref().ok_or(R2a2Error::BrokerTruth)?,
            trades: trades.as_deref().ok_or(R2a2Error::BrokerTruth)?,
            account: account.as_deref().ok_or(R2a2Error::BrokerTruth)?,
        },
    )?;
    Ok(QualifiedReadonlyEvidence {
        schema_version: 2,
        operation: manifest.operation,
        run_identity_sha256: manifest.run_identity_sha256,
        local_authorities: local.summary,
        broker_truth,
        request_order: attempts,
        auth_request_count: crate::AUTH_REQUEST_BUDGET,
        broker_get_count: plan.sources.len(),
        operator_arm_issued: false,
        dispatch_attempt_recorded: false,
        effect_transport_entered: false,
        finam_order_post_delete_sent: false,
        authorization_status: "NOT_ISSUED",
    })
}

impl From<R2a2Error> for PreflightError {
    fn from(value: R2a2Error) -> Self {
        match value {
            R2a2Error::Json(error) => PreflightError::Json(error),
            R2a2Error::BrokerTruth | R2a2Error::OversizeResponse => {
                PreflightError::IncompleteBrokerTruth
            }
            _ => PreflightError::InvalidCurrentSources,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::{
        date_time_ymd, BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
        Issuer, KeyPair, KeyUsagePurpose,
    };
    use rustls::{
        pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
        ServerConfig,
    };
    use std::net::SocketAddr;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    const ACCOUNT: &str = "ACCOUNT";
    const ACCOUNT_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";
    type QualificationFixture = (Vec<u8>, Vec<u8>, BTreeMap<String, Zeroizing<Vec<u8>>>);

    fn fixture(operation: Operation, now: DateTime<Utc>) -> QualificationFixture {
        let authority: RunAuthority = serde_json::from_str(RUN_AUTHORITY).unwrap();
        let name = match operation {
            Operation::Place => "PLACE",
            Operation::Cancel => "CANCEL",
        };
        let mut fields = authority.golden_vectors[name]
            .manifest_without_run_identity_sha256
            .clone();
        let account_hmac = keyed_account_binding(ACCOUNT, ACCOUNT_KEY).unwrap();
        fields.insert(
            "keyed_account_binding_hmac_sha256".to_owned(),
            account_hmac.clone(),
        );
        fields.insert(
            "endpoint_identity_sha256".to_owned(),
            endpoint_identity(
                operation,
                &account_hmac,
                &fields["endpoint_renderer_sha256"],
            )
            .unwrap(),
        );
        fields.insert(
            "run_expires_at_utc".to_owned(),
            exact_millis(now + ChronoDuration::seconds(30)),
        );
        let variant = match operation {
            Operation::Place => &authority.run_identity.place_fields_in_exact_order,
            Operation::Cancel => &authority.run_identity.cancel_fields_in_exact_order,
        };
        let values: Vec<&str> = authority
            .run_identity
            .common_fields_in_exact_order_excluding_run_identity
            .iter()
            .chain(variant)
            .map(|field| fields[field].as_str())
            .collect();
        let run_identity = digest_parts(&authority.run_identity.domain_utf8, &values);
        fields.insert("run_identity_sha256".to_owned(), run_identity.clone());

        let mut keys = BTreeMap::new();
        let mut receipts = Vec::new();
        for (index, (source, issuer, schema, _)) in LOCAL_SOURCES.iter().enumerate() {
            let key = vec![index as u8 + 1; MIN_HMAC_KEY_BYTES];
            let claims = match *source {
                "trusted_clock" => BTreeMap::from([
                    ("trusted_now_utc".to_owned(), exact_millis(now)),
                    (
                        "process_boot_fingerprint_sha256".to_owned(),
                        fields["process_boot_fingerprint_sha256"].clone(),
                    ),
                ]),
                "stage7b_current_recovery_seal" => BTreeMap::from([
                    (
                        "stage7b_seal_generation".to_owned(),
                        fields["stage7b_seal_generation"].clone(),
                    ),
                    (
                        "stage6_checkpoint_fingerprint".to_owned(),
                        fields["stage6_checkpoint_fingerprint"].clone(),
                    ),
                ]),
                "stage6_exact_dispatch_ready_command" => {
                    let mut values = BTreeMap::from([
                        (
                            "strategy_request_id".to_owned(),
                            fields["strategy_request_id"].clone(),
                        ),
                        (
                            "durable_client_order_id".to_owned(),
                            fields["durable_client_order_id"].clone(),
                        ),
                        ("operation".to_owned(), fields["operation"].clone()),
                        (
                            "request_body_sha256".to_owned(),
                            fields[match operation {
                                Operation::Place => "place_request_body_sha256",
                                Operation::Cancel => "cancel_request_body_sha256",
                            }]
                            .clone(),
                        ),
                    ]);
                    if operation == Operation::Cancel {
                        for field in [
                            "cancel_target_broker_order_id",
                            "cancel_target_lifecycle_fingerprint",
                            "cancel_target_currently_working_proof_sha256",
                        ] {
                            values.insert(field.to_owned(), fields[field].clone());
                        }
                    }
                    values
                }
                "stage8a_root_config_policy_control" => BTreeMap::from([
                    ("config_sha256".to_owned(), fields["config_sha256"].clone()),
                    ("policy_sha256".to_owned(), fields["policy_sha256"].clone()),
                    (
                        "config_policy_authority_sha256".to_owned(),
                        fields["config_policy_authority_sha256"].clone(),
                    ),
                ]),
                "composite_readiness" => BTreeMap::from([("ready".to_owned(), "true".to_owned())]),
                "kill_switch_run_allowed" => BTreeMap::from([
                    ("run_allowed".to_owned(), "true".to_owned()),
                    (
                        "kill_switch_generation".to_owned(),
                        fields["kill_switch_generation"].clone(),
                    ),
                ]),
                "single_finam_ownership" => BTreeMap::from([
                    ("single_owner".to_owned(), "true".to_owned()),
                    (
                        "ownership_lease_fingerprint".to_owned(),
                        fields["ownership_lease_fingerprint"].clone(),
                    ),
                ]),
                "schedule" => BTreeMap::from([("eligible".to_owned(), "true".to_owned())]),
                "instrument_specification" => BTreeMap::from([
                    ("instrument".to_owned(), TARGET_INSTRUMENT.to_owned()),
                    ("eligible".to_owned(), "true".to_owned()),
                ]),
                "ambiguity_orphan_unresolved_lifecycle" => {
                    BTreeMap::from([("clear".to_owned(), "true".to_owned())])
                }
                "durable_micro_budget" => BTreeMap::from([
                    ("available".to_owned(), "true".to_owned()),
                    (
                        "durable_budget_generation".to_owned(),
                        fields["durable_budget_generation"].clone(),
                    ),
                ]),
                _ => unreachable!(),
            };
            let mut receipt = LocalAuthorityReceipt {
                source_name: source.to_string(),
                issuer: issuer.to_string(),
                evidence_schema: schema.to_string(),
                observed_at_utc: now,
                key_generation_id: "1".to_owned(),
                run_identity_sha256: run_identity.clone(),
                keyed_account_binding_hmac_sha256: account_hmac.clone(),
                execution_build_identity_sha256: fields["execution_build_identity_sha256"].clone(),
                claims,
                authentication_tag_hmac_sha256: String::new(),
            };
            authenticate_receipt_for_test(&mut receipt, &key).unwrap();
            keys.insert(source.to_string(), Zeroizing::new(key));
            receipts.push(receipt);
        }
        (
            serde_json::to_vec(&fields).unwrap(),
            serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "receipts": receipts,
            }))
            .unwrap(),
            keys,
        )
    }

    fn order(order_id: &str, status: &str) -> Value {
        serde_json::json!({
            "exec_id": null,
            "executed_quantity": {"value":"0"},
            "initial_quantity": {"value":"1"},
            "order": {
                "account_id": ACCOUNT,
                "client_order_id": "S8BP000000000000001",
                "comment": null,
                "legs": [],
                "limit_price": {"value":"2210"},
                "quantity": {"value":"1"},
                "side": "ORDER_SIDE_BUY",
                "stop_condition": null,
                "symbol": TARGET_INSTRUMENT,
                "time_in_force": "TIME_IN_FORCE_DAY",
                "type": "ORDER_TYPE_LIMIT",
                "valid_before": null
            },
            "order_id": order_id,
            "remaining_quantity": {"value":"1"},
            "status": status,
            "transact_at": null
        })
    }

    fn account(position: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "account_id": ACCOUNT,
            "cash": [],
            "equity": null,
            "first_non_trade_date": null,
            "open_account_date": null,
            "portfolio_mc": null,
            "positions": [{
                "asset_type": null,
                "average_price": null,
                "avg_price": null,
                "balance": null,
                "current_price": null,
                "quantity": {"value": position},
                "symbol": TARGET_INSTRUMENT,
                "unrealized_profit": null
            }],
            "status": "ACCOUNT_ACTIVE",
            "type": null,
            "unrealized_profit": null
        }))
        .unwrap()
    }

    #[test]
    fn account_hmac_matches_accepted_contract_and_is_constant_time_verified() {
        let computed = keyed_account_binding("ACCOUNT-42", ACCOUNT_KEY).unwrap();
        assert_eq!(computed.len(), 64);
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipts, keys) = fixture(Operation::Place, now);
        let (manifest, _) =
            validate_manifest_and_local_authorities(&manifest, &receipts, &keys, now).unwrap();
        verify_account_binding(&manifest, ACCOUNT, "7", ACCOUNT_KEY).unwrap();
        assert!(verify_account_binding(&manifest, "OTHER", "7", ACCOUNT_KEY).is_err());
    }

    #[test]
    fn local_key_reader_rejects_group_readable_files() {
        let path = std::env::temp_dir().join(format!(
            "stage8b-r2a2-key-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::write(&path, format!("{}\n", "01".repeat(32))).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_secure_key_file(&path).unwrap().len(), 32);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_secure_key_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn self_consistent_but_unaccepted_fixed_identity_is_rejected() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipts, keys) = fixture(Operation::Place, now);
        let mut fields: BTreeMap<String, String> = serde_json::from_slice(&manifest).unwrap();
        fields.insert("config_sha256".to_owned(), "a".repeat(64));
        let authority: RunAuthority = serde_json::from_str(RUN_AUTHORITY).unwrap();
        let values: Vec<&str> = authority
            .run_identity
            .common_fields_in_exact_order_excluding_run_identity
            .iter()
            .chain(&authority.run_identity.place_fields_in_exact_order)
            .map(|field| fields[field].as_str())
            .collect();
        fields.insert(
            "run_identity_sha256".to_owned(),
            digest_parts(&authority.run_identity.domain_utf8, &values),
        );
        assert!(matches!(
            validate_manifest_and_local_authorities(
                &serde_json::to_vec(&fields).unwrap(),
                &receipts,
                &keys,
                now
            ),
            Err(R2a2Error::UnauthorizedManifest)
        ));
    }

    #[test]
    fn forged_receipt_and_pre_network_broker_sources_are_rejected() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipt_bytes, keys) = fixture(Operation::Place, now);
        let mut envelope: Value = serde_json::from_slice(&receipt_bytes).unwrap();
        envelope["receipts"][0]["claims"]["trusted_now_utc"] =
            Value::String("2026-08-25T12:00:00.001Z".to_owned());
        assert!(validate_manifest_and_local_authorities(
            &manifest,
            &serde_json::to_vec(&envelope).unwrap(),
            &keys,
            now
        )
        .is_err());
        envelope["receipts"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "source_name":"account_orders"
            }));
        assert!(validate_manifest_and_local_authorities(
            &manifest,
            &serde_json::to_vec(&envelope).unwrap(),
            &keys,
            now
        )
        .is_err());
    }

    #[test]
    fn authentically_signed_but_stale_receipt_is_rejected() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipt_bytes, keys) = fixture(Operation::Place, now);
        let mut envelope: LocalAuthorityEnvelope = serde_json::from_slice(&receipt_bytes).unwrap();
        let receipt = envelope
            .receipts
            .iter_mut()
            .find(|receipt| receipt.source_name == "composite_readiness")
            .unwrap();
        receipt.observed_at_utc = now - ChronoDuration::seconds(2);
        authenticate_receipt_for_test(receipt, &keys["composite_readiness"]).unwrap();
        assert!(validate_manifest_and_local_authorities(
            &manifest,
            &serde_json::to_vec(&envelope).unwrap(),
            &keys,
            now
        )
        .is_err());
    }

    #[test]
    fn token_details_must_bind_exactly_one_readonly_account() {
        let valid = StrictTokenDetails {
            account_ids: vec![ACCOUNT.to_owned()],
            created_at: Some("2026-08-25T11:59:00Z".to_owned()),
            expires_at: Some("2026-08-25T12:05:00Z".to_owned()),
            md_permissions: vec![],
            readonly: true,
        };
        validate_token_details(valid, ACCOUNT).unwrap();
        let wrong = StrictTokenDetails {
            account_ids: vec!["OTHER".to_owned()],
            created_at: Some("2026-08-25T11:59:00Z".to_owned()),
            expires_at: Some("2026-08-25T12:05:00Z".to_owned()),
            md_permissions: vec![],
            readonly: true,
        };
        assert!(validate_token_details(wrong, ACCOUNT).is_err());
    }

    #[test]
    fn place_reducer_requires_strict_shape_and_exact_position() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipts, keys) = fixture(Operation::Place, now);
        let (manifest, _) =
            validate_manifest_and_local_authorities(&manifest, &receipts, &keys, now).unwrap();
        let orders = serde_json::to_vec(&serde_json::json!({"orders":[]})).unwrap();
        let trades = serde_json::to_vec(&serde_json::json!({"trades":[]})).unwrap();
        let account_body = account("0");
        let summary = reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: &orders,
                trades: &trades,
                account: &account_body,
            },
        )
        .unwrap();
        assert!(summary.approved_position_matches);
        assert!(!summary.raw_bodies_exported);
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: b"{}",
                trades: &trades,
                account: &account_body,
            }
        )
        .is_err());
        let wrong_position = account("1");
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: &orders,
                trades: &trades,
                account: &wrong_position,
            }
        )
        .is_err());
    }

    #[test]
    fn cancel_requires_exact_working_order_in_both_views() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipts, keys) = fixture(Operation::Cancel, now);
        let (manifest, _) =
            validate_manifest_and_local_authorities(&manifest, &receipts, &keys, now).unwrap();
        let exact = serde_json::to_vec(&order("2033126385648208390", "ORDER_STATUS_NEW")).unwrap();
        let orders = serde_json::to_vec(&serde_json::json!({
            "orders":[order("2033126385648208390", "ORDER_STATUS_NEW")]
        }))
        .unwrap();
        let trades = serde_json::to_vec(&serde_json::json!({"trades":[]})).unwrap();
        let account = account("0");
        let result = reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: Some(&exact),
                orders: &orders,
                trades: &trades,
                account: &account,
            },
        )
        .unwrap();
        assert_eq!(result.exact_cancel_working, Some(true));

        let terminal =
            serde_json::to_vec(&order("2033126385648208390", "ORDER_STATUS_CANCELED")).unwrap();
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: Some(&terminal),
                orders: &orders,
                trades: &trades,
                account: &account,
            }
        )
        .is_err());

        let wrong_id = serde_json::to_vec(&order("WRONG-ORDER", "ORDER_STATUS_NEW")).unwrap();
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: Some(&wrong_id),
                orders: &orders,
                trades: &trades,
                account: &account,
            }
        )
        .is_err());
    }

    #[test]
    fn wrong_account_instrument_and_unknown_status_fail_closed() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest, receipts, keys) = fixture(Operation::Place, now);
        let (manifest, _) =
            validate_manifest_and_local_authorities(&manifest, &receipts, &keys, now).unwrap();
        let trades = serde_json::to_vec(&serde_json::json!({"trades":[]})).unwrap();
        let account_body = account("0");
        let mut wrong_account_order = order("ORDER-1", "ORDER_STATUS_CANCELED");
        wrong_account_order["order"]["account_id"] = Value::String("OTHER".to_owned());
        let orders =
            serde_json::to_vec(&serde_json::json!({"orders":[wrong_account_order]})).unwrap();
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: &orders,
                trades: &trades,
                account: &account_body,
            }
        )
        .is_err());

        let unknown = serde_json::to_vec(&serde_json::json!({
            "orders":[order("ORDER-2", "ORDER_STATUS_UNSPECIFIED")]
        }))
        .unwrap();
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: &unknown,
                trades: &trades,
                account: &account_body,
            }
        )
        .is_err());

        let mut wrong_account: Value = serde_json::from_slice(&account_body).unwrap();
        wrong_account["account_id"] = Value::String("OTHER".to_owned());
        assert!(reduce_broker_truth(
            &manifest,
            ACCOUNT,
            BrokerTruthBodies {
                exact_order: None,
                orders: b"{\"orders\":[]}",
                trades: &trades,
                account: &serde_json::to_vec(&wrong_account).unwrap(),
            }
        )
        .is_err());
    }

    #[test]
    fn endpoint_body_caps_are_finite_and_fail_closed() {
        assert!(
            bounded_content_length(Some((AUTH_BODY_CAP + 1) as u64), 0, AUTH_BODY_CAP).is_err()
        );
        assert!(bounded_content_length(None, ORDERS_BODY_CAP + 1, ORDERS_BODY_CAP).is_err());
        assert!(
            bounded_content_length(Some(AUTH_BODY_CAP as u64), AUTH_BODY_CAP, AUTH_BODY_CAP)
                .is_ok()
        );
    }

    #[test]
    fn token_derived_raw_body_digest_is_not_part_of_attempt_receipt() {
        let left = semantic_attempt_receipt(1, "POST", "/v1/sessions", 200, 128);
        let right = semantic_attempt_receipt(1, "POST", "/v1/sessions", 200, 128);
        assert_eq!(left, right);
        assert!(!left.contains("token"));
    }

    struct TlsServer {
        address: SocketAddr,
        root_der: Vec<u8>,
        task: tokio::task::JoinHandle<bool>,
    }

    fn tls_configuration(host: &str) -> (Vec<u8>, ServerConfig) {
        let mut ca = CertificateParams::new(Vec::new()).unwrap();
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.distinguished_name
            .push(DnType::CommonName, "R2A2 controlled CA");
        ca.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        ca.not_before = date_time_ymd(2020, 1, 1);
        ca.not_after = date_time_ymd(2040, 1, 1);
        let ca_key = KeyPair::generate().unwrap();
        let ca_certificate = ca.self_signed(&ca_key).unwrap();
        let root_der = ca_certificate.der().to_vec();
        let issuer = Issuer::new(ca, ca_key);

        let mut server = CertificateParams::new(vec![host.to_owned()]).unwrap();
        server.distinguished_name.push(DnType::CommonName, host);
        server.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        server.not_before = date_time_ymd(2020, 1, 1);
        server.not_after = date_time_ymd(2035, 1, 1);
        let server_key = KeyPair::generate().unwrap();
        let server_certificate = server.signed_by(&server_key, &issuer).unwrap();
        let private_key =
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![server_certificate.der().clone()], private_key)
            .unwrap();
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        (root_der, config)
    }

    async fn tls_server(certificate_host: &str) -> TlsServer {
        let (root_der, config) = tls_configuration(certificate_host);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let Ok(mut tls) = TlsAcceptor::from(Arc::new(config)).accept(socket).await else {
                return false;
            };
            let mut request = vec![0u8; 4096];
            let Ok(count) = tls.read(&mut request).await else {
                return false;
            };
            if count == 0 {
                return false;
            }
            tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            true
        });
        TlsServer {
            address,
            root_der,
            task,
        }
    }

    fn controlled_tls_client(host: &str, address: SocketAddr, root_der: &[u8]) -> reqwest::Client {
        let root = reqwest::Certificate::from_der(root_der).unwrap();
        reqwest::Client::builder()
            .https_only(true)
            .timeout(std::time::Duration::from_secs(2))
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .tls_built_in_root_certs(false)
            .add_root_certificate(root)
            .resolve(host, address)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn standalone_helper_tls_accepts_only_matching_ca_and_hostname() {
        const HOST: &str = "stage8b-r2a2.invalid";
        let valid = tls_server(HOST).await;
        let client = controlled_tls_client(HOST, valid.address, &valid.root_der);
        let response = client
            .get(format!("https://{HOST}:{}/qualified", valid.address.port()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(valid.task.await.unwrap());

        let wrong_ca_server = tls_server(HOST).await;
        let (unrelated_root, _) = tls_configuration(HOST);
        let client = controlled_tls_client(HOST, wrong_ca_server.address, &unrelated_root);
        assert!(client
            .get(format!(
                "https://{HOST}:{}/wrong-ca",
                wrong_ca_server.address.port()
            ))
            .send()
            .await
            .is_err());
        assert!(!wrong_ca_server.task.await.unwrap());

        let wrong_host_server = tls_server("other-stage8b.invalid").await;
        let client =
            controlled_tls_client(HOST, wrong_host_server.address, &wrong_host_server.root_der);
        assert!(client
            .get(format!(
                "https://{HOST}:{}/wrong-host",
                wrong_host_server.address.port()
            ))
            .send()
            .await
            .is_err());
        assert!(!wrong_host_server.task.await.unwrap());
    }

    #[tokio::test]
    async fn controlled_pipeline_derives_broker_truth_only_after_fresh_reads() {
        let now = "2026-08-25T12:00:00Z".parse().unwrap();
        let (manifest_bytes, receipts, keys) = fixture(Operation::Place, now);
        let (manifest, local) =
            validate_manifest_and_local_authorities(&manifest_bytes, &receipts, &keys, now)
                .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for _ in 0..5 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0u8; 8192];
                let count = socket.read(&mut bytes).await.unwrap();
                let request = String::from_utf8_lossy(&bytes[..count]);
                let first = request.lines().next().unwrap().to_owned();
                requests.push(first.clone());
                let body = if first.starts_with("POST /v1/sessions/details ") {
                    serde_json::json!({
                        "account_ids":[ACCOUNT],
                        "created_at":"2026-08-25T11:59:00Z",
                        "expires_at":"2026-08-25T12:05:00Z",
                        "md_permissions":[],
                        "readonly":true
                    })
                    .to_string()
                } else if first.starts_with("POST /v1/sessions ") {
                    serde_json::json!({"token":"controlled-readonly-token"}).to_string()
                } else if first.contains("/trades?") {
                    serde_json::json!({"trades":[]}).to_string()
                } else if first.ends_with("/orders HTTP/1.1") {
                    serde_json::json!({"orders":[]}).to_string()
                } else {
                    String::from_utf8(account("0")).unwrap()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        let client = reqwest::Client::builder()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let evidence = execute_controlled_pipeline(
            &client,
            &format!("http://{address}/"),
            manifest,
            local,
            ACCOUNT,
            ACCOUNT_KEY,
            "controlled-secret",
        )
        .await
        .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 5);
        assert!(requests[0].starts_with("POST /v1/sessions "));
        assert!(requests[1].starts_with("POST /v1/sessions/details "));
        assert!(requests[2..]
            .iter()
            .all(|request| request.starts_with("GET ")));
        assert_eq!(evidence.broker_get_count, 3);
        assert!(evidence.broker_truth.approved_position_matches);
        assert!(evidence
            .request_order
            .iter()
            .all(|attempt| !attempt.raw_body_sha256_exported));
        assert!(
            !evidence
                .local_authorities
                .broker_derived_sources_accepted_pre_network
        );
        assert_eq!(evidence.authorization_status, "NOT_ISSUED");
    }
}
