//! Stage 8B-P R2A3 runnable read-only preflight qualification.
//!
//! The verifier has public Ed25519 keys only. Source-specific issuer services
//! own the private keys and read fixed, owner-pinned authority files. R2A3
//! remains GET-only after the two AuthService calls and grants no arm/effect.

use crate::r2a2::{
    self, BrokerTruthBodies, LocalAuthorityEnvelope, LocalAuthorityReceipt, R2a2Error,
    StrictAuthResponse, StrictTokenDetails, ValidatedLocalAuthorities, ValidatedManifest,
    ACCOUNT_BODY_CAP, AUTH_BODY_CAP, EXACT_ORDER_BODY_CAP, ORDERS_BODY_CAP, TRADES_BODY_CAP,
};
use crate::{NetworkClass, Operation, ReadPlan, Source, AUTH_REQUEST_BUDGET};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rcgen::{
    date_time_ymd, BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use zeroize::Zeroizing;

pub const SIGNATURE_DOMAIN: &[u8] = b"stage8b-p-r2a3-source-receipt-ed25519-v1";
pub const RUN_NONCE_DOMAIN: &str = "stage8b-p-r2a3-run-nonce-v1";
pub const CONTROL_SOURCE_MAX_SKEW_MS: i64 = 1_000;
pub const RUNTIME_SOURCE_MAX_SKEW_MS: i64 = 5_000;
pub const MIN_BROKER_GET_INTERVAL_MS: u64 = 250;
pub const PRODUCTION_RUN_PACKAGE: &str = "/etc/moex-trading/stage8b/r2a3/r2b-run-package.json";
pub const PRODUCTION_MANIFEST: &str = "/var/lib/moex-trading/stage8b/r2a3/run-manifest.json";
pub const PRODUCTION_RECEIPT_DIR: &str = "/run/moex-trading/stage8b/r2a3/receipts";
pub const PRODUCTION_PUBLIC_KEY_DIR: &str = "/etc/moex-trading/stage8b/r2a3/authority-public-keys";
pub const PRODUCTION_SOURCE_DIR: &str = "/var/lib/moex-trading/stage8b/r2a3/authority-sources";
pub const PRODUCTION_PRIVATE_KEY_DIR: &str =
    "/run/credentials/moex-trading/stage8b/r2a3/issuer-private-keys";
pub const PRODUCTION_NONCE_REGISTRY: &str = "/var/lib/moex-trading/stage8b/r2a3/used-run-nonces";
pub const PRODUCTION_ACCOUNT_ID: &str = "/run/credentials/moex-trading/stage8b/r2a3/account-id";
pub const PRODUCTION_ACCOUNT_KEY: &str =
    "/run/credentials/moex-trading/stage8b/r2a3/account-binding-key";
pub const PRODUCTION_FINAM_SECRET: &str =
    "/run/credentials/moex-trading/stage8b/r2a3/finam-readonly-secret";
pub const PRODUCTION_BASE_URL: &str = "https://api.finam.ru";

const RUNTIME_CURRENT_SOURCES: &[&str] = &["schedule", "instrument_specification"];
const RUN_AUTHORITY: &str =
    include_str!("../../../docs/stage-8/stage8b-p-r1b-run-identity-authority.json");
const READ_CONTRACT_SNAPSHOT: &[u8] =
    include_bytes!("../../../docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json");
pub(crate) const CONTROLLED_ACCOUNT: &str = "R2A3-CONTROLLED-ACCOUNT";
pub(crate) const CONTROLLED_ACCOUNT_KEY: &[u8] = b"r2a3-controlled-account-key-32b!";

const SOURCE_IDENTITIES: &[(&str, &str, &str)] = &[
    (
        "trusted_clock",
        "Stage8bTrustedClockIssuer",
        "stage8b-trusted-clock-v1",
    ),
    (
        "stage7b_current_recovery_seal",
        "Stage7bRecoverySealReader",
        "stage7b-current-recovery-seal-v1",
    ),
    (
        "stage6_exact_dispatch_ready_command",
        "Stage6DispatchReadyCommandReader",
        "stage6-dispatch-ready-command-v1",
    ),
    (
        "stage8a_root_config_policy_control",
        "Stage8aCurrentControlIssuer",
        "stage8a-root-config-policy-control-v1",
    ),
    (
        "composite_readiness",
        "Stage8aCompositeReadinessIssuer",
        "stage8a-composite-readiness-v1",
    ),
    (
        "kill_switch_run_allowed",
        "Stage8aPersistentKillSwitchIssuer",
        "stage8a-kill-switch-run-allowed-v1",
    ),
    (
        "single_finam_ownership",
        "Stage8aSingleFinamOwnershipIssuer",
        "stage8a-single-finam-ownership-v1",
    ),
    (
        "schedule",
        "Stage8aScheduleIssuer",
        "stage8a-schedule-window-v1",
    ),
    (
        "instrument_specification",
        "Stage8aInstrumentIssuer",
        "stage8a-instrument-specification-v1",
    ),
    (
        "ambiguity_orphan_unresolved_lifecycle",
        "Stage8aLifecycleAmbiguityIssuer",
        "stage8a-lifecycle-ambiguity-v1",
    ),
    (
        "durable_micro_budget",
        "Stage8aDurableMicroBudgetIssuer",
        "stage8a-durable-micro-budget-v1",
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAuthorityReceipt {
    pub receipt: LocalAuthorityReceipt,
    pub run_nonce_sha256: String,
    pub source_snapshot_sha256: String,
    pub source_generation: u64,
    pub producer_executable_sha256: String,
    pub issuer_executable_sha256: String,
    pub authoritative_store_sha256: String,
    pub source_observed_at_utc: DateTime<Utc>,
    pub produced_at_utc: DateTime<Utc>,
    pub issuer_key_id: String,
    pub signature_ed25519_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAuthorityEnvelope {
    pub schema_version: u8,
    pub run_nonce_sha256: String,
    pub receipts: Vec<SignedAuthorityReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoritySourceSnapshot {
    pub schema_version: u8,
    pub source_name: String,
    pub producer_service: String,
    pub producer_uid: u32,
    pub source_generation: u64,
    pub producer_executable_sha256: String,
    pub authoritative_store_sha256: String,
    pub run_nonce_sha256: String,
    pub source_observed_at_utc: DateTime<Utc>,
    pub produced_at_utc: DateTime<Utc>,
    pub claims: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct R2bRunPackage {
    pub schema_version: u8,
    pub authorization_status: String,
    pub run_nonce_sha256: String,
    pub helper_executable_sha256: String,
    pub contract_snapshot_sha256: String,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct R2a3AttemptEvidence {
    pub ordinal: usize,
    pub network_class: NetworkClass,
    pub method: &'static str,
    pub route_template: &'static str,
    pub request_started_at_utc: DateTime<Utc>,
    pub request_finished_at_utc: DateTime<Utc>,
    pub status: u16,
    pub response_body_len: usize,
    pub semantic_receipt_sha256: String,
    pub raw_body_sha256_exported: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct R2a3ReadonlyEvidence {
    pub schema_version: u8,
    pub run_nonce_sha256: String,
    pub operation: Operation,
    pub run_identity_sha256: String,
    pub broker_truth: r2a2::BrokerTruthSummary,
    pub request_order: Vec<R2a3AttemptEvidence>,
    pub minimum_broker_get_interval_ms: u64,
    pub final_freshness_revalidated: bool,
    pub operator_arm_issued: bool,
    pub dispatch_attempt_recorded: bool,
    pub effect_transport_entered: bool,
    pub finam_order_post_delete_sent: bool,
    pub authorization_status: &'static str,
}

#[derive(Debug, thiserror::Error)]
pub enum R2a3Error {
    #[error("R2A3 authority signature or custody is invalid")]
    Provenance,
    #[error("R2A3 freshness or chronology is invalid")]
    Freshness,
    #[error("R2A3 run package is absent or not independently authorized")]
    Authorization,
    #[error("R2A3 fixed input is invalid")]
    Input,
    #[error("R2A3 network or broker truth failed closed")]
    Network,
    #[error(transparent)]
    R2a2(#[from] R2a2Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], R2a3Error> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(R2a3Error::Provenance);
    }
    let mut output = [0u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair).map_err(|_| R2a3Error::Provenance)?;
        output[index] = u8::from_str_radix(text, 16).map_err(|_| R2a3Error::Provenance)?;
    }
    Ok(output)
}

fn receipt_signing_preimage(receipt: &SignedAuthorityReceipt) -> Result<Vec<u8>, R2a3Error> {
    let mut unsigned = receipt.clone();
    unsigned.signature_ed25519_hex.clear();
    let canonical = serde_json::to_vec(&unsigned)?;
    let mut preimage = Vec::with_capacity(SIGNATURE_DOMAIN.len() + canonical.len() + 5);
    preimage.extend_from_slice(SIGNATURE_DOMAIN);
    preimage.push(0);
    preimage.extend_from_slice(&(canonical.len() as u32).to_be_bytes());
    preimage.extend_from_slice(&canonical);
    Ok(preimage)
}

pub fn sign_authority_receipt(
    mut receipt: SignedAuthorityReceipt,
    signing_key: &SigningKey,
) -> Result<SignedAuthorityReceipt, R2a3Error> {
    receipt.signature_ed25519_hex.clear();
    let signature = signing_key.sign(&receipt_signing_preimage(&receipt)?);
    receipt.signature_ed25519_hex = lower_hex(&signature.to_bytes());
    Ok(receipt)
}

fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn adapter_key(signature_hex: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"stage8b-p-r2a3-verified-signature-adapter-v1");
    digest.update([0]);
    digest.update(signature_hex.as_bytes());
    digest.finalize().into()
}

fn validate_skew(envelope: &SignedAuthorityEnvelope) -> Result<(), R2a3Error> {
    let mut control = Vec::new();
    let mut runtime = Vec::new();
    for signed in &envelope.receipts {
        if RUNTIME_CURRENT_SOURCES.contains(&signed.receipt.source_name.as_str()) {
            runtime.push(signed.receipt.observed_at_utc);
        } else {
            control.push(signed.receipt.observed_at_utc);
        }
    }
    for (values, maximum) in [
        (&control, CONTROL_SOURCE_MAX_SKEW_MS),
        (&runtime, RUNTIME_SOURCE_MAX_SKEW_MS),
    ] {
        let minimum = values.iter().min().ok_or(R2a3Error::Freshness)?;
        let maximum_value = values.iter().max().ok_or(R2a3Error::Freshness)?;
        if maximum_value
            .signed_duration_since(*minimum)
            .num_milliseconds()
            > maximum
        {
            return Err(R2a3Error::Freshness);
        }
    }
    Ok(())
}

pub(crate) fn validate_signed_authorities(
    manifest_bytes: &[u8],
    envelope_bytes: &[u8],
    public_keys: &BTreeMap<String, VerifyingKey>,
    expected_run_nonce: &str,
    now: DateTime<Utc>,
) -> Result<(ValidatedManifest, ValidatedLocalAuthorities), R2a3Error> {
    let envelope: SignedAuthorityEnvelope = serde_json::from_slice(envelope_bytes)?;
    if envelope.schema_version != 1
        || envelope.run_nonce_sha256 != expected_run_nonce
        || decode_hex::<32>(expected_run_nonce).is_err()
        || envelope.receipts.len() != 11
    {
        return Err(R2a3Error::Provenance);
    }
    let required = r2a2::required_local_source_names().collect::<BTreeSet<_>>();
    let actual = envelope
        .receipts
        .iter()
        .map(|signed| signed.receipt.source_name.as_str())
        .collect::<BTreeSet<_>>();
    if required != actual || public_keys.len() != required.len() {
        return Err(R2a3Error::Provenance);
    }
    validate_skew(&envelope)?;
    let mut adapted = Vec::with_capacity(envelope.receipts.len());
    let mut adapter_keys = BTreeMap::new();
    for signed in envelope.receipts {
        if signed.run_nonce_sha256 != expected_run_nonce
            || !signed.receipt.authentication_tag_hmac_sha256.is_empty()
            || signed.source_observed_at_utc != signed.receipt.observed_at_utc
            || signed.produced_at_utc < signed.source_observed_at_utc
            || signed.issuer_key_id != format!("{}-ed25519-v1", signed.receipt.source_name)
            || decode_hex::<32>(&signed.source_snapshot_sha256).is_err()
            || signed.source_generation == 0
            || decode_hex::<32>(&signed.producer_executable_sha256).is_err()
            || decode_hex::<32>(&signed.issuer_executable_sha256).is_err()
            || decode_hex::<32>(&signed.authoritative_store_sha256).is_err()
        {
            return Err(R2a3Error::Provenance);
        }
        let key = public_keys
            .get(&signed.receipt.source_name)
            .ok_or(R2a3Error::Provenance)?;
        let signature_bytes = decode_hex::<64>(&signed.signature_ed25519_hex)?;
        let signature = Signature::from_bytes(&signature_bytes);
        key.verify(&receipt_signing_preimage(&signed)?, &signature)
            .map_err(|_| R2a3Error::Provenance)?;
        let adapter = adapter_key(&signed.signature_ed25519_hex);
        let mut receipt = signed.receipt;
        r2a2::authenticate_verified_receipt_adapter(&mut receipt, &adapter)?;
        adapter_keys.insert(
            receipt.source_name.clone(),
            Zeroizing::new(adapter.to_vec()),
        );
        adapted.push(receipt);
    }
    let adapted_envelope = serde_json::to_vec(&LocalAuthorityEnvelope {
        schema_version: 1,
        receipts: adapted,
    })?;
    let validated = r2a2::validate_manifest_and_local_authorities(
        manifest_bytes,
        &adapted_envelope,
        &adapter_keys,
        now,
    )?;
    let expiry = validated
        .0
        .field("run_expires_at_utc")
        .parse::<DateTime<Utc>>()
        .map_err(|_| R2a3Error::Freshness)?;
    if now >= expiry {
        return Err(R2a3Error::Freshness);
    }
    Ok(validated)
}

pub(crate) struct R2a3PipelineInput<'a> {
    pub manifest: &'a [u8],
    pub signed_authorities: &'a [u8],
    pub public_keys: &'a BTreeMap<String, VerifyingKey>,
    pub run_nonce_sha256: &'a str,
    pub account_id: &'a str,
    pub account_key: &'a [u8],
    pub secret: &'a str,
    pub authorization_status: &'static str,
}

async fn timed_response(
    request: reqwest::RequestBuilder,
    cap: usize,
) -> Result<(DateTime<Utc>, DateTime<Utc>, u16, Zeroizing<Vec<u8>>), R2a3Error> {
    let started = Utc::now();
    let response = request.send().await.map_err(|_| R2a3Error::Network)?;
    let (status, body) = r2a2::read_bounded_response(response, cap).await?;
    let finished = Utc::now();
    if finished < started {
        return Err(R2a3Error::Freshness);
    }
    Ok((started, finished, status, body))
}

struct AttemptInput {
    ordinal: usize,
    class: NetworkClass,
    method: &'static str,
    template: &'static str,
    started: DateTime<Utc>,
    finished: DateTime<Utc>,
    status: u16,
    body_len: usize,
}

fn attempt(input: AttemptInput) -> R2a3AttemptEvidence {
    R2a3AttemptEvidence {
        ordinal: input.ordinal,
        network_class: input.class,
        method: input.method,
        route_template: input.template,
        request_started_at_utc: input.started,
        request_finished_at_utc: input.finished,
        status: input.status,
        response_body_len: input.body_len,
        semantic_receipt_sha256: r2a2::semantic_attempt_receipt(
            input.ordinal,
            input.method,
            input.template,
            input.status,
            input.body_len,
        ),
        raw_body_sha256_exported: false,
    }
}

fn revalidate(
    input: &R2a3PipelineInput<'_>,
) -> Result<(ValidatedManifest, ValidatedLocalAuthorities), R2a3Error> {
    validate_signed_authorities(
        input.manifest,
        input.signed_authorities,
        input.public_keys,
        input.run_nonce_sha256,
        Utc::now(),
    )
}

pub(crate) async fn execute_r2a3_pipeline(
    auth_client: &reqwest::Client,
    broker_client: &reqwest::Client,
    base: &str,
    input: R2a3PipelineInput<'_>,
) -> Result<R2a3ReadonlyEvidence, R2a3Error> {
    if !matches!(input.authorization_status, "ISSUED" | "NOT_ISSUED") {
        return Err(R2a3Error::Authorization);
    }
    let (manifest, _) = revalidate(&input)?;
    r2a2::verify_account_binding(
        &manifest,
        input.account_id,
        &manifest.account_key_generation_id,
        input.account_key,
    )?;
    if input.secret.is_empty() {
        return Err(R2a3Error::Input);
    }
    let base_url = reqwest::Url::parse(base).map_err(|_| R2a3Error::Input)?;
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(R2a3Error::Input);
    }
    let mut attempts = Vec::new();
    let auth_url = base_url.join("v1/sessions").map_err(|_| R2a3Error::Input)?;
    let (started, finished, status, body) = timed_response(
        auth_client
            .post(auth_url)
            .json(&serde_json::json!({"secret": input.secret})),
        AUTH_BODY_CAP,
    )
    .await?;
    attempts.push(attempt(AttemptInput {
        ordinal: 1,
        class: NetworkClass::AuthService,
        method: "POST",
        template: "/v1/sessions",
        started,
        finished,
        status,
        body_len: body.len(),
    }));
    if status != 200 {
        return Err(R2a3Error::Network);
    }
    let auth: StrictAuthResponse = serde_json::from_slice(&body)?;
    let token = Zeroizing::new(auth.token);
    if token.is_empty() {
        return Err(R2a3Error::Network);
    }
    let details_url = base_url
        .join("v1/sessions/details")
        .map_err(|_| R2a3Error::Input)?;
    let (started, finished, status, body) = timed_response(
        auth_client
            .post(details_url)
            .json(&serde_json::json!({"token": token.as_str()})),
        AUTH_BODY_CAP,
    )
    .await?;
    attempts.push(attempt(AttemptInput {
        ordinal: 2,
        class: NetworkClass::AuthService,
        method: "POST",
        template: "/v1/sessions/details",
        started,
        finished,
        status,
        body_len: body.len(),
    }));
    if status != 200 {
        return Err(R2a3Error::Network);
    }
    let details: StrictTokenDetails = serde_json::from_slice(&body)?;
    r2a2::validate_token_details(details, input.account_id, Utc::now())?;

    let (manifest, _) = revalidate(&input)?;
    let plan = ReadPlan {
        operation: manifest.operation,
        run_identity_sha256: manifest.run_identity_sha256.clone(),
        broker_order_id: manifest.broker_order_id.clone(),
        sources: match manifest.operation {
            Operation::Place => vec![
                Source::OrdersSnapshot,
                Source::TradesSnapshot,
                Source::PositionSnapshot,
            ],
            Operation::Cancel => vec![
                Source::GetOrder,
                Source::OrdersSnapshot,
                Source::TradesSnapshot,
                Source::PositionSnapshot,
            ],
        },
    };
    let mut exact_order = None;
    let mut orders = None;
    let mut trades = None;
    let mut account = None;
    let mut previous_get_started: Option<Instant> = None;
    for (index, source) in plan.sources.iter().copied().enumerate() {
        let (current, local) = revalidate(&input)?;
        if current.run_identity_sha256 != manifest.run_identity_sha256 {
            return Err(R2a3Error::Freshness);
        }
        if let Some(previous) = previous_get_started {
            let minimum = Duration::from_millis(MIN_BROKER_GET_INTERVAL_MS);
            let elapsed = previous.elapsed();
            if elapsed < minimum {
                tokio::time::sleep(minimum - elapsed).await;
            }
        }
        revalidate(&input)?;
        previous_get_started = Some(Instant::now());
        let (url, template) = crate::route(
            &base_url,
            source,
            input.account_id,
            plan.broker_order_id.as_deref(),
            local.trusted_now_utc,
        )
        .map_err(|_| R2a3Error::Input)?;
        let cap = match source {
            Source::GetOrder => EXACT_ORDER_BODY_CAP,
            Source::OrdersSnapshot => ORDERS_BODY_CAP,
            Source::TradesSnapshot => TRADES_BODY_CAP,
            Source::PositionSnapshot => ACCOUNT_BODY_CAP,
        };
        let (started, finished, status, body) =
            timed_response(broker_client.get(url).bearer_auth(token.as_str()), cap).await?;
        let ordinal = AUTH_REQUEST_BUDGET + index + 1;
        attempts.push(attempt(AttemptInput {
            ordinal,
            class: NetworkClass::BrokerTruth,
            method: "GET",
            template,
            started,
            finished,
            status,
            body_len: body.len(),
        }));
        if status != 200 {
            return Err(R2a3Error::Network);
        }
        match source {
            Source::GetOrder => exact_order = Some(body),
            Source::OrdersSnapshot => orders = Some(body),
            Source::TradesSnapshot => trades = Some(body),
            Source::PositionSnapshot => account = Some(body),
        }
    }
    let (final_manifest, _) = revalidate(&input)?;
    if final_manifest.run_identity_sha256 != manifest.run_identity_sha256 {
        return Err(R2a3Error::Freshness);
    }
    let broker_truth = r2a2::reduce_broker_truth(
        &manifest,
        input.account_id,
        BrokerTruthBodies {
            exact_order: exact_order.as_ref().map(|body| body.as_slice()),
            orders: orders.as_deref().ok_or(R2a3Error::Network)?,
            trades: trades.as_deref().ok_or(R2a3Error::Network)?,
            account: account.as_deref().ok_or(R2a3Error::Network)?,
        },
    )?;
    Ok(R2a3ReadonlyEvidence {
        schema_version: 3,
        run_nonce_sha256: input.run_nonce_sha256.to_owned(),
        operation: manifest.operation,
        run_identity_sha256: manifest.run_identity_sha256,
        broker_truth,
        request_order: attempts,
        minimum_broker_get_interval_ms: MIN_BROKER_GET_INTERVAL_MS,
        final_freshness_revalidated: true,
        operator_arm_issued: false,
        dispatch_attempt_recorded: false,
        effect_transport_entered: false,
        finam_order_post_delete_sent: false,
        authorization_status: input.authorization_status,
    })
}

fn safe_component(value: &str) -> Result<&str, R2a3Error> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(R2a3Error::Input);
    }
    Ok(value)
}

pub(crate) fn source_issuer_uid(source_name: &str) -> Result<u32, R2a3Error> {
    let source_index = SOURCE_IDENTITIES
        .iter()
        .position(|(name, _, _)| *name == source_name)
        .ok_or(R2a3Error::Input)?;
    8_201u32
        .checked_add(u32::try_from(source_index).map_err(|_| R2a3Error::Input)?)
        .ok_or(R2a3Error::Input)
}

pub(crate) fn source_producer_uid(source_name: &str) -> Result<u32, R2a3Error> {
    let source_index = SOURCE_IDENTITIES
        .iter()
        .position(|(name, _, _)| *name == source_name)
        .ok_or(R2a3Error::Input)?;
    8_101u32
        .checked_add(u32::try_from(source_index).map_err(|_| R2a3Error::Input)?)
        .ok_or(R2a3Error::Input)
}

pub(crate) fn expected_claim_names(source_name: &str) -> Result<BTreeSet<&'static str>, R2a3Error> {
    let names: &[&str] = match source_name {
        "trusted_clock" => &["trusted_now_utc", "process_boot_fingerprint_sha256"],
        "stage7b_current_recovery_seal" => {
            &["stage7b_seal_generation", "stage6_checkpoint_fingerprint"]
        }
        "stage6_exact_dispatch_ready_command" => &[
            "strategy_request_id",
            "durable_client_order_id",
            "operation",
            "request_body_sha256",
            "cancel_target_broker_order_id",
            "cancel_target_lifecycle_fingerprint",
            "cancel_target_currently_working_proof_sha256",
        ],
        "stage8a_root_config_policy_control" => &[
            "config_sha256",
            "policy_sha256",
            "config_policy_authority_sha256",
        ],
        "composite_readiness" => &["ready"],
        "kill_switch_run_allowed" => &["run_allowed", "kill_switch_generation"],
        "single_finam_ownership" => &["single_owner", "ownership_lease_fingerprint"],
        "schedule" => &["eligible"],
        "instrument_specification" => &["instrument", "eligible"],
        "ambiguity_orphan_unresolved_lifecycle" => &["clear"],
        "durable_micro_budget" => &["available", "durable_budget_generation"],
        _ => return Err(R2a3Error::Input),
    };
    Ok(names.iter().copied().collect())
}

fn validate_owner_snapshot(
    source: &AuthoritySourceSnapshot,
    source_name: &str,
    run_nonce: &str,
) -> Result<(), R2a3Error> {
    let expected_uid = source_producer_uid(source_name)?;
    let expected_service = format!("moex-stage8b-r2a3-source-{source_name}.service");
    let actual_claims = source
        .claims
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut expected_claims = expected_claim_names(source_name)?;
    if source_name == "stage6_exact_dispatch_ready_command"
        && source.claims.get("operation").map(String::as_str) == Some("PLACE")
    {
        expected_claims.remove("cancel_target_broker_order_id");
        expected_claims.remove("cancel_target_lifecycle_fingerprint");
        expected_claims.remove("cancel_target_currently_working_proof_sha256");
    }
    if source.schema_version != 1
        || source.source_name != source_name
        || source.producer_service != expected_service
        || source.producer_uid != expected_uid
        || source.source_generation == 0
        || decode_hex::<32>(&source.producer_executable_sha256).is_err()
        || decode_hex::<32>(&source.authoritative_store_sha256).is_err()
        || source.run_nonce_sha256 != run_nonce
        || actual_claims != expected_claims
    {
        return Err(R2a3Error::Provenance);
    }
    Ok(())
}

fn read_regular_file(
    path: &Path,
    cap: usize,
    secret: bool,
) -> Result<Zeroizing<Vec<u8>>, R2a3Error> {
    let mut options = OpenOptions::new();
    // The verified descriptor must survive execveat for a dynamically linked
    // ELF. O_CLOEXEC here makes Linux return ENOENT while resolving its loader.
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || (secret && metadata.mode() & 0o077 != 0)
        || (secret && metadata.uid() != unsafe { libc::geteuid() })
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

fn require_owned_file(path: &Path, expected_uid: u32, secret: bool) -> Result<(), R2a3Error> {
    let metadata = path.symlink_metadata()?;
    let forbidden_mode = if secret { 0o077 } else { 0o022 };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != expected_uid
        || metadata.mode() & forbidden_mode != 0
    {
        return Err(R2a3Error::Input);
    }
    Ok(())
}

fn require_owned_directory(path: &Path, expected_uid: u32) -> Result<(), R2a3Error> {
    let metadata = path.symlink_metadata()?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o022 != 0
    {
        return Err(R2a3Error::Input);
    }
    Ok(())
}

pub fn load_public_keys(directory: &Path) -> Result<BTreeMap<String, VerifyingKey>, R2a3Error> {
    require_owned_directory(directory, 0)?;
    let mut keys = BTreeMap::new();
    for source in r2a2::required_local_source_names() {
        let path = directory.join(format!("{source}.ed25519.pub"));
        require_owned_file(&path, 0, false)?;
        let bytes = read_regular_file(&path, 128, false)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| R2a3Error::Input)?
            .trim();
        let key = VerifyingKey::from_bytes(&decode_hex::<32>(text)?)
            .map_err(|_| R2a3Error::Provenance)?;
        keys.insert(source.to_owned(), key);
    }
    Ok(keys)
}

pub fn issue_from_fixed_source(source_name: &str) -> Result<(), R2a3Error> {
    let source_name = safe_component(source_name)?;
    if !r2a2::required_local_source_names().any(|name| name == source_name) {
        return Err(R2a3Error::Input);
    }
    let source_path = Path::new(PRODUCTION_SOURCE_DIR)
        .join(source_name)
        .join("source.json");
    let key_path = Path::new(PRODUCTION_PRIVATE_KEY_DIR).join(format!("{source_name}.ed25519"));
    let output_path = Path::new(PRODUCTION_RECEIPT_DIR)
        .join(source_name)
        .join("receipt.json");
    let issuer_uid = source_issuer_uid(source_name)?;
    if unsafe { libc::geteuid() } != issuer_uid {
        return Err(R2a3Error::Provenance);
    }
    require_owned_file(&source_path, source_producer_uid(source_name)?, false)?;
    require_owned_file(&key_path, issuer_uid, true)?;
    require_owned_directory(output_path.parent().ok_or(R2a3Error::Input)?, issuer_uid)?;
    let source_bytes = read_regular_file(&source_path, 64 * 1024, false)?;
    let source: AuthoritySourceSnapshot = serde_json::from_slice(&source_bytes)?;
    let key_bytes = read_regular_file(&key_path, 128, true)?;
    let key_text = std::str::from_utf8(&key_bytes)
        .map_err(|_| R2a3Error::Input)?
        .trim();
    let signing_key = SigningKey::from_bytes(&decode_hex::<32>(key_text)?);
    require_owned_file(Path::new(PRODUCTION_MANIFEST), 0, false)?;
    let manifest_bytes = read_regular_file(Path::new(PRODUCTION_MANIFEST), 256 * 1024, false)?;
    let fields: BTreeMap<String, String> = serde_json::from_slice(&manifest_bytes)?;
    let run_nonce_path = Path::new("/run/moex-trading/stage8b/r2a3/run-nonce.sha256");
    require_owned_file(run_nonce_path, 0, false)?;
    let run_nonce = read_regular_file(run_nonce_path, 128, false)?;
    let run_nonce = std::str::from_utf8(&run_nonce)
        .map_err(|_| R2a3Error::Input)?
        .trim();
    decode_hex::<32>(run_nonce)?;
    validate_owner_snapshot(&source, source_name, run_nonce)?;
    let (_, issuer, schema) = SOURCE_IDENTITIES
        .iter()
        .find(|(name, _, _)| *name == source_name)
        .ok_or(R2a3Error::Input)?;
    let receipt = LocalAuthorityReceipt {
        source_name: source_name.to_owned(),
        issuer: (*issuer).to_owned(),
        evidence_schema: (*schema).to_owned(),
        observed_at_utc: source.source_observed_at_utc,
        key_generation_id: r2a2::LOCAL_RECEIPT_KEY_GENERATION_ID.to_owned(),
        run_identity_sha256: fields
            .get("run_identity_sha256")
            .cloned()
            .ok_or(R2a3Error::Input)?,
        keyed_account_binding_hmac_sha256: fields
            .get("keyed_account_binding_hmac_sha256")
            .cloned()
            .ok_or(R2a3Error::Input)?,
        execution_build_identity_sha256: fields
            .get("execution_build_identity_sha256")
            .cloned()
            .ok_or(R2a3Error::Input)?,
        claims: source.claims,
        authentication_tag_hmac_sha256: String::new(),
    };
    let signed = sign_authority_receipt(
        SignedAuthorityReceipt {
            receipt,
            run_nonce_sha256: run_nonce.to_owned(),
            source_snapshot_sha256: sha256(&source_bytes),
            source_generation: source.source_generation,
            producer_executable_sha256: source.producer_executable_sha256,
            issuer_executable_sha256: current_linux_executable_sha256()?,
            authoritative_store_sha256: source.authoritative_store_sha256,
            source_observed_at_utc: source.source_observed_at_utc,
            produced_at_utc: source.produced_at_utc,
            issuer_key_id: format!("{source_name}-ed25519-v1"),
            signature_ed25519_hex: String::new(),
        },
        &signing_key,
    )?;
    let parent = output_path.parent().ok_or(R2a3Error::Input)?;
    let temporary = parent.join(format!(".{source_name}.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o644);
    let mut output = options.open(&temporary)?;
    output.write_all(&serde_json::to_vec(&signed)?)?;
    output.sync_all()?;
    output.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    std::fs::rename(&temporary, &output_path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn cstring(value: &OsStr) -> Result<CString, R2a3Error> {
    CString::new(value.as_bytes()).map_err(|_| R2a3Error::Input)
}

pub fn verified_exec(
    helper: &Path,
    accepted_sha256: &str,
    arguments: &[CString],
    environment: &[CString],
) -> Result<(), R2a3Error> {
    decode_hex::<32>(accepted_sha256)?;
    let parent = helper.parent().ok_or(R2a3Error::Input)?;
    let parent_meta = parent.metadata()?;
    if !parent_meta.is_dir() || parent_meta.uid() != 0 || parent_meta.mode() & 0o022 != 0 {
        return Err(R2a3Error::Input);
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let mut file = options.open(helper)?;
    let before = file.metadata()?;
    if !before.is_file()
        || before.nlink() != 1
        || before.uid() != 0
        || before.mode() & 0o022 != 0
        || before.mode() & 0o111 == 0
    {
        return Err(R2a3Error::Input);
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if lower_hex(&digest.finalize()) != accepted_sha256 {
        return Err(R2a3Error::Authorization);
    }
    let after = file.metadata()?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
    {
        return Err(R2a3Error::Input);
    }
    #[cfg(target_os = "linux")]
    {
        let argv = arguments
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect::<Vec<_>>();
        let envp = environment
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect::<Vec<_>>();
        let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
        let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if descriptor_flags == -1
            || unsafe { libc::fcntl(fd, libc::F_SETFD, descriptor_flags & !libc::FD_CLOEXEC) } == -1
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let result = unsafe {
            libc::execveat(
                fd,
                c"".as_ptr(),
                argv.as_ptr(),
                envp.as_ptr(),
                libc::AT_EMPTY_PATH,
            )
        };
        if result == -1 {
            return Err(std::io::Error::last_os_error().into());
        }
        unreachable!()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (file, arguments, environment);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "fd-bound execution is supported only on the Linux deployment target",
        )
        .into())
    }
}

pub fn path_argument(value: &Path) -> Result<CString, R2a3Error> {
    cstring(value.as_os_str())
}

pub async fn run_r2b_one_shot() -> Result<R2a3ReadonlyEvidence, R2a3Error> {
    require_owned_file(Path::new(PRODUCTION_RUN_PACKAGE), 0, false)
        .map_err(|_| R2a3Error::Authorization)?;
    let package_bytes = read_regular_file(Path::new(PRODUCTION_RUN_PACKAGE), 64 * 1024, false)
        .map_err(|_| R2a3Error::Authorization)?;
    let package: R2bRunPackage = serde_json::from_slice(&package_bytes)?;
    if package.schema_version != 1 || package.authorization_status != "ISSUED" {
        return Err(R2a3Error::Authorization);
    }
    if package.contract_snapshot_sha256 != sha256(READ_CONTRACT_SNAPSHOT)
        || package.helper_executable_sha256 != current_linux_executable_sha256()?
    {
        return Err(R2a3Error::Authorization);
    }
    claim_run_nonce_once(
        Path::new(PRODUCTION_NONCE_REGISTRY),
        &package.run_nonce_sha256,
    )?;
    // R2A3 intentionally ships no ISSUED package. Once independently accepted,
    // R2B supplies the exact package and launcher digest without rebuilding this helper.
    require_owned_file(Path::new(PRODUCTION_MANIFEST), 0, false)?;
    let manifest = read_regular_file(Path::new(PRODUCTION_MANIFEST), 256 * 1024, false)?;
    let manifest_operation = serde_json::from_slice::<BTreeMap<String, String>>(&manifest)?
        .get("operation")
        .cloned()
        .ok_or(R2a3Error::Authorization)?;
    let package_operation = match package.operation {
        Operation::Place => "PLACE",
        Operation::Cancel => "CANCEL",
    };
    if manifest_operation != package_operation {
        return Err(R2a3Error::Authorization);
    }
    let receipts = assemble_production_receipts(&package.run_nonce_sha256)?;
    let public_keys = load_public_keys(Path::new(PRODUCTION_PUBLIC_KEY_DIR))?;
    let account_id = read_regular_file(Path::new(PRODUCTION_ACCOUNT_ID), 4096, true)?;
    let account_key = read_regular_file(Path::new(PRODUCTION_ACCOUNT_KEY), 4096, true)?;
    let secret = read_regular_file(Path::new(PRODUCTION_FINAM_SECRET), 4096, true)?;
    let account_id = std::str::from_utf8(&account_id)
        .map_err(|_| R2a3Error::Input)?
        .trim();
    let secret = std::str::from_utf8(&secret)
        .map_err(|_| R2a3Error::Input)?
        .trim();
    let account_key_text = std::str::from_utf8(&account_key)
        .map_err(|_| R2a3Error::Input)?
        .trim();
    let account_key = decode_hex::<32>(account_key_text)?;
    let (auth_client, broker_client) =
        crate::production_clients().map_err(|_| R2a3Error::Network)?;
    execute_r2a3_pipeline(
        &auth_client,
        &broker_client,
        PRODUCTION_BASE_URL,
        R2a3PipelineInput {
            manifest: &manifest,
            signed_authorities: &receipts,
            public_keys: &public_keys,
            run_nonce_sha256: &package.run_nonce_sha256,
            account_id,
            account_key: &account_key,
            secret,
            authorization_status: "ISSUED",
        },
    )
    .await
}

fn current_linux_executable_sha256() -> Result<String, R2a3Error> {
    #[cfg(target_os = "linux")]
    {
        let mut file = File::open("/proc/self/exe")?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        Ok(lower_hex(&digest.finalize()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(R2a3Error::Authorization)
    }
}

fn assemble_production_receipts(run_nonce: &str) -> Result<Vec<u8>, R2a3Error> {
    let mut receipts = Vec::new();
    for source in r2a2::required_local_source_names() {
        let path = Path::new(PRODUCTION_RECEIPT_DIR)
            .join(source)
            .join("receipt.json");
        require_owned_file(&path, source_issuer_uid(source)?, false)?;
        receipts.push(serde_json::from_slice::<SignedAuthorityReceipt>(
            &read_regular_file(&path, 128 * 1024, false)?,
        )?);
    }
    Ok(serde_json::to_vec(&SignedAuthorityEnvelope {
        schema_version: 1,
        run_nonce_sha256: run_nonce.to_owned(),
        receipts,
    })?)
}

fn claim_run_nonce_once(directory: &Path, run_nonce: &str) -> Result<(), R2a3Error> {
    claim_run_nonce_once_for_uid(directory, run_nonce, 0)
}

fn claim_run_nonce_once_for_uid(
    directory: &Path,
    run_nonce: &str,
    expected_uid: u32,
) -> Result<(), R2a3Error> {
    decode_hex::<32>(run_nonce)?;
    require_owned_directory(directory, expected_uid)?;
    let marker = directory.join(run_nonce);
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let mut file = options.open(marker).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            R2a3Error::Authorization
        } else {
            R2a3Error::Io(error)
        }
    })?;
    file.write_all(b"stage8b-p-r2a3-run-nonce-consumed-v1\n")?;
    file.sync_all()?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

pub(crate) type ControlledFixture = (Vec<u8>, Vec<u8>, BTreeMap<String, VerifyingKey>, String);

#[cfg(test)]
fn controlled_fixture(now: DateTime<Utc>) -> Result<ControlledFixture, R2a3Error> {
    controlled_fixture_for(now, Operation::Place)
}

pub(crate) fn controlled_fixture_for(
    now: DateTime<Utc>,
    operation: Operation,
) -> Result<ControlledFixture, R2a3Error> {
    controlled_fixture_for_boot(now, operation, None)
}

pub(crate) fn controlled_fixture_for_boot(
    now: DateTime<Utc>,
    operation: Operation,
    process_boot_fingerprint_sha256: Option<&str>,
) -> Result<ControlledFixture, R2a3Error> {
    let authority: serde_json::Value = serde_json::from_str(RUN_AUTHORITY)?;
    let operation_name = match operation {
        Operation::Place => "PLACE",
        Operation::Cancel => "CANCEL",
    };
    let mut fields = authority["golden_vectors"][operation_name]
        ["manifest_without_run_identity_sha256"]
        .as_object()
        .ok_or(R2a3Error::Input)?
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                value.as_str().ok_or(R2a3Error::Input)?.to_owned(),
            ))
        })
        .collect::<Result<BTreeMap<String, String>, R2a3Error>>()?;
    let account_hmac = r2a2::keyed_account_binding(CONTROLLED_ACCOUNT, CONTROLLED_ACCOUNT_KEY)?;
    fields.insert(
        "keyed_account_binding_hmac_sha256".to_owned(),
        account_hmac.clone(),
    );
    fields.insert(
        "endpoint_identity_sha256".to_owned(),
        r2a2::endpoint_identity(
            operation,
            &account_hmac,
            &fields["endpoint_renderer_sha256"],
        )?,
    );
    fields.insert(
        "run_expires_at_utc".to_owned(),
        r2a2::exact_millis(now + chrono::Duration::seconds(10)),
    );
    if let Some(fingerprint) = process_boot_fingerprint_sha256 {
        if decode_hex::<32>(fingerprint).is_err() {
            return Err(R2a3Error::Input);
        }
        fields.insert(
            "process_boot_fingerprint_sha256".to_owned(),
            fingerprint.to_owned(),
        );
    }
    let common = authority["run_identity"]["common_fields_in_exact_order_excluding_run_identity"]
        .as_array()
        .ok_or(R2a3Error::Input)?;
    let variant_name = match operation {
        Operation::Place => "place_fields_in_exact_order",
        Operation::Cancel => "cancel_fields_in_exact_order",
    };
    let variant = authority["run_identity"][variant_name]
        .as_array()
        .ok_or(R2a3Error::Input)?;
    let values = common
        .iter()
        .chain(variant)
        .map(|field| {
            fields
                .get(field.as_str().ok_or(R2a3Error::Input)?)
                .map(String::as_str)
                .ok_or(R2a3Error::Input)
        })
        .collect::<Result<Vec<_>, R2a3Error>>()?;
    let domain = authority["run_identity"]["domain_utf8"]
        .as_str()
        .ok_or(R2a3Error::Input)?;
    let run_identity = crate::digest_parts(domain, &values);
    fields.insert("run_identity_sha256".to_owned(), run_identity.clone());
    let run_nonce =
        crate::digest_parts(RUN_NONCE_DOMAIN, &[&run_identity, &r2a2::exact_millis(now)]);

    let mut receipts = Vec::new();
    let mut public_keys = BTreeMap::new();
    for (index, (source, issuer, schema)) in SOURCE_IDENTITIES.iter().enumerate() {
        let claims = match *source {
            "trusted_clock" => BTreeMap::from([
                ("trusted_now_utc".to_owned(), r2a2::exact_millis(now)),
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
                let mut claims = BTreeMap::from([
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
                    for name in [
                        "cancel_target_broker_order_id",
                        "cancel_target_lifecycle_fingerprint",
                        "cancel_target_currently_working_proof_sha256",
                    ] {
                        claims.insert(name.to_owned(), fields[name].clone());
                    }
                }
                claims
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
                ("instrument".to_owned(), r2a2::TARGET_INSTRUMENT.to_owned()),
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
            _ => return Err(R2a3Error::Input),
        };
        let receipt = LocalAuthorityReceipt {
            source_name: (*source).to_owned(),
            issuer: (*issuer).to_owned(),
            evidence_schema: (*schema).to_owned(),
            observed_at_utc: now,
            key_generation_id: "1".to_owned(),
            run_identity_sha256: run_identity.clone(),
            keyed_account_binding_hmac_sha256: account_hmac.clone(),
            execution_build_identity_sha256: fields["execution_build_identity_sha256"].clone(),
            claims,
            authentication_tag_hmac_sha256: String::new(),
        };
        let signing = SigningKey::from_bytes(&[index as u8 + 1; 32]);
        public_keys.insert((*source).to_owned(), signing.verifying_key());
        receipts.push(sign_authority_receipt(
            SignedAuthorityReceipt {
                receipt,
                run_nonce_sha256: run_nonce.clone(),
                source_snapshot_sha256: sha256(source.as_bytes()),
                source_generation: u64::try_from(index + 1).map_err(|_| R2a3Error::Input)?,
                producer_executable_sha256: sha256(
                    format!("controlled-producer-{source}").as_bytes(),
                ),
                issuer_executable_sha256: sha256(format!("controlled-issuer-{source}").as_bytes()),
                authoritative_store_sha256: sha256(format!("controlled-store-{source}").as_bytes()),
                source_observed_at_utc: now,
                produced_at_utc: now,
                issuer_key_id: format!("{source}-ed25519-v1"),
                signature_ed25519_hex: String::new(),
            },
            &signing,
        )?);
    }
    let manifest = serde_json::to_vec(&fields)?;
    let envelope = serde_json::to_vec(&SignedAuthorityEnvelope {
        schema_version: 1,
        run_nonce_sha256: run_nonce.clone(),
        receipts,
    })?;
    Ok((manifest, envelope, public_keys, run_nonce))
}

pub(crate) fn controlled_account_body() -> String {
    serde_json::json!({
        "account_id": CONTROLLED_ACCOUNT,
        "type": "ACCOUNT_TYPE_UNSPECIFIED",
        "status": "ACCOUNT_STATUS_ACTIVE",
        "equity": {"value":"100000"},
        "unrealized_profit": {"value":"0"},
        "positions": [],
        "cash": [{"currency_code":"RUB","units":"100000","nanos":0}],
        "portfolio_mc": {
            "available_cash":{"value":"100000"},
            "initial_margin":{"value":"0"},
            "maintenance_margin":{"value":"0"}
        },
        "portfolio_mct": "0",
        "portfolio_forts": {
            "available_cash":{"value":"100000"},
            "money_reserved":{"value":"0"}
        },
        "open_account_date":"2020-01-01T00:00:00Z",
        "first_trade_date":"2020-01-02T00:00:00Z",
        "first_non_trade_date":"2020-01-03T00:00:00Z"
    })
    .to_string()
}

pub(crate) fn controlled_cancel_order() -> serde_json::Value {
    serde_json::json!({
        "order_id": "2033126385648208390",
        "exec_id": "CONTROLLED-EXEC-1",
        "status": "ORDER_STATUS_WORKING",
        "order": {
            "account_id": CONTROLLED_ACCOUNT,
            "symbol": r2a2::TARGET_INSTRUMENT,
            "quantity": {"value":"1"},
            "side": "SIDE_BUY",
            "type": "ORDER_TYPE_LIMIT",
            "time_in_force": "TIME_IN_FORCE_DAY",
            "limit_price": {"value":"2210"},
            "stop_price": null,
            "stop_condition": null,
            "legs": [],
            "client_order_id": "S8BP000000000000001",
            "valid_before": null,
            "comment": "S8BP000000000000001"
        },
        "transact_at": "2026-08-25T12:00:00Z",
        "accept_at": "2026-08-25T12:00:00Z",
        "withdraw_at": null,
        "initial_quantity": {"value":"1"},
        "executed_quantity": {"value":"0"},
        "remaining_quantity": {"value":"1"},
        "sltp_order": null,
        "triggered_order_id": null
    })
}

pub(crate) fn controlled_tls_configuration(
    host: &str,
) -> Result<(Vec<u8>, ServerConfig), R2a3Error> {
    let mut ca = CertificateParams::new(Vec::new()).map_err(|_| R2a3Error::Input)?;
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca.distinguished_name
        .push(DnType::CommonName, "R2A3 controlled CA");
    ca.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    ca.not_before = date_time_ymd(2020, 1, 1);
    ca.not_after = date_time_ymd(2040, 1, 1);
    let ca_key = KeyPair::generate().map_err(|_| R2a3Error::Input)?;
    let ca_certificate = ca.self_signed(&ca_key).map_err(|_| R2a3Error::Input)?;
    let root_der = ca_certificate.der().to_vec();
    let issuer = Issuer::new(ca, ca_key);
    let mut server = CertificateParams::new(vec![host.to_owned()]).map_err(|_| R2a3Error::Input)?;
    server.distinguished_name.push(DnType::CommonName, host);
    server.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server.not_before = date_time_ymd(2020, 1, 1);
    server.not_after = date_time_ymd(2035, 1, 1);
    let server_key = KeyPair::generate().map_err(|_| R2a3Error::Input)?;
    let server_certificate = server
        .signed_by(&server_key, &issuer)
        .map_err(|_| R2a3Error::Input)?;
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(server_key.serialize_der()));
    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![server_certificate.der().clone()], private_key)
        .map_err(|_| R2a3Error::Input)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok((root_der, config))
}

fn controlled_client(
    host: &str,
    address: std::net::SocketAddr,
    root_der: &[u8],
) -> Result<reqwest::Client, R2a3Error> {
    let root = reqwest::Certificate::from_der(root_der).map_err(|_| R2a3Error::Input)?;
    crate::hardened_client_builder(true, Duration::from_secs(2))
        .tls_built_in_root_certs(false)
        .add_root_certificate(root)
        .resolve(host, address)
        .build()
        .map_err(|_| R2a3Error::Input)
}

async fn controlled_qualification_evidence_for(
    operation: Operation,
) -> Result<R2a3ReadonlyEvidence, R2a3Error> {
    const HOST: &str = "stage8b-r2a3.invalid";
    let now = Utc::now();
    let (manifest, receipts, public_keys, run_nonce) = controlled_fixture_for(now, operation)?;
    let (root_der, tls_config) = controlled_tls_configuration(HOST)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let acceptor = TlsAcceptor::from(Arc::new(tls_config));
        let mut request_lines = Vec::new();
        let request_count = match operation {
            Operation::Place => 5,
            Operation::Cancel => 6,
        };
        for _ in 0..request_count {
            let (socket, _) = listener.accept().await.map_err(|_| R2a3Error::Network)?;
            let mut tls = acceptor
                .accept(socket)
                .await
                .map_err(|_| R2a3Error::Network)?;
            let mut bytes = [0u8; 16 * 1024];
            let count = tls.read(&mut bytes).await.map_err(|_| R2a3Error::Network)?;
            let request = String::from_utf8_lossy(&bytes[..count]);
            let first = request.lines().next().ok_or(R2a3Error::Network)?.to_owned();
            request_lines.push(first.clone());
            let body = if first.starts_with("POST /v1/sessions/details ") {
                serde_json::json!({
                    "created_at": (now - chrono::Duration::minutes(1)).to_rfc3339(),
                    "expires_at": (now + chrono::Duration::minutes(5)).to_rfc3339(),
                    "md_permissions": [],
                    "account_ids": [CONTROLLED_ACCOUNT],
                    "readonly": true
                })
                .to_string()
            } else if first.starts_with("POST /v1/sessions ") {
                serde_json::json!({"token":"controlled-readonly-token"}).to_string()
            } else if first.contains("/trades?") {
                serde_json::json!({"trades":[]}).to_string()
            } else if first.contains("/orders/2033126385648208390") {
                controlled_cancel_order().to_string()
            } else if first.ends_with("/orders HTTP/1.1") {
                match operation {
                    Operation::Place => serde_json::json!({"orders":[]}).to_string(),
                    Operation::Cancel => {
                        serde_json::json!({"orders":[controlled_cancel_order()]}).to_string()
                    }
                }
            } else {
                controlled_account_body()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            tls.write_all(response.as_bytes())
                .await
                .map_err(|_| R2a3Error::Network)?;
        }
        Ok::<_, R2a3Error>(request_lines)
    });
    let client = controlled_client(HOST, address, &root_der)?;
    let evidence = execute_r2a3_pipeline(
        &client,
        &client,
        &format!("https://{HOST}:{}/", address.port()),
        R2a3PipelineInput {
            manifest: &manifest,
            signed_authorities: &receipts,
            public_keys: &public_keys,
            run_nonce_sha256: &run_nonce,
            account_id: CONTROLLED_ACCOUNT,
            account_key: CONTROLLED_ACCOUNT_KEY,
            secret: "controlled-secret-not-a-real-credential",
            authorization_status: "NOT_ISSUED",
        },
    )
    .await?;
    let requests = server.await.map_err(|_| R2a3Error::Network)??;
    let exact = [
        "POST /v1/sessions HTTP/1.1",
        "POST /v1/sessions/details HTTP/1.1",
    ];
    let expected_requests = match operation {
        Operation::Place => 5,
        Operation::Cancel => 6,
    };
    if requests.len() != expected_requests
        || requests[0] != exact[0]
        || requests[1] != exact[1]
        || evidence.request_order.len() != expected_requests
        || evidence.operation != operation
        || !evidence.final_freshness_revalidated
        || evidence.authorization_status != "NOT_ISSUED"
    {
        return Err(R2a3Error::Network);
    }
    Ok(evidence)
}

pub async fn run_controlled_qualification() -> Result<(), R2a3Error> {
    let _ = controlled_qualification_evidence_for(Operation::Place).await?;
    let _ = controlled_qualification_evidence_for(Operation::Cancel).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn assert_pipeline_tls_rejected(
        certificate_host: &str,
        client_host: &str,
        use_wrong_ca: bool,
    ) {
        let now = Utc::now();
        let (manifest, receipts, public_keys, run_nonce) = controlled_fixture(now).unwrap();
        let (root_der, server_config) = controlled_tls_configuration(certificate_host).unwrap();
        let client_root = if use_wrong_ca {
            controlled_tls_configuration("untrusted-r2a3.invalid")
                .unwrap()
                .0
        } else {
            root_der
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            TlsAcceptor::from(Arc::new(server_config))
                .accept(socket)
                .await
                .is_err()
        });
        let client = controlled_client(client_host, address, &client_root).unwrap();
        let result = execute_r2a3_pipeline(
            &client,
            &client,
            &format!("https://{client_host}:{}/", address.port()),
            R2a3PipelineInput {
                manifest: &manifest,
                signed_authorities: &receipts,
                public_keys: &public_keys,
                run_nonce_sha256: &run_nonce,
                account_id: CONTROLLED_ACCOUNT,
                account_key: CONTROLLED_ACCOUNT_KEY,
                secret: "controlled-secret-not-a-real-credential",
                authorization_status: "NOT_ISSUED",
            },
        )
        .await;
        assert!(matches!(result, Err(R2a3Error::Network)));
        assert!(
            server.await.unwrap(),
            "HTTP became reachable before TLS rejection"
        );
    }

    #[test]
    fn signature_is_source_bound_and_verifier_has_no_private_key() {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let receipt = SignedAuthorityReceipt {
            receipt: LocalAuthorityReceipt {
                source_name: "trusted_clock".to_owned(),
                issuer: "Stage8bTrustedClockIssuer".to_owned(),
                evidence_schema: "stage8b-trusted-clock-v1".to_owned(),
                observed_at_utc: Utc::now(),
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
            source_observed_at_utc: Utc::now(),
            produced_at_utc: Utc::now(),
            issuer_key_id: "trusted_clock-ed25519-v1".to_owned(),
            signature_ed25519_hex: String::new(),
        };
        let signed = sign_authority_receipt(receipt, &signing).unwrap();
        let signature =
            Signature::from_bytes(&decode_hex::<64>(&signed.signature_ed25519_hex).unwrap());
        signing
            .verifying_key()
            .verify(&receipt_signing_preimage(&signed).unwrap(), &signature)
            .unwrap();
        let mut forged = signed;
        forged
            .receipt
            .claims
            .insert("ready".to_owned(), "true".to_owned());
        assert!(signing
            .verifying_key()
            .verify(&receipt_signing_preimage(&forged).unwrap(), &signature)
            .is_err());
    }

    #[test]
    fn cross_source_skew_is_fail_closed() {
        let now = Utc::now();
        let make = |name: &str, at: DateTime<Utc>| SignedAuthorityReceipt {
            receipt: LocalAuthorityReceipt {
                source_name: name.to_owned(),
                issuer: String::new(),
                evidence_schema: String::new(),
                observed_at_utc: at,
                key_generation_id: String::new(),
                run_identity_sha256: String::new(),
                keyed_account_binding_hmac_sha256: String::new(),
                execution_build_identity_sha256: String::new(),
                claims: BTreeMap::new(),
                authentication_tag_hmac_sha256: String::new(),
            },
            run_nonce_sha256: String::new(),
            source_snapshot_sha256: String::new(),
            source_generation: 1,
            producer_executable_sha256: "1".repeat(64),
            issuer_executable_sha256: "2".repeat(64),
            authoritative_store_sha256: "3".repeat(64),
            source_observed_at_utc: at,
            // All producers run simultaneously. Skew must still use the
            // immutable source observations rather than this fresh timestamp.
            produced_at_utc: now,
            issuer_key_id: String::new(),
            signature_ed25519_hex: String::new(),
        };
        let envelope = SignedAuthorityEnvelope {
            schema_version: 1,
            run_nonce_sha256: String::new(),
            receipts: vec![
                make("trusted_clock", now),
                make(
                    "composite_readiness",
                    now + chrono::Duration::milliseconds(1_001),
                ),
                make("schedule", now),
                make("instrument_specification", now),
            ],
        };
        assert!(validate_skew(&envelope).is_err());
    }

    #[test]
    fn token_timestamps_are_semantically_validated() {
        let now = Utc::now();
        let details = StrictTokenDetails {
            account_ids: vec!["A".to_owned()],
            created_at: Some((now - chrono::Duration::minutes(1)).to_rfc3339()),
            expires_at: Some((now + chrono::Duration::minutes(1)).to_rfc3339()),
            md_permissions: vec![],
            readonly: true,
        };
        r2a2::validate_token_details(details, "A", now).unwrap();
        let expired = StrictTokenDetails {
            account_ids: vec!["A".to_owned()],
            created_at: Some((now - chrono::Duration::minutes(2)).to_rfc3339()),
            expires_at: Some((now - chrono::Duration::minutes(1)).to_rfc3339()),
            md_permissions: vec![],
            readonly: true,
        };
        assert!(r2a2::validate_token_details(expired, "A", now).is_err());
    }

    #[test]
    fn signed_authorities_expire_and_cannot_be_replayed() {
        let now = Utc::now();
        let (manifest, envelope, keys, nonce) = controlled_fixture(now).unwrap();
        validate_signed_authorities(&manifest, &envelope, &keys, &nonce, now).unwrap();
        assert!(validate_signed_authorities(
            &manifest,
            &envelope,
            &keys,
            &nonce,
            now + chrono::Duration::seconds(3)
        )
        .is_err());
        assert!(
            validate_signed_authorities(&manifest, &envelope, &keys, &"f".repeat(64), now).is_err()
        );
    }

    #[test]
    fn owner_snapshot_is_source_typed_and_producer_separated_from_issuer() {
        let nonce = "a".repeat(64);
        let source_name = "composite_readiness";
        let snapshot = AuthoritySourceSnapshot {
            schema_version: 1,
            source_name: source_name.to_owned(),
            producer_service: format!("moex-stage8b-r2a3-source-{source_name}.service"),
            producer_uid: source_producer_uid(source_name).unwrap(),
            source_generation: 7,
            producer_executable_sha256: "a".repeat(64),
            authoritative_store_sha256: "b".repeat(64),
            run_nonce_sha256: nonce.clone(),
            source_observed_at_utc: Utc::now(),
            produced_at_utc: Utc::now(),
            claims: BTreeMap::from([("ready".to_owned(), "true".to_owned())]),
        };
        assert_ne!(
            source_producer_uid(source_name).unwrap(),
            source_issuer_uid(source_name).unwrap()
        );
        validate_owner_snapshot(&snapshot, source_name, &nonce).unwrap();
        let mut forged = snapshot;
        forged
            .claims
            .insert("caller_truth".to_owned(), "true".to_owned());
        assert!(validate_owner_snapshot(&forged, source_name, &nonce).is_err());
    }

    #[test]
    fn durable_nonce_registry_rejects_second_claim() {
        let directory = tempfile::tempdir().unwrap();
        let uid = unsafe { libc::geteuid() };
        let nonce = "b".repeat(64);
        claim_run_nonce_once_for_uid(directory.path(), &nonce, uid).unwrap();
        assert!(matches!(
            claim_run_nonce_once_for_uid(directory.path(), &nonce, uid),
            Err(R2a3Error::Authorization)
        ));
    }

    #[tokio::test]
    async fn exact_runnable_entry_completes_full_tls_sequence_with_pacing() {
        let evidence = controlled_qualification_evidence_for(Operation::Place)
            .await
            .unwrap();
        assert_eq!(evidence.request_order.len(), 5);
        let broker = evidence
            .request_order
            .iter()
            .filter(|attempt| attempt.network_class == NetworkClass::BrokerTruth)
            .collect::<Vec<_>>();
        assert_eq!(broker.len(), 3);
        for pair in broker.windows(2) {
            let elapsed = pair[1]
                .request_started_at_utc
                .signed_duration_since(pair[0].request_started_at_utc)
                .num_milliseconds();
            assert!(elapsed >= MIN_BROKER_GET_INTERVAL_MS as i64 - 5);
        }
    }

    #[tokio::test]
    async fn exact_runnable_entry_completes_full_cancel_tls_sequence() {
        let evidence = controlled_qualification_evidence_for(Operation::Cancel)
            .await
            .unwrap();
        assert_eq!(evidence.request_order.len(), 6);
        assert_eq!(evidence.operation, Operation::Cancel);
        assert_eq!(evidence.broker_truth.target_order_count, 1);
        assert_eq!(evidence.broker_truth.exact_cancel_working, Some(true));
    }

    #[tokio::test]
    async fn full_runnable_pipeline_rejects_wrong_ca_and_hostname_before_http() {
        assert_pipeline_tls_rejected("stage8b-r2a3.invalid", "stage8b-r2a3.invalid", true).await;
        assert_pipeline_tls_rejected("stage8b-r2a3.invalid", "wrong-stage8b-r2a3.invalid", false)
            .await;
    }
}
