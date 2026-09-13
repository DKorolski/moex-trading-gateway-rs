//! Stage 8B-P1-e production first-boot source boundary.
//!
//! The fixed-path loader performs F00 and turns source observations into one
//! linear validated capability.  The production builder then performs the
//! deterministic F01-F16 composition.  Durable-root creation remains solely
//! owned by `first_boot_stage8b_p1` (F17); Redis and FINAM are unreachable.

use std::{
    collections::BTreeSet,
    ffi::CString,
    fmt,
    fs::{Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use rust_decimal::Decimal;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    stage8b_p1_imoexf_instrument_map_fingerprint_sha256, Stage8bP1RuntimeProfileV1,
    Stage8bP1ValidatedBootstrapConfig, Stage8bP1eSupervisorConfigError,
    Stage8bP1eValidatedSupervisorConfigV1, STAGE8B_P1E_FIRST_BOOT_SOURCE_PATH,
    STAGE8B_P1E_RUNTIME_PROFILE_SHA256, STAGE8B_P1_VENUE_SYMBOL,
};

pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION: u16 = 2;
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_DOMAIN: &str =
    "moex.stage8b.p1e.first-boot-source-bundle.v2";
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES: u64 = 16 * 1024 * 1024;
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_GROUP: &str = "moex-p1-paper";
pub const STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS: usize = 121;
pub const STAGE8B_P1E_FIRST_BOOT_MIN_RISKGATE_SESSIONS: usize = 120;
pub const STAGE8B_P1E_FIRST_BOOT_TRUTH_MAX_AGE_SECONDS: i64 = 300;
pub const STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256: &str =
    "2a507577075b8b5315a462ffeee221dd0a7f8a8f61d42516fbdb9346cc3464ca";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eFirstBootSourceError {
    #[error("Stage 8B-P1-e first-boot source file boundary is invalid")]
    InvalidFileBoundary,
    #[error("Stage 8B-P1-e first-boot source exceeds the bounded size")]
    SourceTooLarge,
    #[error("Stage 8B-P1-e first-boot source cannot be read completely")]
    SourceReadFailed,
    #[error("Stage 8B-P1-e first-boot source SHA-256 does not match config")]
    SourceHashMismatch,
    #[error("Stage 8B-P1-e first-boot source JSON is invalid")]
    InvalidJson,
    #[error("Stage 8B-P1-e first-boot source JSON contains a duplicate key")]
    DuplicateJsonKey,
    #[error("Stage 8B-P1-e first-boot source schema is invalid")]
    InvalidSchema,
    #[error("Stage 8B-P1-e first-boot source deployment binding is invalid")]
    IdentityMismatch,
    #[error("Stage 8B-P1-e first-boot broker truth is invalid or stale")]
    InvalidBrokerTruth,
    #[error("Stage 8B-P1-e first-boot History M10 observations are invalid")]
    InvalidHistory,
    #[error("Stage 8B-P1-e first-boot riskgate observations are invalid")]
    InvalidRiskGateHistory,
    #[error("Stage 8B-P1-e first-boot replay candidate is invalid")]
    InvalidCandidate,
}

/// Authenticated source observations. The type is intentionally linear and
/// non-serializable; later F01-F16 composition must consume it by value.
pub struct Stage8bP1eValidatedFirstBootSourceV1 {
    source_bundle_sha256: String,
    source_bundle_generation: u64,
    captured_at: DateTime<Utc>,
    broker_truth_checked_at: DateTime<Utc>,
    account_id: String,
    history_bars_sha256: String,
    riskgate_session_observations_sha256: String,
    candidate_semantic_id_sha256: String,
    candidate_canonical_m10_sha256: String,
    history_bars: Vec<Stage8bP1eFirstBootBarV1>,
    riskgate_observations: Vec<Stage8bP1eRiskGateObservationV1>,
    candidate: Stage8bP1eFirstBootBarV1,
}

/// Complete no-effect first-boot material ready for the existing durable-root
/// transaction.  It is produced only after the fixed-path F00 source and the
/// deterministic F02-F16 composition both succeed.
pub struct Stage8bP1ePreparedFirstBootV1 {
    bootstrap: Stage8bP1ValidatedBootstrapConfig,
    source: strategy_runtime_core::Stage5gTimerReadyPaperStrategy,
    export_input: strategy_runtime_core::Stage5gCleanRestartExportInput,
    fresh_runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    provenance: strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
}

impl Stage8bP1ePreparedFirstBootV1 {
    pub fn into_parts(
        self,
    ) -> (
        Stage8bP1ValidatedBootstrapConfig,
        strategy_runtime_core::Stage5gTimerReadyPaperStrategy,
        strategy_runtime_core::Stage5gCleanRestartExportInput,
        strategy_runtime_core::HybridIntradayRuntimeStrategy,
        strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1,
    ) {
        (
            self.bootstrap,
            self.source,
            self.export_input,
            self.fresh_runtime,
            self.provenance,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eFirstBootBuildError {
    #[error("Stage 8B-P1-e first-boot source authentication failed")]
    Source,
    #[error("Stage 8B-P1-e first-boot runtime profile failed")]
    RuntimeProfile,
    #[error("Stage 8B-P1-e first-boot deterministic composition failed")]
    Composition,
}

impl From<Stage8bP1eFirstBootSourceError> for Stage8bP1eFirstBootBuildError {
    fn from(_: Stage8bP1eFirstBootSourceError) -> Self {
        Self::Source
    }
}

impl From<Stage8bP1eSupervisorConfigError> for Stage8bP1eFirstBootBuildError {
    fn from(_: Stage8bP1eSupervisorConfigError) -> Self {
        Self::RuntimeProfile
    }
}

/// Executes the production F00-F16 first-boot preparation.  The source path is
/// fixed and no Redis, FINAM, durable root or strategy effect is reachable.
pub fn build_stage8b_p1_first_boot_source_v1(
    supervisor: Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1ePreparedFirstBootV1, Stage8bP1eFirstBootBuildError> {
    let source = load_stage8b_p1e_first_boot_source_v1(&supervisor, trusted_now)?;
    let operational_identity_sha256 = supervisor
        .bootstrap()
        .operational_identity_sha256()
        .to_string();
    let (bootstrap, runtime) = supervisor.into_first_boot_parts();
    let (fresh_runtime, fresh_fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime()?;
    if fresh_fingerprint != bootstrap.runtime_config_fingerprint_sha256() {
        return Err(Stage8bP1eFirstBootBuildError::RuntimeProfile);
    }
    let provenance = strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1::new(
        operational_identity_sha256.clone(),
        STAGE8B_P1E_RUNTIME_PROFILE_SHA256.to_string(),
        bootstrap.runtime_config_fingerprint_sha256().to_string(),
        source.source_bundle_sha256.clone(),
        source.source_bundle_generation,
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256.to_string(),
        source.history_bars_sha256.clone(),
        source.riskgate_session_observations_sha256.clone(),
        source.candidate_semantic_id_sha256.clone(),
    )
    .map_err(|_| Stage8bP1eFirstBootBuildError::Composition)?;
    let history_bars = source.history_bars.iter().map(core_bar_input).collect();
    let riskgate_observations = source
        .riskgate_observations
        .iter()
        .map(
            |observation| strategy_runtime_core::Stage8bP1eRiskGateObservationInputV1 {
                session_date: observation.session_date,
                shadow_pnl_points: observation.shadow_pnl_points_text.clone(),
                shadow_trade_count: observation.shadow_trade_count,
            },
        )
        .collect();
    let composition = strategy_runtime_core::build_stage8b_p1_first_boot_composition_v1(
        strategy_runtime_core::Stage8bP1eFirstBootCompositionInputV1 {
            runtime,
            fresh_runtime,
            account_id: bootstrap.account_id().clone(),
            operational_identity_sha256,
            captured_at: source.captured_at,
            broker_truth_checked_at: source.broker_truth_checked_at,
            history_bars_sha256: source.history_bars_sha256,
            riskgate_session_observations_sha256: source.riskgate_session_observations_sha256,
            validated_candidate_semantic_id_sha256: source.candidate_semantic_id_sha256,
            history_bars,
            riskgate_observations,
            candidate: core_bar_input(&source.candidate),
        },
    )
    .map_err(|_| Stage8bP1eFirstBootBuildError::Composition)?;
    let (source, export_input, fresh_runtime) = composition.into_parts();
    Ok(Stage8bP1ePreparedFirstBootV1 {
        bootstrap,
        source,
        export_input,
        fresh_runtime,
        provenance,
    })
}

fn core_bar_input(
    bar: &Stage8bP1eFirstBootBarV1,
) -> strategy_runtime_core::Stage8bP1eFirstBootBarInputV1 {
    strategy_runtime_core::Stage8bP1eFirstBootBarInputV1 {
        close_time_utc: bar.close_time_utc,
        open: bar.open_text.clone(),
        high: bar.high_text.clone(),
        low: bar.low_text.clone(),
        close: bar.close_text.clone(),
        volume: bar.volume_text.clone(),
    }
}

impl Stage8bP1eValidatedFirstBootSourceV1 {
    pub fn source_bundle_sha256(&self) -> &str {
        &self.source_bundle_sha256
    }

    pub const fn source_bundle_generation(&self) -> u64 {
        self.source_bundle_generation
    }

    pub const fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    pub const fn broker_truth_checked_at(&self) -> DateTime<Utc> {
        self.broker_truth_checked_at
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn history_bars_sha256(&self) -> &str {
        &self.history_bars_sha256
    }

    pub fn riskgate_session_observations_sha256(&self) -> &str {
        &self.riskgate_session_observations_sha256
    }

    pub fn candidate_semantic_id_sha256(&self) -> &str {
        &self.candidate_semantic_id_sha256
    }

    pub fn candidate_canonical_m10_sha256(&self) -> &str {
        &self.candidate_canonical_m10_sha256
    }

    pub fn history_bars(&self) -> &[Stage8bP1eFirstBootBarV1] {
        &self.history_bars
    }

    pub fn riskgate_observations(&self) -> &[Stage8bP1eRiskGateObservationV1] {
        &self.riskgate_observations
    }

    pub fn candidate(&self) -> &Stage8bP1eFirstBootBarV1 {
        &self.candidate
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eFirstBootBarV1 {
    pub close_time_utc: i64,
    pub open_text: String,
    pub high_text: String,
    pub low_text: String,
    pub close_text: String,
    pub volume_text: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eRiskGateObservationV1 {
    pub session_date: NaiveDate,
    pub shadow_pnl_points_text: String,
    pub shadow_pnl_points: Decimal,
    pub shadow_trade_count: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootSourceDocumentV1 {
    schema_version: u16,
    domain: String,
    operational_identity_sha256: String,
    runtime_profile_sha256: String,
    instrument_map_fingerprint_sha256: String,
    source_bundle_generation: u64,
    captured_at_utc: String,
    broker_truth: FirstBootBrokerTruthV1,
    history_provenance: FirstBootHistoryProvenanceV1,
    history_coverage: FirstBootHistoryCoverageV2,
    history_bars: Vec<FirstBootHistoryBarV1>,
    riskgate_history: FirstBootRiskGateHistoryV1,
    candidate: FirstBootCandidateBarV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootBrokerTruthV1 {
    checked_at_utc: String,
    account_id: String,
    instrument: String,
    target_position_qty: String,
    target_positions_complete: bool,
    target_active_orders_count: u64,
    account_active_orders_count: u64,
    active_orders_complete: bool,
    instrument_price_step: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootHistoryProvenanceV1 {
    source_mode: String,
    source_timeframe_sec: u64,
    target_timeframe_sec: u64,
    aggregation_complete: bool,
    gap_absence_proven: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootHistoryCoverageV2 {
    source_mode: String,
    sessions_sha256: String,
    sessions: Vec<FirstBootHistorySessionV2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootHistorySessionV2 {
    session_date: String,
    windows: Vec<FirstBootHistoryWindowV2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootHistoryWindowV2 {
    first_close_time_utc: i64,
    last_close_time_utc: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootHistoryBarV1 {
    instrument: String,
    timeframe_sec: u64,
    close_time_utc: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    is_final: bool,
    origin: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootRiskGateHistoryV1 {
    source_mode: String,
    state_generation: String,
    history_bars_sha256: String,
    session_observations_sha256: String,
    session_observations: Vec<FirstBootRiskGateObservationV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootRiskGateObservationV1 {
    session_date: String,
    shadow_pnl_points: String,
    shadow_trade_count: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstBootCandidateBarV1 {
    instrument: String,
    timeframe_sec: u64,
    close_time_utc: i64,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
    is_final: bool,
    origin: String,
    redis_id: String,
    semantic_id_sha256: String,
    payload_sha256: String,
    open_ts_utc_ms: i64,
    close_ts_utc_ms: i64,
    source_m1: Vec<crate::Stage8bP1CanonicalM10SourceM1>,
}

/// Load F00 from the sole accepted fixed path. The file must be root-owned,
/// single-link, non-symlink and no more permissive than `0640` with the exact
/// service group. No path override is accepted.
pub fn load_stage8b_p1e_first_boot_source_v1(
    supervisor: &Stage8bP1eValidatedSupervisorConfigV1,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
    let path = Path::new(STAGE8B_P1E_FIRST_BOOT_SOURCE_PATH);
    let expected_gid = service_group_gid()?;
    let bytes = read_protected_first_boot_source(path, 0, expected_gid, || {})?;
    parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        supervisor.first_boot_source_bundle_sha256(),
        supervisor.bootstrap().operational_identity_sha256(),
        supervisor.bootstrap().account_id().as_str(),
        trusted_now,
    )
}

fn read_protected_first_boot_source<F>(
    path: &Path,
    expected_uid: u32,
    expected_gid: u32,
    before_read: F,
) -> Result<Vec<u8>, Stage8bP1eFirstBootSourceError>
where
    F: FnOnce(),
{
    let before = path
        .symlink_metadata()
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidFileBoundary)?;
    validate_source_metadata(&before, expected_uid, expected_gid)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidFileBoundary)?;
    let metadata = file
        .metadata()
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidFileBoundary)?;
    validate_source_metadata(&metadata, expected_uid, expected_gid)?;
    if !same_source_metadata(&before, &metadata) {
        return Err(Stage8bP1eFirstBootSourceError::InvalidFileBoundary);
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| Stage8bP1eFirstBootSourceError::SourceTooLarge)?;
    before_read();
    let mut bytes = Vec::with_capacity(capacity);
    (&mut file)
        .take(STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Stage8bP1eFirstBootSourceError::SourceReadFailed)?;
    let after = file
        .metadata()
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidFileBoundary)?;
    if bytes.len() as u64 > STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES {
        return Err(Stage8bP1eFirstBootSourceError::SourceTooLarge);
    }
    if bytes.len() as u64 != metadata.len() || !same_source_metadata(&metadata, &after) {
        return Err(Stage8bP1eFirstBootSourceError::SourceReadFailed);
    }
    Ok(bytes)
}

fn validate_source_metadata(
    metadata: &Metadata,
    expected_uid: u32,
    expected_gid: u32,
) -> Result<(), Stage8bP1eFirstBootSourceError> {
    let mode = metadata.permissions().mode() & 0o777;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.file_type().is_fifo()
        || metadata.nlink() != 1
        || metadata.uid() != expected_uid
        || metadata.gid() != expected_gid
        || mode & !0o640 != 0
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidFileBoundary);
    }
    if metadata.len() > STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES {
        return Err(Stage8bP1eFirstBootSourceError::SourceTooLarge);
    }
    Ok(())
}

fn same_source_metadata(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev()
        && left.ino() == right.ino()
        && left.len() == right.len()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
        && left.permissions().mode() & 0o777 == right.permissions().mode() & 0o777
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
}

/// Validate already-read bytes. This remains crate-private so the future
/// `validate-config` process mode and deterministic tests can prove the same
/// parser without gaining file-system or lifecycle authority.
pub(crate) fn parse_stage8b_p1e_first_boot_source_v1(
    bytes: &[u8],
    expected_source_bundle_sha256: &str,
    expected_operational_identity_sha256: &str,
    expected_account_id: &str,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
    if bytes.len() as u64 > STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES {
        return Err(Stage8bP1eFirstBootSourceError::SourceTooLarge);
    }
    let actual_source_bundle_sha256 = sha256_hex(bytes);
    if !is_sha256_hex(expected_source_bundle_sha256)
        || actual_source_bundle_sha256 != expected_source_bundle_sha256
    {
        return Err(Stage8bP1eFirstBootSourceError::SourceHashMismatch);
    }

    let value = parse_json_without_duplicates(bytes)?;
    let history_hash = canonical_member_sha256(&value, "history_bars")?;
    let observations_hash = value
        .get("riskgate_history")
        .and_then(Value::as_object)
        .and_then(|riskgate| riskgate.get("session_observations"))
        .map(canonical_value_sha256)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidSchema)?;
    let coverage_hash = value
        .get("history_coverage")
        .and_then(Value::as_object)
        .and_then(|coverage| coverage.get("sessions"))
        .map(canonical_value_sha256)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidSchema)?;
    let document: FirstBootSourceDocumentV1 =
        serde_json::from_value(value).map_err(|_| Stage8bP1eFirstBootSourceError::InvalidSchema)?;

    if document.schema_version != STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION
        || document.domain != STAGE8B_P1E_FIRST_BOOT_SOURCE_DOMAIN
        || document.source_bundle_generation == 0
        || document.operational_identity_sha256 != expected_operational_identity_sha256
        || document.runtime_profile_sha256 != STAGE8B_P1E_RUNTIME_PROFILE_SHA256
        || document.instrument_map_fingerprint_sha256
            != stage8b_p1_imoexf_instrument_map_fingerprint_sha256()
        || document.broker_truth.account_id != expected_account_id
    {
        return Err(Stage8bP1eFirstBootSourceError::IdentityMismatch);
    }

    let captured_at = parse_canonical_timestamp(&document.captured_at_utc)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidSchema)?;
    let broker_truth_checked_at = parse_canonical_timestamp(&document.broker_truth.checked_at_utc)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidBrokerTruth)?;
    if captured_at > trusted_now
        || broker_truth_checked_at > captured_at
        || trusted_now.signed_duration_since(broker_truth_checked_at)
            > Duration::seconds(STAGE8B_P1E_FIRST_BOOT_TRUTH_MAX_AGE_SECONDS)
        || document.broker_truth.instrument != STAGE8B_P1_VENUE_SYMBOL
        || document.broker_truth.target_position_qty != "0"
        || !document.broker_truth.target_positions_complete
        || document.broker_truth.target_active_orders_count != 0
        || document.broker_truth.account_active_orders_count != 0
        || !document.broker_truth.active_orders_complete
        || document.broker_truth.instrument_price_step != "0.5"
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidBrokerTruth);
    }

    if document.history_provenance.source_mode != "finam_derived_m1_to_m10"
        || document.history_provenance.source_timeframe_sec != 60
        || document.history_provenance.target_timeframe_sec != 600
        || !document.history_provenance.aggregation_complete
        || !document.history_provenance.gap_absence_proven
        || document.history_bars.is_empty()
        || history_hash != document.riskgate_history.history_bars_sha256
        || document.history_coverage.source_mode != "config-bound-explicit-session-windows-v1"
        || !is_sha256_hex(&document.history_coverage.sessions_sha256)
        || coverage_hash != document.history_coverage.sessions_sha256
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidHistory);
    }

    let mut history_bars = Vec::with_capacity(document.history_bars.len());
    let mut history_sessions = BTreeSet::new();
    for raw in &document.history_bars {
        let bar = validate_bar(
            &raw.instrument,
            raw.timeframe_sec,
            raw.close_time_utc,
            &raw.open,
            &raw.high,
            &raw.low,
            &raw.close,
            &raw.volume,
            raw.is_final,
            &raw.origin,
            "history",
        )
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidHistory)?;
        if history_bars
            .last()
            .is_some_and(|prior: &Stage8bP1eFirstBootBarV1| {
                prior.close_time_utc >= bar.close_time_utc
            })
        {
            return Err(Stage8bP1eFirstBootSourceError::InvalidHistory);
        }
        history_sessions.insert(
            moscow_session_date(bar.close_time_utc)
                .ok_or(Stage8bP1eFirstBootSourceError::InvalidHistory)?,
        );
        history_bars.push(bar);
    }
    if history_sessions.len() < STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS
        || !history_coverage_is_exact(
            &document.history_coverage.sessions,
            &history_bars,
            &history_sessions,
        )
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidHistory);
    }

    if document.riskgate_history.source_mode != "source-compatible-high180-shadow-history-v1"
        || document.riskgate_history.state_generation != "runtime-ledger-v1"
        || observations_hash != document.riskgate_history.session_observations_sha256
        || document.riskgate_history.session_observations.len()
            < STAGE8B_P1E_FIRST_BOOT_MIN_RISKGATE_SESSIONS
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidRiskGateHistory);
    }
    let mut riskgate_observations =
        Vec::with_capacity(document.riskgate_history.session_observations.len());
    for raw in document.riskgate_history.session_observations {
        let session_date = NaiveDate::parse_from_str(&raw.session_date, "%Y-%m-%d")
            .ok()
            .filter(|date| date.format("%Y-%m-%d").to_string() == raw.session_date)
            .ok_or(Stage8bP1eFirstBootSourceError::InvalidRiskGateHistory)?;
        let shadow_pnl_points = parse_signed_decimal(&raw.shadow_pnl_points)
            .ok_or(Stage8bP1eFirstBootSourceError::InvalidRiskGateHistory)?;
        if riskgate_observations
            .last()
            .is_some_and(|prior: &Stage8bP1eRiskGateObservationV1| {
                prior.session_date >= session_date
            })
            || !history_sessions.contains(&session_date)
        {
            return Err(Stage8bP1eFirstBootSourceError::InvalidRiskGateHistory);
        }
        riskgate_observations.push(Stage8bP1eRiskGateObservationV1 {
            session_date,
            shadow_pnl_points_text: raw.shadow_pnl_points,
            shadow_pnl_points,
            shadow_trade_count: raw.shadow_trade_count,
        });
    }

    let candidate = validate_bar(
        &document.candidate.instrument,
        document.candidate.timeframe_sec,
        document.candidate.close_time_utc,
        &document.candidate.open,
        &document.candidate.high,
        &document.candidate.low,
        &document.candidate.close,
        &document.candidate.volume,
        document.candidate.is_final,
        &document.candidate.origin,
        "replay",
    )
    .ok_or(Stage8bP1eFirstBootSourceError::InvalidCandidate)?;
    let history_tail = history_bars
        .last()
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidHistory)?;
    let tail_session = moscow_session_date(history_tail.close_time_utc)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidHistory)?;
    let candidate_session = moscow_session_date(candidate.close_time_utc)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidCandidate)?;
    if candidate.close_time_utc <= history_tail.close_time_utc
        || candidate.close_time_utc > captured_at.timestamp()
        || candidate_session <= tail_session
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidCandidate);
    }
    let candidate_open_ts_utc_ms = document
        .candidate
        .close_ts_utc_ms
        .checked_sub(600_000)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidCandidate)?;
    if document.candidate.close_ts_utc_ms != candidate.close_time_utc.saturating_mul(1_000)
        || document.candidate.open_ts_utc_ms != candidate_open_ts_utc_ms
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidCandidate);
    }
    let canonical_candidate =
        crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: expected_operational_identity_sha256.to_string(),
            open_ts_utc_ms: document.candidate.open_ts_utc_ms,
            close_ts_utc_ms: document.candidate.close_ts_utc_ms,
            open: document.candidate.open.clone(),
            high: document.candidate.high.clone(),
            low: document.candidate.low.clone(),
            close: document.candidate.close.clone(),
            volume: document.candidate.volume.clone(),
            source_m1: document.candidate.source_m1,
        })
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidCandidate)?;
    let validated_candidate = crate::parse_stage8b_p1_canonical_m10(
        &canonical_candidate,
        expected_operational_identity_sha256,
    )
    .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidCandidate)?;
    if document.candidate.redis_id != validated_candidate.redis_id()
        || document.candidate.semantic_id_sha256 != validated_candidate.semantic_id_sha256()
        || document.candidate.payload_sha256 != validated_candidate.payload_sha256()
    {
        return Err(Stage8bP1eFirstBootSourceError::InvalidCandidate);
    }

    Ok(Stage8bP1eValidatedFirstBootSourceV1 {
        source_bundle_sha256: actual_source_bundle_sha256,
        source_bundle_generation: document.source_bundle_generation,
        captured_at,
        broker_truth_checked_at,
        account_id: document.broker_truth.account_id,
        history_bars_sha256: history_hash,
        riskgate_session_observations_sha256: observations_hash,
        candidate_semantic_id_sha256: validated_candidate.semantic_id_sha256().to_string(),
        candidate_canonical_m10_sha256: sha256_hex(validated_candidate.canonical_bytes()),
        history_bars,
        riskgate_observations,
        candidate,
    })
}

