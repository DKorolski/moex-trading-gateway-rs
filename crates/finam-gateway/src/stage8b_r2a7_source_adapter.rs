//! Stage 8B-P R2A7 fixed-root, one-shot recovery reader.
//!
//! This module has no FINAM client, credential, arm, dispatch, effect, Redis or
//! background-loop dependency.  It reconstructs the accepted durable owner,
//! derives the sole current Stage 6 request from replay and publishes only the
//! already-reviewed read-only operational records.

use crate::stage8a1_execution_capability::{
    publish_stage8b_r2a7_operational_sources_from_owner, Stage8bR2a5SourcePublicationEvidence,
    STAGE8B_R2A6_SOURCE_ADAPTER_UID,
};
use broker_core::{BrokerReadinessSnapshot, BrokerTruthSnapshot};
use chrono::{Duration, NaiveTime, TimeZone, Timelike, Utc};
use runtime_durable_service::{
    Stage7bCompositeReadinessSnapshot, Stage7bDurableRootAuthority, Stage7bPaperReadinessPhase,
    Stage7bRecoveryReadyOwner, Stage7bRestartOutcome,
};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use strategy_runtime_core::hybrid_intraday::{
    BreakoutEodMode, HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    MinRangeMode,
};
use strategy_runtime_core::{
    BrokerNeutralMarketOrderStyle, HybridIntradayProfile, HybridIntradayRuntimeConfig,
    HybridIntradayRuntimeStrategy, MeanReversionVariant, MrGatePolicy, RiskGateMode,
    Stage5gLifecycleCommitmentKey, Stage6dOperationalIdentityConfig,
};

const MANIFEST_FILE: &str = "stage8b-r2a7-reader-manifest.json";
const LIFECYCLE_KEY_FILE: &str = "stage8b-r2a7-lifecycle-key.hex";
const PRODUCTION_WORK_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a7/production";
const PRODUCTION_STAGE7B_PARENT: &str = "/var/lib/moex-trading/stage7b";
const PRODUCTION_AUTHORITY_ROOT: &str = "/var/lib/moex-trading/stage8a1-authority";
const PRODUCTION_OUTPUT_ROOT: &str = "/var/lib/moex-trading/operational-authorities";
const CONTROLLED_ROOT: &str = "/var/lib/moex-trading/stage8b/r2a7-controlled";
const RUNTIME_PROFILE_ID: &str = "imoexf-stage5ge-c-normal-append-v1";
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_KEY_BYTES: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage8bR2a7RunMode {
    Production,
    ControlledPlace,
    ControlledCancel,
}

impl Stage8bR2a7RunMode {
    fn adapter_domain(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::ControlledPlace | Self::ControlledCancel => "controlled_qualification",
        }
    }

    fn expected_file_owner(self) -> u32 {
        match self {
            Self::Production => 0,
            Self::ControlledPlace | Self::ControlledCancel => STAGE8B_R2A6_SOURCE_ADAPTER_UID,
        }
    }

    fn layout(self) -> Stage8bR2a7FixedLayout {
        match self {
            Self::Production => Stage8bR2a7FixedLayout {
                work_root: PathBuf::from(PRODUCTION_WORK_ROOT),
                stage7b_parent: PathBuf::from(PRODUCTION_STAGE7B_PARENT),
                authority_root: PathBuf::from(PRODUCTION_AUTHORITY_ROOT),
                output_root: PathBuf::from(PRODUCTION_OUTPUT_ROOT),
            },
            Self::ControlledPlace | Self::ControlledCancel => {
                let operation = if self == Self::ControlledPlace {
                    "place"
                } else {
                    "cancel"
                };
                let base = Path::new(CONTROLLED_ROOT).join(operation);
                Stage8bR2a7FixedLayout {
                    work_root: base.join("input"),
                    stage7b_parent: base.join("input/stage7b"),
                    authority_root: base.join("input/stage8a1-authority"),
                    output_root: base.join("operational-authorities"),
                }
            }
        }
    }
}