fn history_coverage_is_exact(
    sessions: &[FirstBootHistorySessionV2],
    history_bars: &[Stage8bP1eFirstBootBarV1],
    history_sessions: &BTreeSet<NaiveDate>,
) -> bool {
    if sessions.len() < STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS
        || sessions.len() != history_sessions.len()
    {
        return false;
    }
    let mut expected_closes = Vec::with_capacity(history_bars.len());
    let mut declared_dates = BTreeSet::new();
    let mut prior_date = None;
    for session in sessions {
        let Ok(session_date) = NaiveDate::parse_from_str(&session.session_date, "%Y-%m-%d") else {
            return false;
        };
        if session_date.format("%Y-%m-%d").to_string() != session.session_date
            || prior_date.is_some_and(|prior| prior >= session_date)
            || session.windows.is_empty()
            || matches!(session_date.weekday(), Weekday::Sat | Weekday::Sun)
        {
            return false;
        }
        prior_date = Some(session_date);
        declared_dates.insert(session_date);
        let mut prior_close = None;
        for window in &session.windows {
            if window.first_close_time_utc <= 0
                || window.first_close_time_utc.rem_euclid(600) != 0
                || window.last_close_time_utc < window.first_close_time_utc
                || window.last_close_time_utc.rem_euclid(600) != 0
                || prior_close.is_some_and(|prior| prior >= window.first_close_time_utc)
                || moscow_session_date(window.first_close_time_utc) != Some(session_date)
                || moscow_session_date(window.last_close_time_utc) != Some(session_date)
            {
                return false;
            }
            let mut close = window.first_close_time_utc;
            loop {
                expected_closes.push(close);
                if expected_closes.len() > history_bars.len() || close == window.last_close_time_utc
                {
                    break;
                }
                let Some(next) = close.checked_add(600) else {
                    return false;
                };
                close = next;
            }
            prior_close = Some(window.last_close_time_utc);
        }
    }
    declared_dates == *history_sessions
        && expected_closes.len() == history_bars.len()
        && expected_closes
            .iter()
            .zip(history_bars)
            .all(|(expected, actual)| *expected == actual.close_time_utc)
}

#[allow(clippy::too_many_arguments)]
fn validate_bar(
    instrument: &str,
    timeframe_sec: u64,
    close_time_utc: i64,
    open: &str,
    high: &str,
    low: &str,
    close: &str,
    volume: &str,
    is_final: bool,
    origin: &str,
    expected_origin: &str,
) -> Option<Stage8bP1eFirstBootBarV1> {
    if instrument != STAGE8B_P1_VENUE_SYMBOL
        || timeframe_sec != 600
        || close_time_utc <= 0
        || close_time_utc.rem_euclid(600) != 0
        || !is_final
        || origin != expected_origin
    {
        return None;
    }
    let open_text = open.to_string();
    let high_text = high.to_string();
    let low_text = low.to_string();
    let close_text = close.to_string();
    let volume_text = volume.to_string();
    let open = parse_unsigned_decimal(open)?;
    let high = parse_unsigned_decimal(high)?;
    let low = parse_unsigned_decimal(low)?;
    let close = parse_unsigned_decimal(close)?;
    let volume = parse_unsigned_decimal(volume)?;
    if low > high || high < open.max(close) || low > open.min(close) {
        return None;
    }
    Some(Stage8bP1eFirstBootBarV1 {
        close_time_utc,
        open_text,
        high_text,
        low_text,
        close_text,
        volume_text,
        open,
        high,
        low,
        close,
        volume,
    })
}