struct Stage8bR2a7FixedLayout {
    work_root: PathBuf,
    stage7b_parent: PathBuf,
    authority_root: PathBuf,
    output_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bR2a7ReaderManifestV1 {
    pub schema_version: u16,
    pub adapter_domain: String,
    pub runtime_profile_id: String,
    pub operational_identity: Stage6dOperationalIdentityConfig,
    pub accepted_config_sha256: String,
    pub composite_checked_at: chrono::DateTime<Utc>,
    pub broker_truth: BrokerTruthSnapshot,
    pub broker_readiness: BrokerReadinessSnapshot,
    pub manifest_hmac_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stage8bR2a7PublicationEvidence {
    pub schema_version: u16,
    pub adapter_domain: &'static str,
    pub source_count: usize,
    pub source_sha256: std::collections::BTreeMap<String, String>,
    pub publication_root_sha256: String,
    pub execution_authority_granted: bool,
    pub network_accessed: bool,
    pub finam_credential_accessed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bR2a7SourceAdapterError {
    #[error("R2A7 adapter must run as the fixed source-adapter UID")]
    WrongUid,
    #[error("R2A7 fixed reader input is unsafe or invalid")]
    ReaderInputInvalid,
    #[error("R2A7 runtime profile is not the accepted fixed profile")]
    RuntimeProfileInvalid,
    #[error("R2A7 Stage 7B restart did not yield one ready owner")]
    RecoveryNotReady,
    #[error("R2A7 durable request selection is not unique and current")]
    DurableRequestInvalid,
    #[error("R2A7 operational source publication failed")]
    PublicationFailed,
}

/// Runs the exact production-capable source adapter once.  `mode` selects one
/// of three compile-time layouts; no caller-supplied path or request identity
/// crosses this boundary.
pub fn run_stage8b_r2a7_source_adapter(
    mode: Stage8bR2a7RunMode,
) -> Result<Stage8bR2a7PublicationEvidence, Stage8bR2a7SourceAdapterError> {
    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    let layout = mode.layout();
    let manifest_bytes = read_fixed_regular_file(
        &layout.work_root.join(MANIFEST_FILE),
        MAX_MANIFEST_BYTES,
        mode.expected_file_owner(),
    )?;
    let manifest: Stage8bR2a7ReaderManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let key_bytes = read_fixed_regular_file(
        &layout.work_root.join(LIFECYCLE_KEY_FILE),
        MAX_KEY_BYTES,
        mode.expected_file_owner(),
    )?;
    let commitment_key = parse_lifecycle_key(&key_bytes)?;
    validate_manifest(&manifest, mode, &commitment_key)?;
    let runtime = fixed_runtime_profile(&manifest.runtime_profile_id)?;
    let root_name =
        Stage7bDurableRootAuthority::expected_directory_name(&manifest.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let durable_root = layout.stage7b_parent.join(root_name);
    let root = Stage7bDurableRootAuthority::validate(&durable_root, &manifest.operational_identity)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let restarted = Stage7bRecoveryReadyOwner::restart(
        root,
        manifest.operational_identity,
        &commitment_key,
        runtime,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::RecoveryNotReady)?;
    let Stage7bRestartOutcome::Ready(mut owner) = restarted else {
        return Err(Stage8bR2a7SourceAdapterError::RecoveryNotReady);
    };
    let (identity, command) = owner
        .recovered()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?
        .single_exact_dispatch_ready_request()
        .map_err(|_| Stage8bR2a7SourceAdapterError::DurableRequestInvalid)?;
    let readiness = Stage7bCompositeReadinessSnapshot {
        phase: Stage7bPaperReadinessPhase::PaperReady,
        reasons: Vec::new(),
        blocked_entry_ids: Vec::new(),
        blocked_request_ids: Vec::new(),
        checked_at: manifest.composite_checked_at,
    };
    let evidence = publish_stage8b_r2a7_operational_sources_from_owner(
        &mut owner,
        &commitment_key,
        &identity,
        &command,
        &layout.authority_root,
        &manifest.accepted_config_sha256,
        &readiness,
        &manifest.broker_truth,
        &manifest.broker_readiness,
        &layout.output_root,
        mode.adapter_domain(),
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
    verify_published_domain(&layout.output_root, &evidence, mode.adapter_domain())?;
    Ok(publication_evidence(mode, evidence))
}

fn verify_published_domain(
    output_root: &Path,
    evidence: &Stage8bR2a5SourcePublicationEvidence,
    expected_domain: &str,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    if !matches!(expected_domain, "production" | "controlled_qualification")
        || evidence.schema_version != 2
        || evidence.source_count != evidence.source_sha256.len()
    {
        return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
    }
    for file_name in evidence.source_sha256.keys() {
        if file_name.contains('/') || file_name.contains("..") {
            return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
        }
        let bytes = read_fixed_regular_file(
            &output_root.join(file_name),
            MAX_MANIFEST_BYTES,
            STAGE8B_R2A6_SOURCE_ADAPTER_UID,
        )?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| Stage8bR2a7SourceAdapterError::PublicationFailed)?;
        if value.get("adapter_domain").and_then(|value| value.as_str()) != Some(expected_domain)
            || value.get("adapter_mode").and_then(|value| value.as_str())
                != Some("one_shot_recovery_reader")
        {
            return Err(Stage8bR2a7SourceAdapterError::PublicationFailed);
        }
    }
    Ok(())
}

fn publication_evidence(
    mode: Stage8bR2a7RunMode,
    evidence: Stage8bR2a5SourcePublicationEvidence,
) -> Stage8bR2a7PublicationEvidence {
    Stage8bR2a7PublicationEvidence {
        schema_version: 1,
        adapter_domain: mode.adapter_domain(),
        source_count: evidence.source_count,
        source_sha256: evidence.source_sha256,
        publication_root_sha256: evidence.publication_root_sha256,
        execution_authority_granted: false,
        network_accessed: false,
        finam_credential_accessed: false,
    }
}

fn validate_manifest(
    manifest: &Stage8bR2a7ReaderManifestV1,
    mode: Stage8bR2a7RunMode,
    commitment_key: &Stage5gLifecycleCommitmentKey,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    if manifest.schema_version != 1
        || manifest.adapter_domain != mode.adapter_domain()
        || manifest.runtime_profile_id != RUNTIME_PROFILE_ID
        || !valid_sha256(&manifest.accepted_config_sha256)
        || manifest.broker_truth.received_ts > Utc::now() + Duration::seconds(1)
        || manifest.composite_checked_at > Utc::now() + Duration::seconds(1)
        || !commitment_key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(
            &reader_manifest_commitment_sha256(manifest)?,
            &manifest.manifest_hmac_sha256,
        )
    {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    Ok(())
}

fn reader_manifest_commitment_sha256(
    manifest: &Stage8bR2a7ReaderManifestV1,
) -> Result<String, Stage8bR2a7SourceAdapterError> {
    use sha2::{Digest, Sha256};
    let mut value = serde_json::to_value(manifest)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    value
        .as_object_mut()
        .ok_or(Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?
        .remove("manifest_hmac_sha256");
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let mut hasher = Sha256::new();
    hasher.update(b"stage8b-r2a7-reader-manifest-commitment-v1");
    hasher.update(b"\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_fixed_regular_file(
    path: &Path,
    max_bytes: u64,
    expected_owner: u32,
) -> Result<Vec<u8>, Stage8bR2a7SourceAdapterError> {
    if !path.is_absolute() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let metadata = file
        .metadata()
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if !metadata.file_type().is_file()
        || metadata.uid() != expected_owner
        || metadata.nlink() != 1
        || metadata.len() > max_bytes
        || metadata.mode() & 0o022 != 0
    {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    Ok(bytes)
}

fn parse_lifecycle_key(
    bytes: &[u8],
) -> Result<Stage5gLifecycleCommitmentKey, Stage8bR2a7SourceAdapterError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?
        .trim();
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let mut decoded = [0_u8; 32];
    for (index, slot) in decoded.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    }
    Stage5gLifecycleCommitmentKey::from_secret_bytes(&decoded)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)
}

fn fixed_runtime_profile(
    profile: &str,
) -> Result<HybridIntradayRuntimeStrategy, Stage8bR2a7SourceAdapterError> {
    if profile != RUNTIME_PROFILE_ID {
        return Err(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid);
    }
    let bar_close = Utc
        .with_ymd_and_hms(2026, 8, 3, 12, 0, 0)
        .single()
        .ok_or(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?;
    let timezone_offset_hours = 9 - i32::try_from(bar_close.hour())
        .map_err(|_| Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?;
    let local_bar_close = bar_close + Duration::hours(i64::from(timezone_offset_hours));
    Ok(HybridIntradayRuntimeStrategy::new(
        HybridIntradayRuntimeConfig {
            symbol: "IMOEXF".to_owned(),
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
            risk_gate_mode: RiskGateMode::NormalAppend,
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: Some((local_bar_close - Duration::minutes(10)).time()),
            model_session_end_time: Some((local_bar_close + Duration::hours(1)).time()),
            qty: 1.0,
            live_order_style: BrokerNeutralMarketOrderStyle::Market,
            tick_size: 0.5,
            marketable_limit_offset_ticks: 0,
            timezone_offset_hours,
            session_close_hour: 23,
            session_close_minute: 49,
            weekends_off: false,
            stop_end_buffer_sec: 60,
            repair_deadline_sec: 180,
            sl_escalate_timeout_sec: 30,
            max_repair_retries: 3,
            repair_backoff_base_sec: 5,
            repair_backoff_max_sec: 60,
            pending_timeout_sec: 30,
            partial_entry_fill_timeout_ms: 3_000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig {
                k: 0.53,
                stop1_range: 0.51,
                stop2_range: 0.35,
                big_move_threshold: 0.025,
                min_range: 1.01,
                min_range_mode: MinRangeMode::Absolute,
                exclude_weekends: false,
                wait_hours: 0.0,
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: BreakoutEodMode::SameDay,
                breakout_overnight_exit_time: NaiveTime::from_hms_opt(9, 30, 0)
                    .ok_or(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid)?,
            },
        },
    ))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Qualification-only setup.  This executable is built separately with test
/// fixtures; the accepted source-adapter executable is built without that
/// feature and only reads the resulting fixed durable layout.
#[cfg(feature = "stage8b-r2a7-controlled-qualification")]
#[doc(hidden)]
pub fn seed_stage8b_r2a7_controlled_reader(
    mode: Stage8bR2a7RunMode,
) -> Result<(), Stage8bR2a7SourceAdapterError> {
    use crate::{
        Stage8KillSwitchState, Stage8a1AcceptedExecutionConfigV1, Stage8a1CurrentControlStateV1,
    };
    use broker_core::{
        BrokerFeedFreshness, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState,
        BrokerOrderSnapshot, BrokerStopOrderReadiness, BrokerSymbol, Exchange, InstrumentMapEntry,
        InternalSymbol, Market, OperatorArm, OrderPreflightPolicy, OrderSide, OrderStatus,
        OrderType, TimeInForce,
    };
    use runtime_durable_service::{
        stage8a4_i3_production_test_setup_in, stage8b_r2a6_cancel_production_test_setup_in,
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    if unsafe { libc::geteuid() } != STAGE8B_R2A6_SOURCE_ADAPTER_UID {
        return Err(Stage8bR2a7SourceAdapterError::WrongUid);
    }
    if !matches!(
        mode,
        Stage8bR2a7RunMode::ControlledPlace | Stage8bR2a7RunMode::ControlledCancel
    ) {
        return Err(Stage8bR2a7SourceAdapterError::ReaderInputInvalid);
    }
    let layout = mode.layout();
    fs::create_dir_all(&layout.stage7b_parent)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let (setup, owner) = match mode {
        Stage8bR2a7RunMode::ControlledPlace => {
            stage8a4_i3_production_test_setup_in(layout.stage7b_parent.clone())
        }
        Stage8bR2a7RunMode::ControlledCancel => {
            stage8b_r2a6_cancel_production_test_setup_in(layout.stage7b_parent.clone())
        }
        Stage8bR2a7RunMode::Production => unreachable!("checked above"),
    };
    drop(owner);

    fs::create_dir(&layout.authority_root)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::set_permissions(&layout.authority_root, fs::Permissions::from_mode(0o700))
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let fixed_runtime = fixed_runtime_profile(RUNTIME_PROFILE_ID)?;
    if fixed_runtime.stage5c_config_fingerprint() != setup.runtime.stage5c_config_fingerprint() {
        return Err(Stage8bR2a7SourceAdapterError::RuntimeProfileInvalid);
    }
    let account_id = match &setup.command {
        broker_core::BrokerCommand::PlaceOrder(place) => place.account_id.clone(),
        broker_core::BrokerCommand::CancelOrder(cancel) => cancel.account_id.clone(),
    };
    let instrument = setup.command_context.instrument().clone();
    let venue_symbol = instrument
        .venue_symbol
        .clone()
        .ok_or(Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let now = Utc::now();
    let runtime_fingerprint = setup.runtime.stage5c_config_fingerprint();
    let operational_identity_sha256 =
        strategy_runtime_core::stage6d_operational_identity_sha256(&setup.operational_identity)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?
            .as_str()
            .to_owned();
    let config = Stage8a1AcceptedExecutionConfigV1 {
        schema_version: 1,
        operational_identity_sha256: operational_identity_sha256.clone(),
        runtime_config_fingerprint_sha256: runtime_fingerprint.clone(),
        broker: BrokerKind::Finam,
        strategy_instance_id: setup.command_context.attribution().strategy_id().to_owned(),
        account_id: account_id.clone(),
        instrument: instrument.clone(),
        broker_policy: OrderPreflightPolicy {
            allowed_accounts: vec![account_id.clone()],
            allowed_venue_symbols: vec![venue_symbol.clone()],
            allowed_order_types: vec![OrderType::Market, OrderType::Limit],
            allowed_time_in_force: vec![TimeInForce::Day],
            min_qty: rust_decimal::Decimal::ONE,
            qty_step: rust_decimal::Decimal::ONE,
            max_qty: rust_decimal::Decimal::new(2, 0),
            price_step: Some(rust_decimal::Decimal::new(5, 1)),
            max_market_qty: rust_decimal::Decimal::ONE,
            max_notional_per_order: Some(rust_decimal::Decimal::new(10_000, 0)),
            max_notional_per_run: Some(rust_decimal::Decimal::new(10_000, 0)),
            max_limit_deviation_bps: Some(1_000),
            max_reference_age_ms: 5_000,
            allow_cancel_by_broker_order_id_without_mapping: false,
            operator_arm: OperatorArm {
                session_id: "STAGE8B_R2A7_CONTROLLED_NO_EFFECT".to_owned(),
                armed_until: now + Duration::seconds(30),
                endpoint_calls_enabled: true,
                one_shot: true,
                endpoint_attempted: false,
                preflight_digest: runtime_fingerprint.clone(),
            },
        },
        build_sha256: "b".repeat(64),
        endpoint_policy_sha256: "e".repeat(64),
        max_arm_ttl_ms: 20_000,
        max_evidence_age_ms: 20_000,
        stage8a4_writer_issuer_public_key_hex: setup
            .operational_identity
            .stage8a4_writer_issuer_public_key_hex
            .clone(),
    };
    let config_bytes = serde_json::to_vec(&config)
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let config_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(b"stage8a1-accepted-config-file-v1");
        hasher.update(b"\0");
        hasher.update(&config_bytes);
        format!("{:x}", hasher.finalize())
    };
    fs::write(
        layout
            .authority_root
            .join("stage8a1-accepted-execution-config.json"),
        config_bytes,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::write(
        layout
            .authority_root
            .join("stage8a1-accepted-execution-config.json.sha256"),
        &config_sha256,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let observed = Utc::now();
    let control = Stage8a1CurrentControlStateV1 {
        schema_version: 1,
        operational_identity_sha256,
        runtime_config_fingerprint_sha256: runtime_fingerprint,
        kill_switch: Stage8KillSwitchState::RunAllowed,
        durable_revision: 1,
        active_owner_count: 1,
        reconciliation_required_count: 0,
        max_orders: 1,
        consumed_orders: 0,
        observed_at: observed,
        valid_until: observed + Duration::seconds(20),
    };
    fs::write(
        layout
            .authority_root
            .join("stage8a1-current-control-state.json"),
        serde_json::to_vec(&control)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    let signing_key_path = layout
        .authority_root
        .join("stage8a4-writer-issuer-signing-key.hex");
    fs::write(
        &signing_key_path,
        b"9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::set_permissions(&signing_key_path, fs::Permissions::from_mode(0o600))
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;

    let orders = match &setup.command {
        broker_core::BrokerCommand::PlaceOrder(_) => Vec::new(),
        broker_core::BrokerCommand::CancelOrder(cancel) => vec![BrokerOrderSnapshot {
            account_id: account_id.clone(),
            broker_order_id: Some(cancel.order_id.clone()),
            client_order_id: cancel.client_order_id.clone(),
            instrument: instrument.clone(),
            side: OrderSide::Buy,
            order_type: OrderType::Limit,
            time_in_force: Some(TimeInForce::Day),
            status: OrderStatus::New,
            lifecycle: BrokerOrderSnapshot::lifecycle_for(&OrderStatus::New),
            qty: rust_decimal::Decimal::ONE,
            filled_qty: rust_decimal::Decimal::ZERO,
            remaining_qty: Some(rust_decimal::Decimal::ONE),
            limit_price: Some(rust_decimal::Decimal::new(2_210, 0)),
            broker_asset_id: Some("ASSET_IMOEXF".to_owned()),
            board: Some("RFUD".to_owned()),
            expiration_date: None,
            source_ts: Some(observed),
            received_ts: observed,
        }],
    };
    let truth = BrokerTruthSnapshot {
        account_id,
        orders,
        positions: Vec::new(),
        cash: None,
        trades: Vec::new(),
        instruments: vec![BrokerInstrumentSpec {
            instrument: InstrumentMapEntry {
                internal_symbol: InternalSymbol(instrument.symbol.clone()),
                broker: BrokerKind::Finam,
                broker_symbol: BrokerSymbol(venue_symbol),
                exchange: Exchange::Moex,
                market: Market::Futures,
                price_step: rust_decimal::Decimal::new(5, 1),
                qty_step: rust_decimal::Decimal::ONE,
                lot_size: rust_decimal::Decimal::ONE,
                min_qty: rust_decimal::Decimal::ONE,
                step_value: rust_decimal::Decimal::ONE,
                currency: "RUB".to_owned(),
                schedule_id: "MOEX_FUT".to_owned(),
                expiration_date: None,
                is_tradable: true,
            },
            broker_asset_id: Some("ASSET_IMOEXF".to_owned()),
            board: Some("RFUD".to_owned()),
            long_initial_margin: None,
            short_initial_margin: None,
        }],
        received_ts: observed,
    };
    let freshness = || BrokerFeedFreshness {
        observed_ts: Some(observed),
        max_age_ms: 30_000,
    };
    let broker_readiness = BrokerReadinessSnapshot {
        account: freshness(),
        positions: freshness(),
        orders: freshness(),
        trades: freshness(),
        quotes: freshness(),
        instrument_spec: freshness(),
        schedule: freshness(),
        market_session: BrokerMarketSessionState::Open,
        unknown_order_count: 0,
        cash_margin_present: true,
        instrument_spec_validated: true,
        live_market_data_seen: true,
        subscription_ready: true,
        stream_or_polling_connected: true,
        event_sink_degraded: false,
        stop_order_readiness: BrokerStopOrderReadiness::UnsupportedBlocked,
    };
    let mut manifest = Stage8bR2a7ReaderManifestV1 {
        schema_version: 1,
        adapter_domain: mode.adapter_domain().to_owned(),
        runtime_profile_id: RUNTIME_PROFILE_ID.to_owned(),
        operational_identity: setup.operational_identity,
        accepted_config_sha256: config_sha256,
        composite_checked_at: observed,
        broker_truth: truth,
        broker_readiness,
        manifest_hmac_sha256: String::new(),
    };
    manifest.manifest_hmac_sha256 = setup
        .commitment_key
        .stage8b_r2a7_reader_manifest_hmac_sha256(&reader_manifest_commitment_sha256(&manifest)?);
    fs::write(
        layout.work_root.join(MANIFEST_FILE),
        serde_json::to_vec(&manifest)
            .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?,
    )
    .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    fs::write(layout.work_root.join(LIFECYCLE_KEY_FILE), "5a".repeat(32))
        .map_err(|_| Stage8bR2a7SourceAdapterError::ReaderInputInvalid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_profile_is_stable_and_modes_have_disjoint_roots() {
        let runtime = fixed_runtime_profile(RUNTIME_PROFILE_ID).expect("fixed profile");
        assert_eq!(runtime.stage5c_config_fingerprint().len(), 64);
        let production = Stage8bR2a7RunMode::Production.layout();
        let controlled = Stage8bR2a7RunMode::ControlledPlace.layout();
        assert_ne!(production.output_root, controlled.output_root);
        assert_eq!(
            Stage8bR2a7RunMode::ControlledCancel.adapter_domain(),
            "controlled_qualification"
        );
    }

    #[test]
    fn manifest_hmac_is_not_rebindable_to_another_domain_commitment() {
        let key = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap();
        let controlled_commitment = "a".repeat(64);
        let production_commitment = "b".repeat(64);
        let hmac = key.stage8b_r2a7_reader_manifest_hmac_sha256(&controlled_commitment);
        assert!(key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(&controlled_commitment, &hmac));
        assert!(!key.stage8b_r2a7_verify_reader_manifest_hmac_sha256(&production_commitment, &hmac));
    }
}