fn parse_unsigned_decimal(value: &str) -> Option<Decimal> {
    if value.is_empty()
        || value.starts_with('-')
        || value.starts_with('+')
        || value
            .split_once('.')
            .is_some_and(|(whole, fractional)| whole.is_empty() || fractional.is_empty())
        || value.matches('.').count() > 1
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    Decimal::from_str_exact(value).ok()
}

fn parse_signed_decimal(value: &str) -> Option<Decimal> {
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    if value.starts_with('+')
        || unsigned.is_empty()
        || (unsigned.len() > 1 && unsigned.starts_with('0') && !unsigned.starts_with("0."))
    {
        return None;
    }
    parse_unsigned_decimal(unsigned).map(|parsed| {
        if value.starts_with('-') {
            -parsed
        } else {
            parsed
        }
    })
}

fn parse_canonical_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .ok()?
        .with_timezone(&Utc);
    if parsed.nanosecond() != 0
        || parsed.to_rfc3339_opts(chrono::SecondsFormat::Secs, true) != value
    {
        return None;
    }
    Some(parsed)
}

fn moscow_session_date(close_time_utc: i64) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp(close_time_utc, 0)
        .and_then(|value| value.checked_add_signed(Duration::hours(3)))
        .map(|value| value.date_naive())
}

fn service_group_gid() -> Result<u32, Stage8bP1eFirstBootSourceError> {
    let name = CString::new(STAGE8B_P1E_FIRST_BOOT_SOURCE_GROUP)
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidFileBoundary)?;
    // SAFETY: `name` is NUL-terminated and remains alive for the call. We copy
    // only the numeric gid from the process-local group database result.
    let group = unsafe { libc::getgrnam(name.as_ptr()) };
    if group.is_null() {
        return Err(Stage8bP1eFirstBootSourceError::InvalidFileBoundary);
    }
    Ok(unsafe { (*group).gr_gid })
}

fn parse_json_without_duplicates(bytes: &[u8]) -> Result<Value, Stage8bP1eFirstBootSourceError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = NoDuplicateJson::deserialize(&mut deserializer)
        .map_err(|error| {
            if error.to_string().contains("duplicate JSON key") {
                Stage8bP1eFirstBootSourceError::DuplicateJsonKey
            } else {
                Stage8bP1eFirstBootSourceError::InvalidJson
            }
        })?
        .0;
    deserializer
        .end()
        .map_err(|_| Stage8bP1eFirstBootSourceError::InvalidJson)?;
    Ok(value)
}

fn canonical_member_sha256(
    value: &Value,
    member: &str,
) -> Result<String, Stage8bP1eFirstBootSourceError> {
    value
        .get(member)
        .map(canonical_value_sha256)
        .ok_or(Stage8bP1eFirstBootSourceError::InvalidSchema)
}

fn canonical_value_sha256(value: &Value) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("serde_json::Value remains serializable"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

struct NoDuplicateJson(Value);

impl<'de> Deserialize<'de> for NoDuplicateJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NoDuplicateVisitor;

        impl<'de> Visitor<'de> for NoDuplicateVisitor {
            type Value = NoDuplicateJson;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(NoDuplicateJson)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::String(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(NoDuplicateJson(Value::Null))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                NoDuplicateJson::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<NoDuplicateJson>()? {
                    values.push(value.0);
                }
                Ok(NoDuplicateJson(Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    let value = map.next_value::<NoDuplicateJson>()?;
                    values.insert(key, value.0);
                }
                Ok(NoDuplicateJson(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(NoDuplicateVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::CString,
        fs,
        os::unix::{ffi::OsStrExt, fs::PermissionsExt},
        path::{Path, PathBuf},
        time::{Duration as StdDuration, Instant},
    };

    use chrono::{Datelike, TimeZone, Weekday};
    use serde_json::json;

    use super::*;

    fn fixture() -> (Vec<u8>, DateTime<Utc>, String, String) {
        fixture_for_binding(&"1".repeat(64), "ACC_TEST_0001")
    }

    fn fixture_for_binding(
        operational: &str,
        account: &str,
    ) -> (Vec<u8>, DateTime<Utc>, String, String) {
        let mut day = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let mut history = Vec::new();
        let mut observations = Vec::new();
        let mut coverage_sessions = Vec::new();
        let mut session_count = 0_usize;
        while session_count < STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS {
            if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
                let first_close = Utc
                    .with_ymd_and_hms(day.year(), day.month(), day.day(), 6, 10, 0)
                    .single()
                    .unwrap()
                    .timestamp();
                let last_close = Utc
                    .with_ymd_and_hms(day.year(), day.month(), day.day(), 20, 40, 0)
                    .single()
                    .unwrap()
                    .timestamp();
                let mut close_time_utc = first_close;
                let mut bar_index = 0_usize;
                while close_time_utc <= last_close {
                    let (open, high, low, close) = if bar_index == 0 {
                        if session_count == 0 {
                            ("2200", "2210", "2190", "2200")
                        } else {
                            ("2200", "2210", "2190", "2201.5")
                        }
                    } else if session_count > 0 && bar_index == 1 {
                        ("2201.5", "2202", "2199.5", "2200")
                    } else {
                        ("2200", "2201", "2199", "2200")
                    };
                    history.push(json!({
                        "instrument": STAGE8B_P1_VENUE_SYMBOL,
                        "timeframe_sec": 600,
                        "close_time_utc": close_time_utc,
                        "open": open,
                        "high": high,
                        "low": low,
                        "close": close,
                        "volume": "10",
                        "is_final": true,
                        "origin": "history"
                    }));
                    close_time_utc += 600;
                    bar_index += 1;
                }
                coverage_sessions.push(json!({
                    "session_date": day.format("%Y-%m-%d").to_string(),
                    "windows": [{
                        "first_close_time_utc": first_close,
                        "last_close_time_utc": last_close
                    }]
                }));
                observations.push(json!({
                    "session_date": day.format("%Y-%m-%d").to_string(),
                    "shadow_pnl_points": if session_count == 0 { "0.0" } else { "1.4" },
                    "shadow_trade_count": if session_count == 0 { 0 } else { 1 }
                }));
                session_count += 1;
            }
            day = day.succ_opt().unwrap();
        }
        while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            day = day.succ_opt().unwrap();
        }
        let candidate_close = Utc
            .with_ymd_and_hms(day.year(), day.month(), day.day(), 12, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        let captured = Utc.timestamp_opt(candidate_close + 30, 0).single().unwrap();
        let history_hash = canonical_value_sha256(&Value::Array(history.clone()));
        let coverage_hash = canonical_value_sha256(&Value::Array(coverage_sessions.clone()));
        let observation_hash = canonical_value_sha256(&Value::Array(observations.clone()));
        let close_ts_utc_ms = candidate_close * 1_000;
        let open_ts_utc_ms = close_ts_utc_ms - 600_000;
        let source_m1 = (0_i64..10)
            .map(|index| {
                let open = open_ts_utc_ms + index * 60_000;
                let close = open + 60_000;
                crate::Stage8bP1CanonicalM10SourceM1 {
                    redis_id: format!("{close}-0"),
                    semantic_id_sha256: sha256_hex(
                        format!("candidate-m1-semantic-{index}").as_bytes(),
                    ),
                    payload_sha256: sha256_hex(format!("candidate-m1-payload-{index}").as_bytes()),
                    open_ts_utc_ms: open,
                    close_ts_utc_ms: close,
                }
            })
            .collect::<Vec<_>>();
        let candidate_bytes =
            crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
                operational_identity_sha256: operational.to_string(),
                open_ts_utc_ms,
                close_ts_utc_ms,
                open: "2200".to_string(),
                high: "2201".to_string(),
                low: "2199".to_string(),
                close: "2200".to_string(),
                volume: "20".to_string(),
                source_m1: source_m1.clone(),
            })
            .unwrap();
        let candidate =
            crate::parse_stage8b_p1_canonical_m10(&candidate_bytes, operational).unwrap();
        let value = json!({
            "schema_version": STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION,
            "domain": STAGE8B_P1E_FIRST_BOOT_SOURCE_DOMAIN,
            "operational_identity_sha256": operational,
            "runtime_profile_sha256": STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            "instrument_map_fingerprint_sha256": stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            "source_bundle_generation": 1,
            "captured_at_utc": captured.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "broker_truth": {
                "checked_at_utc": (captured - Duration::seconds(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "account_id": account,
                "instrument": STAGE8B_P1_VENUE_SYMBOL,
                "target_position_qty": "0",
                "target_positions_complete": true,
                "target_active_orders_count": 0,
                "account_active_orders_count": 0,
                "active_orders_complete": true,
                "instrument_price_step": "0.5"
            },
            "history_provenance": {
                "source_mode": "finam_derived_m1_to_m10",
                "source_timeframe_sec": 60,
                "target_timeframe_sec": 600,
                "aggregation_complete": true,
                "gap_absence_proven": true
            },
            "history_coverage": {
                "source_mode": "config-bound-explicit-session-windows-v1",
                "sessions_sha256": coverage_hash,
                "sessions": coverage_sessions
            },
            "history_bars": history,
            "riskgate_history": {
                "source_mode": "source-compatible-high180-shadow-history-v1",
                "state_generation": "runtime-ledger-v1",
                "history_bars_sha256": history_hash,
                "session_observations_sha256": observation_hash,
                "session_observations": observations
            },
            "candidate": {
                "instrument": STAGE8B_P1_VENUE_SYMBOL,
                "timeframe_sec": 600,
                "close_time_utc": candidate_close,
                "open": "2200",
                "high": "2201",
                "low": "2199",
                "close": "2200",
                "volume": "20",
                "is_final": true,
                "origin": "replay",
                "redis_id": candidate.redis_id(),
                "semantic_id_sha256": candidate.semantic_id_sha256(),
                "payload_sha256": candidate.payload_sha256(),
                "open_ts_utc_ms": open_ts_utc_ms,
                "close_ts_utc_ms": close_ts_utc_ms,
                "source_m1": source_m1
            }
        });
        (
            serde_json::to_vec(&value).unwrap(),
            captured + Duration::seconds(1),
            operational.to_string(),
            account.to_string(),
        )
    }

    fn build_composition(
        source: Stage8bP1eValidatedFirstBootSourceV1,
        operational: String,
        account: String,
    ) -> Result<
        strategy_runtime_core::Stage8bP1eFirstBootCompositionV1,
        strategy_runtime_core::Stage8bP1eFirstBootCompositionError,
    > {
        let (runtime, fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        let (fresh_runtime, fresh_fingerprint) =
            Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        assert_eq!(fingerprint, fresh_fingerprint);
        strategy_runtime_core::build_stage8b_p1_first_boot_composition_v1(
            strategy_runtime_core::Stage8bP1eFirstBootCompositionInputV1 {
                runtime,
                fresh_runtime,
                account_id: broker_core::BrokerAccountId::new(account),
                operational_identity_sha256: operational,
                captured_at: source.captured_at,
                broker_truth_checked_at: source.broker_truth_checked_at,
                history_bars_sha256: source.history_bars_sha256,
                riskgate_session_observations_sha256: source.riskgate_session_observations_sha256,
                validated_candidate_semantic_id_sha256: source.candidate_semantic_id_sha256,
                history_bars: source.history_bars.iter().map(core_bar_input).collect(),
                riskgate_observations: source
                    .riskgate_observations
                    .iter()
                    .map(|observation| {
                        strategy_runtime_core::Stage8bP1eRiskGateObservationInputV1 {
                            session_date: observation.session_date,
                            shadow_pnl_points: observation.shadow_pnl_points_text.clone(),
                            shadow_trade_count: observation.shadow_trade_count,
                        }
                    })
                    .collect(),
                candidate: core_bar_input(&source.candidate),
            },
        )
    }

    fn temp_directory(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stage8b-p1e-first-boot-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        fs::canonicalize(path).unwrap()
    }

    fn loader_file(directory: &Path, bytes: &[u8]) -> PathBuf {
        let path = directory.join("first-boot-source.json");
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    fn current_identity() -> (u32, u32) {
        // SAFETY: these libc calls have no preconditions and do not dereference pointers.
        unsafe { (libc::getuid(), libc::getgid()) }
    }

    fn assert_no_durable_root(parent: &Path) {
        assert_eq!(
            fs::read_dir(parent)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count(),
            0
        );
    }

    fn bootstrap_config(
        durable_parent: PathBuf,
        runtime_config_fingerprint_sha256: String,
    ) -> crate::Stage8bP1BootstrapConfig {
        crate::Stage8bP1BootstrapConfig {
            schema_version: crate::STAGE8B_P1_BOOTSTRAP_CONFIG_SCHEMA_VERSION,
            broker_id: crate::STAGE8B_P1_BROKER_ID.to_string(),
            strategy_id: crate::STAGE8B_P1_STRATEGY_ID.to_string(),
            account_id: "ACC_TEST_0001".to_string(),
            internal_symbol: crate::STAGE8B_P1_INTERNAL_SYMBOL.to_string(),
            venue_symbol: crate::STAGE8B_P1_VENUE_SYMBOL.to_string(),
            exchange: crate::STAGE8B_P1_EXCHANGE.to_string(),
            market: crate::STAGE8B_P1_MARKET.to_string(),
            tick_size: crate::STAGE8B_P1_TICK_SIZE.to_string(),
            runtime_config_fingerprint_sha256,
            instrument_map_fingerprint_sha256:
                crate::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            deployment_id: "finam-imoexf-paper-p1-first-boot-composition".to_string(),
            deployment_generation: 1,
            gateway_instance_id: "finam-imoexf-paper-gateway-1".to_string(),
            market_data_generation: 1,
            command_consumer_generation: 1,
            stage8a4_writer_issuer_public_key_hex: "3".repeat(64),
            durable_parent,
        }
    }

    fn prepared_transaction(
        parent: &Path,
    ) -> (
        Stage8bP1ePreparedFirstBootV1,
        crate::Stage8bP1FirstBootAdminCommand,
        strategy_runtime_core::Stage5gLifecycleCommitmentKey,
        String,
    ) {
        let (_, runtime_fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        let validated = crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
            parent.to_path_buf(),
            runtime_fingerprint.clone(),
        ))
        .unwrap();
        let operational = validated.operational_identity_sha256().to_string();
        let (bytes, now, _, account) = fixture_for_binding(&operational, "ACC_TEST_0001");
        let source = parse_fixture(&bytes, now, &operational, &account).unwrap();
        let provenance = strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1::new(
            operational.clone(),
            STAGE8B_P1E_RUNTIME_PROFILE_SHA256.to_string(),
            runtime_fingerprint.clone(),
            source.source_bundle_sha256.clone(),
            source.source_bundle_generation,
            STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256.to_string(),
            source.history_bars_sha256.clone(),
            source.riskgate_session_observations_sha256.clone(),
            source.candidate_semantic_id_sha256.clone(),
        )
        .unwrap();
        let composition = build_composition(source, operational, account).unwrap();
        let (source, export_input, fresh_runtime) = composition.into_parts();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let key =
            strategy_runtime_core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x8b; 32])
                .unwrap();
        (
            Stage8bP1ePreparedFirstBootV1 {
                bootstrap: validated,
                source,
                export_input,
                fresh_runtime,
                provenance,
            },
            admin,
            key,
            runtime_fingerprint,
        )
    }

    fn parse_fixture(
        bytes: &[u8],
        now: DateTime<Utc>,
        operational: &str,
        account: &str,
    ) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
        parse_stage8b_p1e_first_boot_source_v1(bytes, &sha256_hex(bytes), operational, account, now)
    }

    #[test]
    fn exact_source_bundle_is_accepted() {
        let (bytes, now, operational, account) = fixture();
        let source = parse_fixture(&bytes, now, &operational, &account).unwrap();
        assert_eq!(source.source_bundle_sha256(), sha256_hex(&bytes));
        assert_eq!(source.history_bars().len(), 121 * 88);
        assert_eq!(source.riskgate_observations().len(), 121);
        assert!(source
            .riskgate_observations()
            .iter()
            .any(|observation| observation.shadow_trade_count == 1
                && observation.shadow_pnl_points_text == "1.4"));
        assert_eq!(source.candidate().close_time_utc % 600, 0);
        assert!(is_sha256_hex(source.candidate_canonical_m10_sha256()));
    }

    #[test]
    fn authenticated_observations_build_complete_first_boot_composition() {
        let (bytes, now, operational, account) = fixture();
        let source = parse_fixture(&bytes, now, &operational, &account).unwrap();
        let candidate_close_time = source.candidate.close_time_utc;
        let candidate_semantic_id = source.candidate_semantic_id_sha256.clone();
        let composition = build_composition(source, operational, account).unwrap();
        let (_timer_ready, export, _) = composition.into_parts();
        assert_eq!(export.snapshot_revision, 1);
        assert_eq!(export.previous_revision, None);
        assert_eq!(export.write_generation, 1);
        assert_eq!(
            export.lifecycle_watermarks.persisted_event_watermark,
            Some(candidate_semantic_id)
        );
        assert_eq!(export.riskgate.materialized_state.ledger_rows_count, 121);
        assert_eq!(
            export
                .riskgate
                .materialized_state
                .current_shadow_session_date,
            Some(
                moscow_session_date(candidate_close_time)
                    .unwrap()
                    .format("%Y-%m-%d")
                    .to_string()
            )
        );
    }

    #[test]
    fn rehashed_forged_riskgate_observation_cannot_bypass_source_oracle() {
        let (bytes, now, operational, account) = fixture();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["riskgate_history"]["session_observations"][0]["shadow_pnl_points"] =
            Value::String("1.0".to_string());
        value["riskgate_history"]["session_observations_sha256"] = Value::String(
            canonical_value_sha256(&value["riskgate_history"]["session_observations"]),
        );
        let forged = serde_json::to_vec(&value).unwrap();
        let source = parse_fixture(&forged, now, &operational, &account)
            .expect("F00 authenticates the internally self-consistent observation bytes");
        assert_eq!(
            build_composition(source, operational, account).err(),
            Some(strategy_runtime_core::Stage8bP1eFirstBootCompositionError::RiskGate)
        );
    }

    #[test]
    fn composition_exports_restores_then_creates_the_single_durable_root() {
        let parent = temp_directory("durable-root");
        let (_, runtime_fingerprint) = Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
        let raw = bootstrap_config(parent.clone(), runtime_fingerprint.clone());
        let validated = crate::validate_stage8b_p1_bootstrap_config(raw).unwrap();
        let operational = validated.operational_identity_sha256().to_string();
        let (bytes, now, _, account) = fixture_for_binding(&operational, "ACC_TEST_0001");
        let source = parse_fixture(&bytes, now, &operational, &account).unwrap();
        let candidate_semantic_id = source.candidate_semantic_id_sha256.clone();
        let provenance = strategy_runtime_core::Stage8bP1eFirstBootProvenanceV1::new(
            operational.clone(),
            STAGE8B_P1E_RUNTIME_PROFILE_SHA256.to_string(),
            runtime_fingerprint.clone(),
            source.source_bundle_sha256.clone(),
            source.source_bundle_generation,
            STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256.to_string(),
            source.history_bars_sha256.clone(),
            source.riskgate_session_observations_sha256.clone(),
            source.candidate_semantic_id_sha256.clone(),
        )
        .unwrap();
        let composition = build_composition(source, operational, account).unwrap();
        let (source, export_input, fresh_runtime) = composition.into_parts();
        assert_eq!(
            export_input.lifecycle_watermarks.persisted_event_watermark,
            Some(candidate_semantic_id)
        );
        let restart_runtime = fresh_runtime.clone();
        let durable_root_name = validated.expected_root_name().to_string();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &validated,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let key =
            strategy_runtime_core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x8b; 32])
                .unwrap();
        let outcome = crate::first_boot_stage8b_p1e_transaction_v5(
            Stage8bP1ePreparedFirstBootV1 {
                bootstrap: validated,
                source,
                export_input,
                fresh_runtime,
                provenance,
            },
            admin,
            1,
            &key,
        )
        .expect("F15 validation precedes the sole F17 durable-root creation");
        assert!(outcome.owner().recovery_ready());
        assert_eq!(outcome.receipt().schema_version, 2);
        assert_eq!(outcome.receipt().restart_package_schema_version, 2);
        drop(outcome);

        let inspection = crate::classify_stage8b_p1e_first_boot_v5(
            crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                runtime_fingerprint.clone(),
            ))
            .unwrap(),
            &key,
            restart_runtime.clone(),
        );
        assert_eq!(
            inspection.classification,
            crate::Stage8bP1eFirstBootClassificationV5::AdoptedCommittedRoot
        );
        assert!(inspection.ordinary_run_allowed);

        let restart = crate::restart_stage8b_p1(
            crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                parent.clone(),
                runtime_fingerprint,
            ))
            .unwrap(),
            &key,
            restart_runtime,
        )
        .expect("the source-produced root restarts without another first boot");
        assert!(restart.recovery_ready());
        drop(restart);
        assert!(parent.join(&durable_root_name).is_dir());
        assert!(parent
            .join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)
            .is_file());
        assert!(parent
            .join(crate::STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE)
            .is_file());
        assert_eq!(
            fs::read_dir(&parent)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count(),
            1
        );
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn every_v5_crash_hook_has_one_exact_fail_closed_classification() {
        use crate::Stage8bP1eFirstBootClassificationV5 as Classification;

        let cases = [
            (
                "after-prepared-marker-temp-sync-before-rename",
                Classification::UnpublishedMarkerTemp,
            ),
            (
                "after-root-published-marker-temp-sync-before-rename",
                Classification::PreparedToRootPublishedMarkerTempPending,
            ),
            (
                "after-root-parent-fsync-before-journal-create",
                Classification::RootWithoutJournal,
            ),
            (
                "after-journal-durable-marker-temp-sync-before-rename",
                Classification::RootPublishedToJournalDurableMarkerTempPending,
            ),
            (
                "after-journal-fsync-before-initial-seal-commit",
                Classification::JournalWithoutSeal,
            ),
            (
                "after-seal-committed-marker-temp-sync-before-rename",
                Classification::JournalDurableToSealCommittedMarkerTempPending,
            ),
            (
                "after-seal-persist-reread-before-bootstrap-success-report",
                Classification::CommittedRootResponseLost,
            ),
            (
                "after-receipt-temp-sync-before-final-rename",
                Classification::CommittedRootReceiptTemp,
            ),
            (
                "after-receipt-rename-parent-fsync-before-adopted-marker-temp-create",
                Classification::ReceiptCommittedMarkerUpdatePending,
            ),
            (
                "after-adopted-marker-temp-sync-before-rename",
                Classification::SealCommittedToAdoptedMarkerTempPending,
            ),
        ];

        for (hook, expected) in cases {
            let parent = temp_directory(hook);
            let (prepared, admin, key, runtime_fingerprint) = prepared_transaction(&parent);
            let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::stage8b_p1e_first_boot_transaction::test_first_boot_stage8b_p1e_transaction_v5_with_observer(
                    prepared,
                    admin,
                    1,
                    &key,
                    |observed| {
                        assert_ne!(observed, "unexpected-hook");
                        if observed == hook {
                            panic!("simulated-sigkill-at-{hook}");
                        }
                    },
                )
                .unwrap();
            }));
            assert!(interrupted.is_err(), "hook {hook} was not reached");
            let (fresh_runtime, fresh_fingerprint) =
                Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
            assert_eq!(fresh_fingerprint, runtime_fingerprint);
            let inspection = crate::classify_stage8b_p1e_first_boot_v5(
                crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                    parent.clone(),
                    runtime_fingerprint.clone(),
                ))
                .unwrap(),
                &key,
                fresh_runtime,
            );
            assert_eq!(inspection.classification, expected, "hook {hook}");
            assert!(!inspection.ordinary_run_allowed, "hook {hook}");

            let recovery_action = match expected {
                Classification::CommittedRootResponseLost => {
                    Some(crate::Stage8bP1eAdoptionRecoveryActionV5::AdoptCommittedRoot)
                }
                Classification::CommittedRootReceiptTemp => {
                    Some(crate::Stage8bP1eAdoptionRecoveryActionV5::RemoveReceiptTempAndAdopt)
                }
                Classification::ReceiptCommittedMarkerUpdatePending => {
                    Some(crate::Stage8bP1eAdoptionRecoveryActionV5::StartSealCommittedToAdopted)
                }
                Classification::SealCommittedToAdoptedMarkerTempPending => {
                    Some(crate::Stage8bP1eAdoptionRecoveryActionV5::CompleteSealCommittedToAdopted)
                }
                _ => None,
            };
            if let Some(action) = recovery_action {
                if expected == Classification::CommittedRootResponseLost {
                    let marker_before =
                        fs::read(parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)).unwrap();
                    let (fresh_runtime, fresh_fingerprint) =
                        Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
                    assert_eq!(fresh_fingerprint, runtime_fingerprint);
                    assert_eq!(
                        crate::recover_stage8b_p1e_first_boot_adoption_v5(
                            crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                                parent.clone(),
                                runtime_fingerprint.clone(),
                            ))
                            .unwrap(),
                            &key,
                            fresh_runtime,
                            &"00".repeat(32),
                            action,
                        )
                        .err(),
                        Some(crate::Stage8bP1eFirstBootTransactionError::RecoverySelectorMismatch)
                    );
                    assert_eq!(
                        fs::read(parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)).unwrap(),
                        marker_before
                    );
                    assert!(!parent
                        .join(crate::STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE)
                        .exists());
                }

                if matches!(
                    expected,
                    Classification::ReceiptCommittedMarkerUpdatePending
                        | Classification::SealCommittedToAdoptedMarkerTempPending
                ) {
                    let marker_before =
                        fs::read(parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)).unwrap();
                    let final_receipt = parent.join(crate::STAGE8B_P1E_FIRST_BOOT_RECEIPT_FILE);
                    let stale_temp = parent.join(crate::STAGE8B_P1E_FIRST_BOOT_RECEIPT_TEMP_FILE);
                    fs::copy(&final_receipt, &stale_temp).unwrap();
                    fs::set_permissions(&stale_temp, fs::Permissions::from_mode(0o600)).unwrap();
                    let (fresh_runtime, fresh_fingerprint) =
                        Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
                    assert_eq!(fresh_fingerprint, runtime_fingerprint);
                    let stale = crate::classify_stage8b_p1e_first_boot_v5(
                        crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                            parent.clone(),
                            runtime_fingerprint.clone(),
                        ))
                        .unwrap(),
                        &key,
                        fresh_runtime,
                    );
                    assert_eq!(
                        stale.classification,
                        Classification::CorruptOrIdentityMismatch,
                        "stale receipt temp at {hook}"
                    );
                    assert_eq!(
                        fs::read(parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)).unwrap(),
                        marker_before
                    );
                    fs::remove_file(stale_temp).unwrap();
                }

                let transaction_id = inspection
                    .transaction_id_sha256
                    .as_deref()
                    .expect("post-seal classification binds one transaction");
                let (fresh_runtime, fresh_fingerprint) =
                    Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
                assert_eq!(fresh_fingerprint, runtime_fingerprint);
                let outcome = crate::recover_stage8b_p1e_first_boot_adoption_v5(
                    crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                        parent.clone(),
                        runtime_fingerprint.clone(),
                    ))
                    .unwrap(),
                    &key,
                    fresh_runtime,
                    transaction_id,
                    action,
                )
                .expect("the exact response-loss selector completes adoption");
                assert!(outcome.owner().recovery_ready());
                assert!(outcome
                    .owner()
                    .recovered()
                    .unwrap()
                    .stage8b_p1e_initial_adoption_ready());
                drop(outcome);

                let (fresh_runtime, fresh_fingerprint) =
                    Stage8bP1RuntimeProfileV1::build_hybrid_runtime().unwrap();
                assert_eq!(fresh_fingerprint, runtime_fingerprint);
                let adopted = crate::classify_stage8b_p1e_first_boot_v5(
                    crate::validate_stage8b_p1_bootstrap_config(bootstrap_config(
                        parent.clone(),
                        runtime_fingerprint.clone(),
                    ))
                    .unwrap(),
                    &key,
                    fresh_runtime,
                );
                assert_eq!(
                    adopted.classification,
                    Classification::AdoptedCommittedRoot,
                    "recovery after {hook}"
                );
                assert!(adopted.ordinary_run_allowed);
            }
            fs::remove_dir_all(parent).unwrap();
        }
    }

    #[test]
    fn byte_hash_and_identity_are_fail_closed() {
        let (bytes, now, operational, account) = fixture();
        assert_eq!(
            parse_stage8b_p1e_first_boot_source_v1(
                &bytes,
                &"0".repeat(64),
                &operational,
                &account,
                now,
            )
            .err(),
            Some(Stage8bP1eFirstBootSourceError::SourceHashMismatch)
        );
        assert_eq!(
            parse_fixture(&bytes, now, &"f".repeat(64), &account).err(),
            Some(Stage8bP1eFirstBootSourceError::IdentityMismatch)
        );
    }

    #[test]
    fn duplicate_key_is_rejected_before_typed_decode() {
        let bytes = br#"{"schema_version":1,"schema_version":1}"#;
        assert_eq!(
            parse_stage8b_p1e_first_boot_source_v1(
                bytes,
                &sha256_hex(bytes),
                &"1".repeat(64),
                "ACC_TEST_0001",
                Utc::now(),
            )
            .err(),
            Some(Stage8bP1eFirstBootSourceError::DuplicateJsonKey)
        );
    }

    #[test]
    fn legacy_wire_schema_is_rejected_explicitly() {
        let (bytes, now, operational, account) = fixture();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["schema_version"] = Value::from(1);
        value["domain"] = Value::String("moex.stage8b.p1e.first-boot-source-bundle.v1".to_owned());
        let legacy = serde_json::to_vec(&value).unwrap();

        assert_eq!(
            parse_fixture(&legacy, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::IdentityMismatch)
        );
    }

    #[test]
    fn stale_truth_and_tampered_observation_hash_are_rejected() {
        let (bytes, now, operational, account) = fixture();
        assert_eq!(
            parse_fixture(
                &bytes,
                now + Duration::seconds(STAGE8B_P1E_FIRST_BOOT_TRUTH_MAX_AGE_SECONDS + 1),
                &operational,
                &account,
            )
            .err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidBrokerTruth)
        );

        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["riskgate_history"]["session_observations_sha256"] = Value::String("f".repeat(64));
        let tampered = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            parse_fixture(&tampered, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidRiskGateHistory)
        );
    }

    #[test]
    fn invalid_ohlc_and_same_session_candidate_are_rejected() {
        let (bytes, now, operational, account) = fixture();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["history_bars"][0]["high"] = Value::String("2190.0".to_string());
        value["riskgate_history"]["history_bars_sha256"] =
            Value::String(canonical_value_sha256(&value["history_bars"]));
        let invalid = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            parse_fixture(&invalid, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidHistory)
        );

        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        let tail = value["history_bars"].as_array().unwrap().last().unwrap()["close_time_utc"]
            .as_i64()
            .unwrap();
        value["candidate"]["close_time_utc"] = Value::Number((tail + 600).into());
        let same_session = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            parse_fixture(&same_session, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidCandidate)
        );
    }

    #[test]
    fn redigested_candidate_identity_mismatch_and_material_change_are_rejected() {
        let (bytes, now, operational, account) = fixture();
        let parent = temp_directory("candidate-rejection-no-root");
        let mut wrong_identity: Value = serde_json::from_slice(&bytes).unwrap();
        wrong_identity["candidate"]["semantic_id_sha256"] = Value::String("f".repeat(64));
        let wrong_identity = serde_json::to_vec(&wrong_identity).unwrap();
        assert_eq!(
            parse_fixture(&wrong_identity, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidCandidate)
        );
        assert_no_durable_root(&parent);

        let mut changed_material: Value = serde_json::from_slice(&bytes).unwrap();
        changed_material["candidate"]["close"] = Value::String("2200.5".to_string());
        changed_material["candidate"]["high"] = Value::String("2201.5".to_string());
        let changed_material = serde_json::to_vec(&changed_material).unwrap();
        assert_eq!(
            parse_fixture(&changed_material, now, &operational, &account).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidCandidate)
        );
        assert_no_durable_root(&parent);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn redigested_history_gap_tail_truncation_and_uncovered_date_are_rejected() {
        let (bytes, now, operational, account) = fixture();
        let parent = temp_directory("history-rejection-no-root");
        for mutation in ["internal-gap", "truncated-tail", "uncovered-date"] {
            let mut value: Value = serde_json::from_slice(&bytes).unwrap();
            let history = value["history_bars"].as_array_mut().unwrap();
            match mutation {
                "internal-gap" => {
                    history.remove(10);
                }
                "truncated-tail" => {
                    history.pop();
                }
                "uncovered-date" => {
                    let mut bar = history.last().unwrap().clone();
                    bar["close_time_utc"] = Value::Number(
                        (bar["close_time_utc"].as_i64().unwrap() + 3 * 86_400).into(),
                    );
                    history.push(bar);
                }
                _ => unreachable!(),
            }
            value["riskgate_history"]["history_bars_sha256"] =
                Value::String(canonical_value_sha256(&value["history_bars"]));
            let redigested = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                parse_fixture(&redigested, now, &operational, &account).err(),
                Some(Stage8bP1eFirstBootSourceError::InvalidHistory),
                "mutation {mutation}"
            );
            assert_no_durable_root(&parent);
        }
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn f00_regular_single_link_file_is_read_completely() {
        let directory = temp_directory("loader-regular");
        let (bytes, now, operational, account) = fixture();
        let path = loader_file(&directory, &bytes);
        let (uid, gid) = current_identity();
        let loaded = read_protected_first_boot_source(&path, uid, gid, || {}).unwrap();
        assert_eq!(loaded, bytes);
        parse_fixture(&loaded, now, &operational, &account).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f00_symlink_and_hardlink_are_rejected() {
        let directory = temp_directory("loader-links");
        let target = loader_file(&directory, b"source");
        let symlink_path = directory.join("source-link.json");
        std::os::unix::fs::symlink(&target, &symlink_path).unwrap();
        let (uid, gid) = current_identity();
        assert_eq!(
            read_protected_first_boot_source(&symlink_path, uid, gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        let hardlink_path = directory.join("source-hardlink.json");
        fs::hard_link(&target, &hardlink_path).unwrap();
        assert_eq!(
            read_protected_first_boot_source(&target, uid, gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f00_permissions_owner_and_group_are_fail_closed() {
        let directory = temp_directory("loader-identity");
        let path = loader_file(&directory, b"source");
        let (uid, gid) = current_identity();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert_eq!(
            read_protected_first_boot_source(&path, uid, gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_protected_first_boot_source(&path, uid.wrapping_add(1), gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        assert_eq!(
            read_protected_first_boot_source(&path, uid, gid.wrapping_add(1), || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f00_oversize_input_is_rejected_before_read() {
        let directory = temp_directory("loader-oversize");
        let path = loader_file(&directory, b"");
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(STAGE8B_P1E_FIRST_BOOT_SOURCE_MAX_BYTES + 1)
            .unwrap();
        let (uid, gid) = current_identity();
        assert_eq!(
            read_protected_first_boot_source(&path, uid, gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::SourceTooLarge)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f00_change_or_incomplete_read_is_rejected() {
        let directory = temp_directory("loader-change");
        let path = loader_file(&directory, b"complete source");
        let (uid, gid) = current_identity();
        assert_eq!(
            read_protected_first_boot_source(&path, uid, gid, || {
                fs::write(&path, b"short").unwrap();
            })
            .err(),
            Some(Stage8bP1eFirstBootSourceError::SourceReadFailed)
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn f00_fifo_failure_is_bounded() {
        let directory = temp_directory("loader-fifo");
        let path = directory.join("first-boot-source.fifo");
        let raw_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `raw_path` is a live NUL-terminated path and mode is valid.
        assert_eq!(unsafe { libc::mkfifo(raw_path.as_ptr(), 0o600) }, 0);
        let (uid, gid) = current_identity();
        let started = Instant::now();
        assert_eq!(
            read_protected_first_boot_source(&path, uid, gid, || {}).err(),
            Some(Stage8bP1eFirstBootSourceError::InvalidFileBoundary)
        );
        assert!(started.elapsed() < StdDuration::from_secs(1));
        fs::remove_dir_all(directory).unwrap();
    }
}
