//! Stage 5F test-only observation and source-characterization harness.
//!
//! This module is compiled only for tests. It observes the immutable intent
//! slice returned by the exact broker-neutral `Strategy::on_bar` expression,
//! stores only a redacted projection, and never controls runtime flow.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use broker_core::{BrokerAccountId, Exchange, InstrumentId, Market, StrategyRequestId};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::hybrid_intraday::{
    BreakoutEodMode, EntryStyle, HybridOrchestratorConfig, IntradayBreakoutConfig,
    MeanReversionConfig, MinRangeMode, Owner, ReasonCode, Side,
};
use crate::hybrid_intraday_runtime::{
    HybridIntradayProfile, HybridIntradayRuntimeConfig, HybridIntradayRuntimeStrategy,
    MeanReversionVariant, MrGatePolicy, RiskGateMode,
};
use crate::runtime_compat::{Intent, IntentClass, OrderSide, StopLimitCondition};
use crate::runtime_compat::{MarketBuyAndCloseLiveOrderStyle, RiskGateRuntimeState, Strategy};

const COMMENT_HASH_DOMAIN: &[u8] = b"moex.stage5f.intent-comment.v1\0";
const CYCLE_HASH_DOMAIN: &[u8] = b"moex.stage5f.cycle-id.v1\0";
const ORDER_ID_HASH_DOMAIN: &[u8] = b"moex.stage5f.broker-order-id.v1\0";
const STOP_ID_HASH_DOMAIN: &[u8] = b"moex.stage5f.broker-stop-id.v1\0";

enum ObservationSlot {
    Idle,
    Armed {
        generation: u64,
    },
    Observed {
        generation: u64,
        vector: Stage5fObservedIntentVector,
    },
}

struct ObservationState {
    next_generation: u64,
    slot: ObservationSlot,
}

thread_local! {
    static OBSERVATION_STATE: RefCell<ObservationState> = const {
        RefCell::new(ObservationState {
            next_generation: 1,
            slot: ObservationSlot::Idle,
        })
    };
}

/// Linear test-only capability. It intentionally implements none of Clone,
/// Copy, Debug, Display, Default, Serialize or Deserialize.
pub(crate) struct Stage5fObservationScope {
    generation: u64,
    consumed: bool,
}

/// Linear redacted observer result. Raw intents never enter this type.
pub(crate) struct Stage5fObservedIntentVector {
    projections: Vec<Stage5fIntentProjection>,
}

struct Stage5fIntentProjection {
    ordinal: usize,
    intent_class: Option<&'static str>,
    base_action: &'static str,
    route_symbol: Option<String>,
    owner: Option<String>,
    role: Option<String>,
    side: Option<&'static str>,
    cycle_id_domain_sha256: Option<String>,
    quantity_f64_bits_be: Option<String>,
    price_f64_bits_be: Option<String>,
    trigger_f64_bits_be: Option<String>,
    fill_f64_bits_be: Option<String>,
    stop_end_unix_time: Option<i64>,
    broker_order_id_domain_sha256: Option<String>,
    broker_stop_id_domain_sha256: Option<String>,
    comment_present: bool,
    comment_domain_sha256: Option<String>,
    check_duplicates: Option<bool>,
    condition_flags: Vec<&'static str>,
}

pub(crate) fn arm_observer_once() -> Stage5fObservationScope {
    OBSERVATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(
            matches!(state.slot, ObservationSlot::Idle),
            "Stage 5F observer must be idle before arming"
        );
        let generation = state.next_generation;
        state.next_generation = state.next_generation.wrapping_add(1).max(1);
        state.slot = ObservationSlot::Armed { generation };
        Stage5fObservationScope {
            generation,
            consumed: false,
        }
    })
}

/// Called only from the exact callback expression. If no Stage 5F scenario is
/// armed this is deliberately a no-op so existing unit tests remain unchanged.
pub(crate) fn observe_exact_on_bar_result(intents: &[Intent]) {
    OBSERVATION_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let generation = match state.slot {
            ObservationSlot::Idle => return,
            ObservationSlot::Armed { generation } => generation,
            ObservationSlot::Observed { .. } => {
                panic!("Stage 5F observer saw a second callback before consume")
            }
        };
        let projections = intents
            .iter()
            .enumerate()
            .map(|(ordinal, intent)| project_intent(ordinal, intent))
            .collect();
        state.slot = ObservationSlot::Observed {
            generation,
            vector: Stage5fObservedIntentVector { projections },
        };
    });
}

impl Stage5fObservationScope {
    pub(crate) fn consume_once(mut self) -> Option<Stage5fObservedIntentVector> {
        let vector = OBSERVATION_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let slot = std::mem::replace(&mut state.slot, ObservationSlot::Idle);
            match slot {
                ObservationSlot::Observed { generation, vector }
                    if generation == self.generation =>
                {
                    Some(vector)
                }
                ObservationSlot::Armed { generation } if generation == self.generation => None,
                other => {
                    state.slot = other;
                    panic!("Stage 5F observer generation mismatch")
                }
            }
        });
        self.consumed = true;
        vector
    }
}

impl Drop for Stage5fObservationScope {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        OBSERVATION_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let generation_matches = match &state.slot {
                ObservationSlot::Armed { generation }
                | ObservationSlot::Observed { generation, .. } => *generation == self.generation,
                ObservationSlot::Idle => false,
            };
            if generation_matches {
                state.slot = ObservationSlot::Idle;
            }
        });
    }
}

impl Stage5fObservedIntentVector {
    fn len(&self) -> usize {
        self.projections.len()
    }
}

#[derive(Serialize)]
struct Stage5fCandidateResult {
    schema_version: u16,
    row_id: String,
    scenario_id: String,
    disposition: &'static str,
    callback_count: usize,
    observer_count: usize,
    settlement_attempt_count: usize,
    pre_state_fingerprint: String,
    accepted_post_state_fingerprint: Option<String>,
    ordered_intent_vector: Vec<Value>,
    ordered_intent_vector_sha256: Option<String>,
    b3f_outcome: String,
    settlement_identity_sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fFixtureSourceV1 {
    path: String,
    sha256: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fInstrumentFixture {
    symbol: String,
    venue_symbol: String,
    exchange: String,
    market: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fCatalogTarget {
    strategy_id: String,
    account_id: String,
    instrument: Stage5fInstrumentFixture,
    profile: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioTarget {
    strategy_id: String,
    account_id: String,
    instrument: Stage5fInstrumentFixture,
    profile: String,
    paper_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fCycleIdentityV2 {
    created_ts_utc: String,
    sequence: u16,
    value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stage5fStateClass {
    Flat,
    OpenPosition,
    FlatAfterTerminalCleanup,
    PendingEntry,
    PendingExit,
    DeferredEntry,
    DeferredExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Stage5fOrchestratorState {
    Flat,
    Pending,
    Open,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fPendingEntryFixture {
    request_id: String,
    owner: Owner,
    side: Side,
    cycle: Stage5fCycleIdentityV2,
    created_ts_utc: String,
    reason: ReasonCode,
    entry_style: EntryStyle,
    target_qty: String,
    stop_price: Option<String>,
    take_price: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fPendingExitFixture {
    request_id: String,
    owner: Owner,
    side: Side,
    cycle: Stage5fCycleIdentityV2,
    created_ts_utc: String,
    reason: ReasonCode,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fDeferredEntryFixture {
    original_request_id: String,
    owner: Owner,
    side: Side,
    cycle: Stage5fCycleIdentityV2,
    entry_style: EntryStyle,
    reason: ReasonCode,
    stop_price: Option<String>,
    take_price: Option<String>,
    deferred_ts_utc: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fDeferredExitFixture {
    original_request_id: String,
    owner: Owner,
    reason: ReasonCode,
    cycle: Stage5fCycleIdentityV2,
    deferred_ts_utc: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fStateSeedV2 {
    seed_id: String,
    state_class: Stage5fStateClass,
    position_qty: String,
    current_owner: Option<Owner>,
    current_side: Option<Side>,
    orchestrator_state: Stage5fOrchestratorState,
    last_processed_bar_ts_utc: String,
    pending_entry: Option<Stage5fPendingEntryFixture>,
    pending_exit: Option<Stage5fPendingExitFixture>,
    deferred_entry: Option<Stage5fDeferredEntryFixture>,
    deferred_exit: Option<Stage5fDeferredExitFixture>,
    was_long_today: bool,
    was_short_today: bool,
    overnight_exit_armed_date: Option<String>,
    active_cycle: Option<Stage5fCycleIdentityV2>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fRiskgateSemanticDefaults {
    mr_enabled_current_session: bool,
    rolling_sum_lb120: String,
    last_finalized_session_date: String,
    ledger_rows_count: usize,
    current_shadow_session_date: String,
    current_shadow_pnl_points: String,
    current_shadow_trade_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fPrivateDefaults {
    cleanup_stop_retry_attempts: u32,
    expected_working_order_ids: Vec<String>,
    expected_working_stop_order_ids: Vec<String>,
    runtime_pending_finalizations: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fStateDefaultsV2 {
    entry_ready: bool,
    safe_mode_close_only: bool,
    last_day_local: String,
    prev_day_close: String,
    prev_day_range: String,
    day_before_close: String,
    current_day_high: String,
    current_day_low: String,
    current_day_close: String,
    today_start_local: String,
    last_bar_close: String,
    next_cycle_seq: u32,
    prev_day_return: String,
    riskgate_semantic_state: Stage5fRiskgateSemanticDefaults,
    private_state: Stage5fPrivateDefaults,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fStateCatalogV2 {
    schema_version: u16,
    fixture_kind: String,
    source_v1: Stage5fFixtureSourceV1,
    target: Stage5fCatalogTarget,
    seed_defaults: Stage5fStateDefaultsV2,
    seeds: Vec<Stage5fStateSeedV2>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fRiskgateSeedV2 {
    seed_id: String,
    authority_state: String,
    risk_gate_mode: String,
    mr_gate_policy: String,
    enforced_for_entry: bool,
    mr_enabled_current_session: Option<bool>,
    mr_enabled_next_session: Option<bool>,
    rolling_sum_lb120: Option<String>,
    last_finalized_session_date: Option<String>,
    ledger_rows_count: usize,
    expected_pre_callback_disposition: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fRiskgateCatalogV2 {
    schema_version: u16,
    fixture_kind: String,
    source_v1: Stage5fFixtureSourceV1,
    target: Option<Value>,
    seeds: Vec<Stage5fRiskgateSeedV2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioBarV2 {
    origin: String,
    is_final: bool,
    timeframe_sec: u32,
    close_time_utc: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioClockV2 {
    event_ts_utc: String,
    callback_ts_utc: String,
    lifecycle_ts_utc: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioBrokerTruthV2 {
    working_order_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fCatalogReferenceV2 {
    catalog_path: String,
    catalog_sha256: String,
    seed_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioExpectedV2 {
    characterization_status: String,
    disposition: String,
    callback_count: usize,
    settlement_attempt_count: usize,
    pre_state_fingerprint: Option<String>,
    accepted_post_state_fingerprint: Option<String>,
    ordered_intent_vector: Option<Value>,
    ordered_intent_vector_sha256: Option<String>,
    b3f_outcome: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioRecordV2 {
    schema_version: u16,
    scenario_id: String,
    row_id: String,
    group_id: String,
    case_id: String,
    target: Stage5fScenarioTarget,
    broker_truth: Stage5fScenarioBrokerTruthV2,
    bar: Stage5fScenarioBarV2,
    clock: Stage5fScenarioClockV2,
    pre_state: Stage5fCatalogReferenceV2,
    riskgate: Stage5fCatalogReferenceV2,
    expected: Stage5fScenarioExpectedV2,
    owning_test: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fCharacterizationPolicyV2 {
    current_status: String,
    pending_outputs_are_acceptance_evidence: bool,
    source_callback_allowed_for_existing_seven_rows_only: bool,
    candidate_outputs_require_separate_freeze: bool,
    correction_overlay_allowed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fPathHashBinding {
    path: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fClockOwnership {
    event_ts_utc: String,
    callback_ts_utc: String,
    lifecycle_ts_utc: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fScenarioCatalogV2 {
    schema_version: u16,
    fixture_kind: String,
    status: String,
    source_v1: Stage5fFixtureSourceV1,
    characterization_policy: Stage5fCharacterizationPolicyV2,
    target_config: Stage5fPathHashBinding,
    clock_ownership: Stage5fClockOwnership,
    records: Vec<Stage5fScenarioRecordV2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fConfigSourceEvidence {
    path: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fClassicMrConfigV1 {
    min_range_long: String,
    max_range_long: String,
    k_long: String,
    take_k_long: String,
    stop_k_long: String,
    min_range_short: String,
    max_range_short: String,
    k_short: String,
    take_k_short: String,
    stop_k_short: String,
    tick_size: String,
    session_end_time: String,
    exit_offset_minutes: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fBreakoutConfigV1 {
    k: String,
    stop1_range: String,
    stop2_range: String,
    big_move_threshold: String,
    min_range: String,
    min_range_mode: String,
    exclude_weekends: bool,
    wait_hours: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fOrchestratorConfigV1 {
    breakout_eod_mode: String,
    breakout_overnight_exit_time: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fClosedSurfaces {
    redis: bool,
    finam: bool,
    transport: bool,
    dispatch: bool,
    runtime_live: bool,
    broker_execution: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage5fTargetConfigV1 {
    schema_version: u16,
    fixture_kind: String,
    status: String,
    source_evidence: Vec<Stage5fConfigSourceEvidence>,
    symbol: String,
    profile: String,
    mr_variant: String,
    mr_gate_policy: String,
    risk_gate_mode: String,
    risk_gate_seed_file: Option<String>,
    risk_gate_ledger_key: Option<String>,
    model_session_start_time: String,
    model_session_end_time: String,
    qty: String,
    live_order_style: String,
    tick_size: String,
    marketable_limit_offset_ticks: i64,
    timezone_offset_hours: i32,
    session_close_hour: u32,
    session_close_minute: u32,
    weekends_off: bool,
    stop_end_buffer_sec: u64,
    repair_deadline_sec: u64,
    sl_escalate_timeout_sec: u64,
    max_repair_retries: u32,
    repair_backoff_base_sec: u64,
    repair_backoff_max_sec: u64,
    pending_timeout_sec: u64,
    partial_entry_fill_timeout_ms: u64,
    classic_mr: Stage5fClassicMrConfigV1,
    breakout: Stage5fBreakoutConfigV1,
    orchestrator: Stage5fOrchestratorConfigV1,
    expected_stage5d_config_fingerprint: Option<String>,
    closed_surfaces: Stage5fClosedSurfaces,
}

impl Stage5fTargetConfigV1 {
    fn into_runtime_config(self) -> HybridIntradayRuntimeConfig {
        assert_eq!(self.schema_version, 1);
        assert_eq!(self.fixture_kind, "stage5f-target-config-v1");
        assert_eq!(self.status, "canonical_r1_non_golden");
        assert_eq!(self.source_evidence.len(), 3);
        for evidence in &self.source_evidence {
            assert_eq!(evidence.sha256.len(), 64);
            assert!(!evidence.path.is_empty());
        }
        assert_eq!(self.profile, "imoexf_primary_riskgate_high180_lb120");
        assert_eq!(self.mr_variant, "high180");
        assert_eq!(self.mr_gate_policy, "shadow_pnl_lb120_positive");
        assert_eq!(self.risk_gate_mode, "normal_append");
        assert_eq!(self.live_order_style, "market");
        assert_eq!(self.breakout.min_range_mode, "absolute");
        assert_eq!(self.orchestrator.breakout_eod_mode, "same_day");
        assert!(
            !self.closed_surfaces.redis
                && !self.closed_surfaces.finam
                && !self.closed_surfaces.transport
                && !self.closed_surfaces.dispatch
                && !self.closed_surfaces.runtime_live
                && !self.closed_surfaces.broker_execution
        );

        let expected_fingerprint = self.expected_stage5d_config_fingerprint;
        let config = HybridIntradayRuntimeConfig {
            symbol: self.symbol,
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
            risk_gate_mode: RiskGateMode::NormalAppend,
            risk_gate_seed_file: self.risk_gate_seed_file,
            risk_gate_ledger_key: self.risk_gate_ledger_key,
            model_session_start_time: Some(parse_time(&self.model_session_start_time)),
            model_session_end_time: Some(parse_time(&self.model_session_end_time)),
            qty: parse_f64(&self.qty),
            live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
            tick_size: parse_f64(&self.tick_size),
            marketable_limit_offset_ticks: self.marketable_limit_offset_ticks,
            timezone_offset_hours: self.timezone_offset_hours,
            session_close_hour: self.session_close_hour,
            session_close_minute: self.session_close_minute,
            weekends_off: self.weekends_off,
            stop_end_buffer_sec: self.stop_end_buffer_sec,
            repair_deadline_sec: self.repair_deadline_sec,
            sl_escalate_timeout_sec: self.sl_escalate_timeout_sec,
            max_repair_retries: self.max_repair_retries,
            repair_backoff_base_sec: self.repair_backoff_base_sec,
            repair_backoff_max_sec: self.repair_backoff_max_sec,
            pending_timeout_sec: self.pending_timeout_sec,
            partial_entry_fill_timeout_ms: self.partial_entry_fill_timeout_ms,
            mr_config: MeanReversionConfig {
                min_range_long: parse_f64(&self.classic_mr.min_range_long),
                max_range_long: parse_f64(&self.classic_mr.max_range_long),
                k_long: parse_f64(&self.classic_mr.k_long),
                take_k_long: parse_f64(&self.classic_mr.take_k_long),
                stop_k_long: parse_f64(&self.classic_mr.stop_k_long),
                min_range_short: parse_f64(&self.classic_mr.min_range_short),
                max_range_short: parse_f64(&self.classic_mr.max_range_short),
                k_short: parse_f64(&self.classic_mr.k_short),
                take_k_short: parse_f64(&self.classic_mr.take_k_short),
                stop_k_short: parse_f64(&self.classic_mr.stop_k_short),
                tick_size: parse_f64(&self.classic_mr.tick_size),
                session_end_time: parse_time(&self.classic_mr.session_end_time),
                exit_offset: Duration::minutes(self.classic_mr.exit_offset_minutes),
            },
            breakout_config: IntradayBreakoutConfig {
                k: parse_f64(&self.breakout.k),
                stop1_range: parse_f64(&self.breakout.stop1_range),
                stop2_range: parse_f64(&self.breakout.stop2_range),
                big_move_threshold: parse_f64(&self.breakout.big_move_threshold),
                min_range: parse_f64(&self.breakout.min_range),
                min_range_mode: MinRangeMode::Absolute,
                exclude_weekends: self.breakout.exclude_weekends,
                wait_hours: parse_f64(&self.breakout.wait_hours),
            },
            orchestrator_config: HybridOrchestratorConfig {
                breakout_eod_mode: BreakoutEodMode::SameDay,
                breakout_overnight_exit_time: parse_time(
                    &self.orchestrator.breakout_overnight_exit_time,
                ),
            },
        };
        if let Some(expected) = expected_fingerprint {
            let actual = HybridIntradayRuntimeStrategy::new(config.clone())
                .stage5d_canonical_config_fingerprint();
            assert_eq!(actual, expected, "canonical Stage 5F config drift");
        }
        config
    }
}

struct Stage5fScenarioInput {
    state_defaults: Stage5fStateDefaultsV2,
    state_seed: Stage5fStateSeedV2,
    riskgate_seed: Stage5fRiskgateSeedV2,
    row_id: String,
    scenario_id: String,
    event_at: DateTime<Utc>,
    callback_at: DateTime<Utc>,
    lifecycle_at: DateTime<Utc>,
    bar: broker_core::HybridRuntimeBarEvent,
    strategy_id: String,
    account_id: BrokerAccountId,
    target: InstrumentId,
    position_qty: f64,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/stage5/stage5f/v2")
}

fn candidate_results_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/stage-5/stage5f-c-r1-candidate-results.json")
}

fn load_json(path: &Path) -> Value {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", path.display()))
}

fn load_typed<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("cannot strictly parse {}: {error}", path.display()))
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap_or_else(|error| panic!("invalid fixture timestamp {value}: {error}"))
        .with_timezone(&Utc)
}

fn parse_time(value: &str) -> NaiveTime {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .unwrap_or_else(|error| panic!("invalid fixture time {value}: {error}"))
}

fn parse_f64(value: &str) -> f64 {
    let parsed = value
        .parse::<f64>()
        .unwrap_or_else(|error| panic!("invalid fixture decimal {value}: {error}"));
    assert!(parsed.is_finite(), "fixture decimal must be finite");
    parsed
}

fn sha256_path(path: &Path) -> String {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot hash {}: {error}", path.display()));
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_v1_binding(binding: &Stage5fFixtureSourceV1, path: &str, sha256: &str) {
    assert_eq!(binding.path, path);
    assert_eq!(binding.sha256, sha256);
    assert!(!binding.status.is_empty());
}

fn validate_catalog_target(target: &Stage5fCatalogTarget) {
    assert_eq!(target.strategy_id, "hybrid_imoexf");
    assert_eq!(target.account_id, "ACC_TEST_0001");
    assert_eq!(target.profile, "imoexf_primary_riskgate_high180_lb120");
    validate_instrument_fixture(&target.instrument);
}

fn validate_instrument_fixture(instrument: &Stage5fInstrumentFixture) {
    assert_eq!(instrument.symbol, "IMOEXF");
    assert_eq!(instrument.venue_symbol, "IMOEXF@RTSX");
    assert_eq!(instrument.exchange, "Moex");
    assert_eq!(instrument.market, "Futures");
}

fn cycle_value(cycle: &Stage5fCycleIdentityV2) -> String {
    assert!(cycle.sequence <= u16::from(u8::MAX));
    let created = parse_utc(&cycle.created_ts_utc);
    let expected = format!(
        "{:08x}{:02x}",
        (created.timestamp().max(0) as u64) & 0xffff_ffff,
        cycle.sequence & 0xff
    );
    assert_eq!(
        cycle.value.len(),
        10,
        "source cycle id must be exactly 10 bytes"
    );
    assert!(
        cycle.value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "source cycle id must be production-valid ASCII hex"
    );
    assert_eq!(
        cycle.value, expected,
        "source-produced cycle identity drift"
    );
    expected
}

fn validate_state_seed(defaults: &Stage5fStateDefaultsV2, seed: &Stage5fStateSeedV2) {
    let position = parse_f64(&seed.position_qty);
    let last_processed = parse_utc(&seed.last_processed_bar_ts_utc);
    let expected_day = NaiveDate::parse_from_str(&defaults.last_day_local, "%Y-%m-%d")
        .expect("state default local day");
    let mut cycles = Vec::new();
    if let Some(cycle) = &seed.active_cycle {
        let _ = cycle_value(cycle);
        assert_eq!(
            (parse_utc(&cycle.created_ts_utc) + Duration::hours(3)).date_naive(),
            expected_day,
            "active cycle local day drift"
        );
        assert!(parse_utc(&cycle.created_ts_utc) <= last_processed + Duration::seconds(1));
        cycles.push(cycle);
    }
    for cycle in [
        seed.pending_entry.as_ref().map(|value| &value.cycle),
        seed.pending_exit.as_ref().map(|value| &value.cycle),
        seed.deferred_entry.as_ref().map(|value| &value.cycle),
        seed.deferred_exit.as_ref().map(|value| &value.cycle),
    ]
    .into_iter()
    .flatten()
    {
        let _ = cycle_value(cycle);
        assert_eq!(
            (parse_utc(&cycle.created_ts_utc) + Duration::hours(3)).date_naive(),
            expected_day,
            "nested cycle local day drift"
        );
        cycles.push(cycle);
    }
    assert!(cycles
        .iter()
        .all(|cycle| { u32::from(cycle.sequence) < defaults.next_cycle_seq }));

    if let Some(entry) = &seed.pending_entry {
        assert_eq!(seed.active_cycle.as_ref(), Some(&entry.cycle));
        assert_eq!(seed.current_owner, None);
        assert_eq!(seed.current_side, None);
        assert!(parse_utc(&entry.cycle.created_ts_utc) <= parse_utc(&entry.created_ts_utc));
        assert_eq!(entry.entry_style, EntryStyle::Bracket);
        assert!(parse_f64(&entry.target_qty) > 0.0);
        assert!(entry.stop_price.as_deref().map(parse_f64).is_some());
        assert!(entry.take_price.as_deref().map(parse_f64).is_some());
        let _ = request_id(&entry.request_id);
        let _ = entry.reason;
    }
    if let Some(exit) = &seed.pending_exit {
        assert_eq!(seed.active_cycle.as_ref(), Some(&exit.cycle));
        assert_eq!(seed.current_owner, Some(exit.owner));
        assert_eq!(seed.current_side, Some(exit.side));
        assert!(parse_utc(&exit.cycle.created_ts_utc) <= parse_utc(&exit.created_ts_utc));
        let _ = request_id(&exit.request_id);
        let _ = exit.reason;
    }
    if let Some(entry) = &seed.deferred_entry {
        assert!(seed.active_cycle.is_none());
        assert!(parse_utc(&entry.cycle.created_ts_utc) <= parse_utc(&entry.deferred_ts_utc));
        assert_eq!(entry.entry_style, EntryStyle::Bracket);
        assert!(entry.stop_price.as_deref().map(parse_f64).is_some());
        assert!(entry.take_price.as_deref().map(parse_f64).is_some());
        let _ = request_id(&entry.original_request_id);
        let _ = (entry.owner, entry.side, entry.reason);
    }
    if let Some(exit) = &seed.deferred_exit {
        assert_eq!(seed.active_cycle.as_ref(), Some(&exit.cycle));
        assert!(parse_utc(&exit.cycle.created_ts_utc) <= parse_utc(&exit.deferred_ts_utc));
        let _ = request_id(&exit.original_request_id);
        let _ = (exit.owner, exit.reason);
    }

    match seed.state_class {
        Stage5fStateClass::Flat | Stage5fStateClass::FlatAfterTerminalCleanup => {
            assert_eq!(position, 0.0);
            assert_eq!(seed.orchestrator_state, Stage5fOrchestratorState::Flat);
        }
        Stage5fStateClass::OpenPosition => {
            assert_ne!(position, 0.0);
            assert_eq!(seed.orchestrator_state, Stage5fOrchestratorState::Open);
        }
        Stage5fStateClass::PendingEntry => {
            assert_eq!(position, 0.0);
            assert_eq!(seed.orchestrator_state, Stage5fOrchestratorState::Pending);
            assert!(seed.pending_entry.is_some());
        }
        Stage5fStateClass::PendingExit => {
            assert_ne!(position, 0.0);
            assert_eq!(seed.orchestrator_state, Stage5fOrchestratorState::Open);
            assert!(seed.pending_exit.is_some());
        }
        Stage5fStateClass::DeferredEntry => {
            assert_eq!(position, 0.0);
            assert!(seed.deferred_entry.is_some());
        }
        Stage5fStateClass::DeferredExit => {
            assert_ne!(position, 0.0);
            assert!(seed.deferred_exit.is_some());
        }
    }
    assert!(!(seed.was_long_today && seed.was_short_today));
    if let Some(date) = &seed.overnight_exit_armed_date {
        let _ = NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("overnight armed day");
    }
}

fn load_scenario(row_id: &str) -> Stage5fScenarioInput {
    let root = fixture_root();
    let scenarios_path = root.join("scenarios/atomic-hybrid-scenarios.json");
    let states_path = root.join("states/imoexf-hybrid-state-seeds.json");
    let riskgates_path = root.join("riskgate/imoexf-high180-riskgate-seeds.json");
    let config_path = root.join("config/imoexf-target-config.json");
    let scenarios: Stage5fScenarioCatalogV2 = load_typed(&scenarios_path);
    let states: Stage5fStateCatalogV2 = load_typed(&states_path);
    let riskgates: Stage5fRiskgateCatalogV2 = load_typed(&riskgates_path);

    assert_eq!(scenarios.schema_version, 2);
    assert_eq!(
        scenarios.fixture_kind,
        "stage5f-atomic-hybrid-scenario-catalog-v2"
    );
    assert_eq!(scenarios.status, "canonical_r2_non_golden");
    validate_v1_binding(
        &scenarios.source_v1,
        "tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json",
        "e83f10b58ba6c72efbf95d561edc9f7de84ce8e092129f6a9b449d2683e84184",
    );
    assert_eq!(
        scenarios.characterization_policy.current_status,
        "canonical_v2_pending_source_characterization"
    );
    assert!(
        !scenarios
            .characterization_policy
            .pending_outputs_are_acceptance_evidence
    );
    assert!(
        scenarios
            .characterization_policy
            .source_callback_allowed_for_existing_seven_rows_only
    );
    assert!(
        scenarios
            .characterization_policy
            .candidate_outputs_require_separate_freeze
    );
    assert!(!scenarios.characterization_policy.correction_overlay_allowed);
    assert_eq!(
        scenarios.target_config.path,
        "tests/fixtures/stage5/stage5f/v2/config/imoexf-target-config.json"
    );
    assert_eq!(scenarios.target_config.sha256, sha256_path(&config_path));
    assert_eq!(
        scenarios.clock_ownership.event_ts_utc,
        "broker_neutral_event_context_and_bar_close"
    );
    assert_eq!(
        scenarios.clock_ownership.callback_ts_utc,
        "stage5e_callback_authority_issue_and_invoke"
    );
    assert_eq!(
        scenarios.clock_ownership.lifecycle_ts_utc,
        "stage5c_admission_schedule_and_recovery"
    );

    assert_eq!(states.schema_version, 2);
    assert_eq!(states.fixture_kind, "stage5f-hybrid-state-seed-catalog-v2");
    validate_v1_binding(
        &states.source_v1,
        "tests/fixtures/stage5/stage5f/v1/states/imoexf-hybrid-state-seeds.json",
        "bb732fcebc0da78d3acdc88a3ceeb3db11a6a5a0719a92aeb91bcdcaf11729b4",
    );
    validate_catalog_target(&states.target);
    assert_eq!(riskgates.schema_version, 2);
    assert_eq!(riskgates.fixture_kind, "stage5f-riskgate-seed-catalog-v2");
    validate_v1_binding(
        &riskgates.source_v1,
        "tests/fixtures/stage5/stage5f/v1/riskgate/imoexf-high180-riskgate-seeds.json",
        "20e95ace0c1d92746c2198083d6b73fd0e78e1e58bc0b9b4bbcebf696fb5a1fc",
    );
    assert!(riskgates.target.is_none());

    let raw = scenarios
        .records
        .into_iter()
        .find(|record| record.row_id == row_id)
        .unwrap_or_else(|| panic!("scenario row not found: {row_id}"));
    assert_eq!(raw.schema_version, 2);
    assert!(!raw.group_id.is_empty() && !raw.case_id.is_empty());
    assert_eq!(raw.owning_test, raw.scenario_id);
    assert!(raw
        .broker_truth
        .working_order_ids
        .iter()
        .all(|order_id| !order_id.trim().is_empty()));
    let unique_working_order_ids = raw
        .broker_truth
        .working_order_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        unique_working_order_ids.len(),
        raw.broker_truth.working_order_ids.len()
    );
    assert_eq!(raw.target.strategy_id, "hybrid_imoexf");
    assert_eq!(raw.target.account_id, "ACC_TEST_0001");
    assert_eq!(raw.target.profile, "imoexf_primary_riskgate_high180_lb120");
    assert!(raw.target.paper_only);
    validate_instrument_fixture(&raw.target.instrument);
    assert_eq!(raw.bar.origin, "Live");
    assert!(raw.bar.is_final);
    assert_eq!(raw.bar.timeframe_sec, 600);
    assert_eq!(
        raw.expected.characterization_status,
        "canonical_v2_pending_source_characterization"
    );
    assert!(raw.expected.callback_count <= 1);
    assert!(raw.expected.settlement_attempt_count <= 1);
    assert!(raw.expected.pre_state_fingerprint.is_none());
    assert!(raw.expected.accepted_post_state_fingerprint.is_none());
    assert!(raw.expected.ordered_intent_vector.is_none());
    assert!(raw.expected.ordered_intent_vector_sha256.is_none());
    assert!(!raw.expected.disposition.is_empty() && !raw.expected.b3f_outcome.is_empty());

    assert_eq!(
        raw.pre_state.catalog_path,
        "tests/fixtures/stage5/stage5f/v2/states/imoexf-hybrid-state-seeds.json"
    );
    assert_eq!(raw.pre_state.catalog_sha256, sha256_path(&states_path));
    assert_eq!(raw.riskgate.catalog_sha256, sha256_path(&riskgates_path));
    assert_eq!(
        raw.riskgate.catalog_path,
        "tests/fixtures/stage5/stage5f/v2/riskgate/imoexf-high180-riskgate-seeds.json"
    );

    let state_seed = states
        .seeds
        .into_iter()
        .find(|seed| seed.seed_id == raw.pre_state.seed_id)
        .unwrap_or_else(|| panic!("fixture state seed not found: {}", raw.pre_state.seed_id));
    validate_state_seed(&states.seed_defaults, &state_seed);
    let riskgate_seed = riskgates
        .seeds
        .into_iter()
        .find(|seed| seed.seed_id == raw.riskgate.seed_id)
        .unwrap_or_else(|| panic!("fixture riskgate seed not found: {}", raw.riskgate.seed_id));
    assert_eq!(riskgate_seed.risk_gate_mode, "normal_append");
    assert_eq!(riskgate_seed.mr_gate_policy, "shadow_pnl_lb120_positive");
    assert!(!riskgate_seed.enforced_for_entry);
    assert!(!riskgate_seed.expected_pre_callback_disposition.is_empty());

    let event_at = parse_utc(&raw.clock.event_ts_utc);
    let lifecycle_at = parse_utc(&raw.clock.lifecycle_ts_utc);
    let callback_at = parse_utc(&raw.clock.callback_ts_utc);
    assert!(event_at <= lifecycle_at && lifecycle_at <= callback_at);
    assert!(callback_at - event_at <= Duration::seconds(10));
    let target = InstrumentId {
        symbol: raw.target.instrument.symbol,
        venue_symbol: Some(raw.target.instrument.venue_symbol),
        exchange: Exchange::Moex,
        market: Market::Futures,
    };
    let bar = broker_core::HybridRuntimeBarEvent {
        instrument: target.clone(),
        close_time_utc: parse_utc(&raw.bar.close_time_utc).timestamp(),
        open: parse_f64(&raw.bar.open),
        high: parse_f64(&raw.bar.high),
        low: parse_f64(&raw.bar.low),
        close: parse_f64(&raw.bar.close),
        volume: parse_f64(&raw.bar.volume),
        origin: broker_core::HybridRuntimeBarOrigin::Live,
        is_final: raw.bar.is_final,
        timeframe_sec: raw.bar.timeframe_sec,
    };
    assert_eq!(event_at.timestamp(), bar.close_time_utc);
    let position_qty = parse_f64(&state_seed.position_qty);

    Stage5fScenarioInput {
        state_defaults: states.seed_defaults,
        state_seed,
        riskgate_seed,
        row_id: raw.row_id,
        scenario_id: raw.scenario_id,
        event_at,
        callback_at,
        lifecycle_at,
        bar,
        strategy_id: raw.target.strategy_id,
        account_id: BrokerAccountId::new(raw.target.account_id),
        target,
        position_qty,
    }
}

fn stage5f_config() -> HybridIntradayRuntimeConfig {
    load_typed::<Stage5fTargetConfigV1>(&fixture_root().join("config/imoexf-target-config.json"))
        .into_runtime_config()
}

fn request_id(value: &str) -> StrategyRequestId {
    StrategyRequestId::from(
        uuid::Uuid::parse_str(value)
            .unwrap_or_else(|error| panic!("invalid fixture request id {value}: {error}")),
    )
}

fn stage5d_owner(value: Owner) -> crate::stage5d_persistence::Stage5dOwner {
    match value {
        Owner::MeanReversion => crate::stage5d_persistence::Stage5dOwner::MeanReversion,
        Owner::IntradayBreakout => crate::stage5d_persistence::Stage5dOwner::IntradayBreakout,
    }
}

fn stage5d_side(value: Side) -> crate::stage5d_persistence::Stage5dSide {
    match value {
        Side::Long => crate::stage5d_persistence::Stage5dSide::Long,
        Side::Short => crate::stage5d_persistence::Stage5dSide::Short,
    }
}

fn stage5d_entry_style(value: EntryStyle) -> crate::stage5d_persistence::Stage5dEntryStyle {
    match value {
        EntryStyle::Bracket => crate::stage5d_persistence::Stage5dEntryStyle::Bracket,
        EntryStyle::Market => crate::stage5d_persistence::Stage5dEntryStyle::Market,
    }
}

fn stage5d_reason(value: ReasonCode) -> crate::stage5d_persistence::Stage5dLifecycleReason {
    use crate::stage5d_persistence::Stage5dLifecycleReason as Target;
    match value {
        ReasonCode::MorningMeanReversionLong => Target::MorningMeanReversionLong,
        ReasonCode::MorningMeanReversionShort => Target::MorningMeanReversionShort,
        ReasonCode::BreakoutLong => Target::BreakoutLong,
        ReasonCode::BreakoutShort => Target::BreakoutShort,
        ReasonCode::BreakoutEodExit => Target::BreakoutEodExit,
        ReasonCode::BreakoutStop2Long => Target::BreakoutStop2Long,
        ReasonCode::BreakoutStop1Long => Target::BreakoutStop1Long,
        ReasonCode::BreakoutStop2Short => Target::BreakoutStop2Short,
        ReasonCode::BreakoutStop1Short => Target::BreakoutStop1Short,
        ReasonCode::MeanRevTimeCutoff => Target::MeanRevTimeCutoff,
        ReasonCode::WaitfixOvernightExit => Target::WaitfixOvernightExit,
    }
}

fn materialize_strategy(input: &Stage5fScenarioInput) -> HybridIntradayRuntimeStrategy {
    use crate::runtime_compat::StrategyState;
    let defaults = &input.state_defaults;
    let seed = &input.state_seed;
    let state = StrategyState::HybridIntradayRuntime {
        active_cycle_id: seed.active_cycle.as_ref().map(cycle_value),
        next_cycle_seq: defaults.next_cycle_seq,
        last_position_qty: input.position_qty,
        current_owner: seed.current_owner,
        current_side: seed.current_side,
        pending_entry_owner: seed.pending_entry.as_ref().map(|value| value.owner),
        pending_entry_side: seed.pending_entry.as_ref().map(|value| value.side),
        pending_entry_cycle_id: seed
            .pending_entry
            .as_ref()
            .map(|value| cycle_value(&value.cycle)),
        pending_entry_request_id: seed
            .pending_entry
            .as_ref()
            .map(|value| request_id(&value.request_id)),
        pending_entry_created_ts_utc: seed
            .pending_entry
            .as_ref()
            .map(|value| parse_utc(&value.created_ts_utc).timestamp()),
        deferred_entry_owner: seed.deferred_entry.as_ref().map(|value| value.owner),
        deferred_entry_side: seed.deferred_entry.as_ref().map(|value| value.side),
        deferred_entry_cycle_id: seed
            .deferred_entry
            .as_ref()
            .map(|value| cycle_value(&value.cycle)),
        deferred_entry_entry_style: seed.deferred_entry.as_ref().map(|value| value.entry_style),
        deferred_entry_reason: seed.deferred_entry.as_ref().map(|value| value.reason),
        deferred_entry_stop_price: seed
            .deferred_entry
            .as_ref()
            .and_then(|value| value.stop_price.as_deref().map(parse_f64)),
        deferred_entry_take_price: seed
            .deferred_entry
            .as_ref()
            .and_then(|value| value.take_price.as_deref().map(parse_f64)),
        deferred_entry_ts_utc: seed
            .deferred_entry
            .as_ref()
            .map(|value| parse_utc(&value.deferred_ts_utc).timestamp()),
        deferred_entry_request_id: seed
            .deferred_entry
            .as_ref()
            .map(|value| request_id(&value.original_request_id)),
        pending_exit_request_id: seed
            .pending_exit
            .as_ref()
            .map(|value| request_id(&value.request_id)),
        pending_exit_created_ts_utc: seed
            .pending_exit
            .as_ref()
            .map(|value| parse_utc(&value.created_ts_utc).timestamp()),
        deferred_exit_owner: seed.deferred_exit.as_ref().map(|value| value.owner),
        deferred_exit_reason: seed.deferred_exit.as_ref().map(|value| value.reason),
        deferred_exit_cycle_id: seed
            .deferred_exit
            .as_ref()
            .map(|value| cycle_value(&value.cycle)),
        deferred_exit_ts_utc: seed
            .deferred_exit
            .as_ref()
            .map(|value| parse_utc(&value.deferred_ts_utc).timestamp()),
        deferred_exit_request_id: seed
            .deferred_exit
            .as_ref()
            .map(|value| request_id(&value.original_request_id)),
        pending_tp_request_id: None,
        pending_tp_created_ts_utc: None,
        pending_sl_request_id: None,
        pending_sl_created_ts_utc: None,
        tp_order_id: None,
        sl_stop_order_id: None,
        sl_exchange_order_id: None,
        sl_triggered_ts: None,
        mr_take_price: None,
        mr_stop_price: None,
        repair_deadline_ts: None,
        next_repair_at_ts: None,
        repair_backoff_level: 0,
        repair_attempts: 0,
        safe_mode_close_only: defaults.safe_mode_close_only,
        safe_mode_reason: None,
        entry_ready: defaults.entry_ready,
        last_bar_close: Some(parse_f64(&defaults.last_bar_close)),
        prev_day_close: Some(parse_f64(&defaults.prev_day_close)),
        last_day_local: Some(defaults.last_day_local.clone()),
        current_day_high: Some(parse_f64(&defaults.current_day_high)),
        current_day_low: Some(parse_f64(&defaults.current_day_low)),
        current_day_close: Some(parse_f64(&defaults.current_day_close)),
        prev_day_range: Some(parse_f64(&defaults.prev_day_range)),
        prev_day_return: Some(parse_f64(&defaults.prev_day_return)),
        day_before_close: Some(parse_f64(&defaults.day_before_close)),
        today_start_local: Some(defaults.today_start_local.clone()),
        was_long_today: seed.was_long_today,
        was_short_today: seed.was_short_today,
        overnight_exit_armed_date: seed.overnight_exit_armed_date.clone(),
        risk_gate_shadow_session_date: Some(
            defaults
                .riskgate_semantic_state
                .current_shadow_session_date
                .clone(),
        ),
        risk_gate_shadow_pnl_points: parse_f64(
            &defaults.riskgate_semantic_state.current_shadow_pnl_points,
        ),
        risk_gate_shadow_trade_count: defaults.riskgate_semantic_state.current_shadow_trade_count,
        risk_gate_shadow_entry_ts_utc: None,
        risk_gate_shadow_entry_price: None,
        risk_gate_shadow_side: None,
        risk_gate_shadow_target_price: None,
        risk_gate_shadow_stop_price: None,
        risk_gate_pending_session_date: None,
        risk_gate_pending_shadow_pnl_points: 0.0,
        risk_gate_pending_shadow_trade_count: 0,
        risk_gate_mr_enabled_current_session: Some(
            defaults.riskgate_semantic_state.mr_enabled_current_session,
        ),
        risk_gate_rolling_sum_lb120: Some(parse_f64(
            &defaults.riskgate_semantic_state.rolling_sum_lb120,
        )),
        risk_gate_last_finalized_session_date: Some(
            defaults
                .riskgate_semantic_state
                .last_finalized_session_date
                .clone(),
        ),
        risk_gate_ledger_rows_count: defaults.riskgate_semantic_state.ledger_rows_count,
    };
    let expected_state = state.clone();
    let mut strategy = HybridIntradayRuntimeStrategy::new(stage5f_config());
    Strategy::set_state(&mut strategy, state);

    assert!(defaults.private_state.expected_working_order_ids.is_empty());
    assert!(defaults
        .private_state
        .expected_working_stop_order_ids
        .is_empty());
    assert!(defaults
        .private_state
        .runtime_pending_finalizations
        .is_empty());
    let private = crate::stage5d_persistence::Stage5dRuntimePrivateExtension {
        schema_version:
            crate::stage5d_persistence::STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION,
        pending_entry: seed.pending_entry.as_ref().map(|entry| {
            crate::stage5d_persistence::Stage5dPendingEntryExtension {
                owner: stage5d_owner(entry.owner),
                side: stage5d_side(entry.side),
                reason: stage5d_reason(entry.reason),
                entry_style: stage5d_entry_style(entry.entry_style),
                target_qty: entry.target_qty.clone(),
                stop_price: entry.stop_price.clone(),
                take_price: entry.take_price.clone(),
                request_id: Some(request_id(&entry.request_id)),
            }
        }),
        partial_entry_timer: None,
        pending_exit: seed.pending_exit.as_ref().map(|exit| {
            crate::stage5d_persistence::Stage5dPendingExitExtension {
                owner: stage5d_owner(exit.owner),
                reason: stage5d_reason(exit.reason),
                request_id: request_id(&exit.request_id),
            }
        }),
        bracket_reconciliation_timer: None,
        cleanup_retry_state: Some(crate::stage5d_persistence::Stage5dCleanupRetryState {
            cleanup_stop_retry_attempts: defaults.private_state.cleanup_stop_retry_attempts,
        }),
        expected_working_sets: crate::stage5d_persistence::Stage5dExpectedWorkingSets {
            expected_working_order_ids: Vec::new(),
            expected_working_stop_order_ids: Vec::new(),
        },
        last_processed_bar_ts: Some(parse_utc(&seed.last_processed_bar_ts_utc)),
        runtime_pending_finalizations: Vec::new(),
    };
    strategy
        .stage5d_apply_runtime_private_extension(&private)
        .expect("Stage 5F complete private extension must apply exactly");
    assert_eq!(
        serde_json::to_value(Strategy::state(&strategy)).expect("runtime state serializes"),
        serde_json::to_value(expected_state).expect("fixture state serializes"),
        "Stage 5F typed materializer changed public runtime state"
    );
    assert_eq!(
        strategy
            .stage5d_export_runtime_private_extension()
            .expect("runtime private extension exports"),
        private,
        "Stage 5F typed materializer changed private runtime state"
    );
    strategy
}

enum Stage5fRiskgatePreparation {
    Ready,
    Blocked(crate::stage5d_persistence::Stage5dRiskGateInjectionBlockReason),
}

fn invalid_riskgate_evidence(
    authority_state: &str,
) -> crate::stage5d_persistence::Stage5dRiskGateLedgerEvidence {
    use crate::stage5d_persistence::{Stage5dRiskGateIdentity, Stage5dRiskGateLedgerEvidence};
    let (schema_version, generation, tail) = match authority_state {
        "missing" => (
            0,
            crate::hybrid_intraday::RISK_GATE_STATE_GENERATION,
            "missing",
        ),
        "inconsistent" => (
            crate::stage5d_persistence::STAGE5D_RISKGATE_SCHEMA_VERSION,
            "wrong-generation",
            "inconsistent",
        ),
        "materialization_integrity_mismatch" => (
            crate::stage5d_persistence::STAGE5D_RISKGATE_SCHEMA_VERSION,
            crate::hybrid_intraday::RISK_GATE_STATE_GENERATION,
            "wrong-tail",
        ),
        other => panic!("unknown blocked riskgate authority state: {other}"),
    };
    Stage5dRiskGateLedgerEvidence {
        schema_version,
        identity: Stage5dRiskGateIdentity {
            strategy_id: "hybrid_imoexf".to_string(),
            profile_id: "imoexf_primary_high180_lb120".to_string(),
            mr_variant: "high180".to_string(),
            timeframe: "10m".to_string(),
            session_policy: "moex_forts_main_evening".to_string(),
            model_version: crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID
                .to_string(),
        },
        ledger_tail_hash: tail.to_string(),
        ledger_records: Vec::new(),
        seed_loaded: false,
        current_shadow_session_date: None,
        current_shadow_pnl_points: "0.0".to_string(),
        current_generation: generation.to_string(),
    }
}

fn prepare_riskgate(
    strategy: &mut HybridIntradayRuntimeStrategy,
    input: &Stage5fScenarioInput,
) -> Stage5fRiskgatePreparation {
    let seed = &input.riskgate_seed;
    match seed.authority_state.as_str() {
        "complete" => {
            Strategy::on_risk_gate_state(
                strategy,
                &RiskGateRuntimeState {
                    profile_id: "imoexf_primary_high180_lb120".to_string(),
                    last_finalized_session_date: Some(
                        NaiveDate::parse_from_str(
                            seed.last_finalized_session_date
                                .as_deref()
                                .expect("riskgate date"),
                            "%Y-%m-%d",
                        )
                        .expect("riskgate session date"),
                    ),
                    rolling_sum_lb120: Some(parse_f64(
                        seed.rolling_sum_lb120
                            .as_deref()
                            .expect("riskgate rolling sum"),
                    )),
                    mr_enabled_current_session: seed.mr_enabled_current_session,
                    mr_enabled_next_session: seed.mr_enabled_next_session,
                    ledger_rows_count: seed.ledger_rows_count,
                },
            );
            Stage5fRiskgatePreparation::Ready
        }
        blocked => {
            let reason = crate::stage5d_persistence::stage5d_validate_riskgate_ledger_evidence(
                invalid_riskgate_evidence(blocked),
            )
            .err()
            .expect("blocked fixture must produce a typed Stage 5D reason");
            Stage5fRiskgatePreparation::Blocked(reason)
        }
    }
}

fn stage5f_state_fingerprint(strategy: &HybridIntradayRuntimeStrategy) -> String {
    crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(strategy)
}

fn settled_projection(
    observed: Stage5fObservedIntentVector,
    request_ids: &[StrategyRequestId],
) -> (Vec<Value>, String) {
    assert_eq!(
        observed.projections.len(),
        request_ids.len(),
        "observer/settlement intent cardinality drift"
    );
    let vector = observed
        .projections
        .into_iter()
        .zip(request_ids)
        .map(|(projection, request_id)| {
            json!({
                "ordinal": projection.ordinal,
                "settled_strategy_request_id": request_id.to_string(),
                "intent_class": projection.intent_class,
                "base_action": projection.base_action,
                "route_symbol": projection.route_symbol,
                "owner": projection.owner,
                "role": projection.role,
                "side": projection.side,
                "cycle_id_domain_sha256": projection.cycle_id_domain_sha256,
                "quantity_f64_bits_be": projection.quantity_f64_bits_be,
                "price_f64_bits_be": projection.price_f64_bits_be,
                "trigger_f64_bits_be": projection.trigger_f64_bits_be,
                "fill_f64_bits_be": projection.fill_f64_bits_be,
                "stop_end_unix_time": projection.stop_end_unix_time,
                "broker_order_id_domain_sha256": projection.broker_order_id_domain_sha256,
                "broker_stop_id_domain_sha256": projection.broker_stop_id_domain_sha256,
                "comment_present": projection.comment_present,
                "comment_domain_sha256": projection.comment_domain_sha256,
                "check_duplicates": projection.check_duplicates,
                "condition_flags": projection.condition_flags,
            })
        })
        .collect::<Vec<_>>();
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage5f.ordered-intent-vector.v1\0");
    hasher.update(serde_json::to_vec(&vector).expect("projection serializes"));
    let hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (vector, hash)
}

#[derive(Clone, Copy)]
enum Stage5fMutation {
    None,
    CallbackValidation,
    B3fChronology,
    Stage5cDuplicateIntent,
}

enum Stage5fSettlementOutcome {
    Settled(
        crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5eValidatedPaperSettlementReceipt,
    ),
    Terminal(
        crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementTerminalReceipt,
    ),
}

struct Stage5fCallbackBoundaryResult {
    callback_count: usize,
    observed: Option<Stage5fObservedIntentVector>,
    accepted_post_state_fingerprint: String,
    settlement: Stage5fSettlementOutcome,
}

/// The only Stage 5F callback/settlement call site. Both the seven-row
/// characterization and the representative full-restart oracle must cross
/// this boundary so static cardinality checks cover the complete harness.
fn invoke_and_settle_stage5f_callback(
    row_id: &str,
    authority: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eCallbackAuthorityReadyPaperStrategy,
    callback_at: DateTime<Utc>,
    mutation: Stage5fMutation,
) -> Stage5fCallbackBoundaryResult {
    use crate::stage5e_no_io_lifecycle::callback_authority::{
        callback_settlement::validate_and_settle_stage5e_paper_callback_escrow,
        invoke_stage5e_authorized_paper_callback_at,
    };

    let scope = arm_observer_once();
    crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
    let mut escrow = invoke_stage5e_authorized_paper_callback_at(authority, callback_at)
        .unwrap_or_else(|_| panic!("{row_id}: callback must reach opaque escrow"));
    let callback_count = crate::stage5c_paper_host::stage5e_test_b3e_callback_count();
    assert_eq!(callback_count, 1, "{row_id}: exact callback cardinality");
    let observed = scope.consume_once();
    let accepted_post_state_fingerprint = escrow.test_strategy_state_fingerprint();
    if matches!(mutation, Stage5fMutation::B3fChronology) {
        escrow.test_set_callback_before_retained_close();
    }
    if matches!(mutation, Stage5fMutation::Stage5cDuplicateIntent) {
        assert_eq!(
            escrow.test_intent_count(),
            1,
            "negative row needs one source intent"
        );
        escrow.test_repeat_first_ok_intent(2);
    }
    let settlement = match validate_and_settle_stage5e_paper_callback_escrow(escrow) {
        Ok(receipt) => Stage5fSettlementOutcome::Settled(receipt),
        Err(terminal) => Stage5fSettlementOutcome::Terminal(terminal),
    };
    Stage5fCallbackBoundaryResult {
        callback_count,
        observed,
        accepted_post_state_fingerprint,
        settlement,
    }
}

fn characterize(row_id: &str, mutation: Stage5fMutation) -> Stage5fCandidateResult {
    use crate::stage5e_no_io_lifecycle::callback_authority::issue_stage5e_callback_authority_at;

    let input = load_scenario(row_id);
    let mut strategy = materialize_strategy(&input);
    let pre_state_fingerprint = stage5f_state_fingerprint(&strategy);
    if let Stage5fRiskgatePreparation::Blocked(reason) = prepare_riskgate(&mut strategy, &input) {
        return Stage5fCandidateResult {
            schema_version: 2,
            row_id: input.row_id,
            scenario_id: input.scenario_id,
            disposition: "blocked_before_callback",
            callback_count: 0,
            observer_count: 0,
            settlement_attempt_count: 0,
            pre_state_fingerprint,
            accepted_post_state_fingerprint: None,
            ordered_intent_vector: Vec::new(),
            ordered_intent_vector_sha256: None,
            b3f_outcome: format!("{reason:?}"),
            settlement_identity_sha256: None,
        };
    }

    let position_qty =
        Decimal::from_str(&input.state_seed.position_qty).expect("position quantity decimal");
    let predecessor_close_ts = input.event_at.timestamp() - 600;
    let bar_close_ts = input.bar.close_time_utc;
    let (recovered, accepted) =
        crate::stage5c_paper_host::stage5f_test_seams::sequence_inputs_from_owned_strategy(
            strategy,
            input.strategy_id.clone(),
            input.account_id.clone(),
            input.target.clone(),
            0.5,
            position_qty,
            input.lifecycle_at,
            predecessor_close_ts,
            input.bar.clone(),
        );
    let mut b3c = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
        stage5f_test_seams::b3c_from_sequence_inputs(
            recovered,
            accepted,
            input.target,
            bar_close_ts,
            input.lifecycle_at,
        );
    if matches!(mutation, Stage5fMutation::CallbackValidation) {
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence::
            stage5f_test_seams::force_callback_validation_error(&mut b3c);
    }
    let authority = issue_stage5e_callback_authority_at(b3c, input.callback_at)
        .unwrap_or_else(|_| panic!("{row_id}: B3C must issue callback authority"));
    let authority_pre_fingerprint = authority.test_ownership_fingerprint().0;
    assert_eq!(
        authority_pre_fingerprint, pre_state_fingerprint,
        "{row_id}: state changed before callback"
    );
    let boundary =
        invoke_and_settle_stage5f_callback(row_id, authority, input.callback_at, mutation);
    let callback_count = boundary.callback_count;
    let observed = boundary.observed;
    let observer_count = usize::from(observed.is_some());
    let accepted_post_state_fingerprint = Some(boundary.accepted_post_state_fingerprint);
    match boundary.settlement {
        Stage5fSettlementOutcome::Settled(receipt) => {
            let (request_ids, intent_count, history_len, canonical, state_fingerprint) =
                receipt.test_identity_proof_shape();
            assert_eq!(history_len, 1);
            assert!(canonical);
            assert_eq!(
                Some(state_fingerprint),
                accepted_post_state_fingerprint,
                "{row_id}: post-state settlement binding"
            );
            let observed = observed.expect("accepted callback must reach exact on_bar observer");
            assert_eq!(observed.len(), intent_count);
            let (vector, vector_hash) = settled_projection(observed, &request_ids);
            Stage5fCandidateResult {
                schema_version: 2,
                row_id: input.row_id,
                scenario_id: input.scenario_id,
                disposition: "accepted",
                callback_count,
                observer_count,
                settlement_attempt_count: 1,
                pre_state_fingerprint,
                accepted_post_state_fingerprint,
                ordered_intent_vector: vector,
                ordered_intent_vector_sha256: Some(vector_hash),
                b3f_outcome: "settled".to_string(),
                settlement_identity_sha256: Some(
                    receipt
                        .test_settlement_identity()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                ),
            }
        }
        Stage5fSettlementOutcome::Terminal(terminal) => {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementTerminalReason as Reason;
            let outcome = match terminal.test_reason() {
                Reason::CallbackValidationError => "CallbackValidationError",
                Reason::IntentCapacityExceeded => "IntentCapacityExceeded",
                Reason::IdentityMismatch => "IdentityMismatch",
                Reason::ChronologyMismatch => "ChronologyMismatch",
                Reason::PaperModeMismatch => "PaperModeMismatch",
                Reason::Stage5cIntentValidationFailed => "Stage5cIntentValidationFailed",
                Reason::Stage5cPendingRequestMismatch => "Stage5cPendingRequestMismatch",
            };
            Stage5fCandidateResult {
                schema_version: 2,
                row_id: input.row_id,
                scenario_id: input.scenario_id,
                disposition: "terminal_after_callback",
                callback_count,
                observer_count,
                settlement_attempt_count: 1,
                pre_state_fingerprint,
                accepted_post_state_fingerprint: None,
                ordered_intent_vector: Vec::new(),
                ordered_intent_vector_sha256: None,
                b3f_outcome: outcome.to_string(),
                settlement_identity_sha256: None,
            }
        }
    }
}

fn project_intent(ordinal: usize, intent: &Intent) -> Stage5fIntentProjection {
    let mut current = intent;
    let mut intent_class = None;
    let mut route_symbol = None;
    loop {
        match current {
            Intent::Classified {
                intent,
                intent_class: class,
            } => {
                intent_class = Some(intent_class_name(*class));
                current = intent;
            }
            Intent::Routed { intent, symbol } => {
                route_symbol = Some(symbol.clone());
                current = intent;
            }
            _ => break,
        }
    }

    let mut projection = Stage5fIntentProjection {
        ordinal,
        intent_class,
        base_action: base_action_name(current),
        route_symbol,
        owner: None,
        role: None,
        side: None,
        cycle_id_domain_sha256: None,
        quantity_f64_bits_be: None,
        price_f64_bits_be: None,
        trigger_f64_bits_be: None,
        fill_f64_bits_be: None,
        stop_end_unix_time: None,
        broker_order_id_domain_sha256: None,
        broker_stop_id_domain_sha256: None,
        comment_present: false,
        comment_domain_sha256: None,
        check_duplicates: None,
        condition_flags: Vec::new(),
    };

    match current {
        Intent::Place {
            price,
            qty,
            side,
            comment,
        } => {
            projection.price_f64_bits_be = Some(f64_bits(*price));
            projection.quantity_f64_bits_be = Some(f64_bits(*qty));
            projection.side = Some(side_name(*side));
            project_comment(&mut projection, comment.as_deref());
        }
        Intent::Market {
            qty,
            side,
            fill_price,
            comment,
        } => {
            projection.quantity_f64_bits_be = Some(f64_bits(*qty));
            projection.side = Some(side_name(*side));
            projection.fill_f64_bits_be = fill_price.map(f64_bits);
            project_comment(&mut projection, comment.as_deref());
        }
        Intent::Cancel { order_id } => {
            projection.broker_order_id_domain_sha256 =
                Some(domain_hash(ORDER_ID_HASH_DOMAIN, order_id.as_str()));
        }
        Intent::Replace {
            order_id,
            new_price,
            new_qty,
        } => {
            projection.broker_order_id_domain_sha256 =
                Some(domain_hash(ORDER_ID_HASH_DOMAIN, order_id.as_str()));
            projection.price_f64_bits_be = Some(f64_bits(*new_price));
            projection.quantity_f64_bits_be = Some(f64_bits(*new_qty));
        }
        Intent::CreateStopLimit {
            side,
            qty,
            trigger_price,
            price,
            condition,
            stop_end_unix_time,
            comment,
            instrument_group,
            check_duplicates,
        } => {
            projection.side = Some(side_name(*side));
            projection.quantity_f64_bits_be = Some(f64_bits(*qty));
            projection.trigger_f64_bits_be = Some(f64_bits(*trigger_price));
            projection.price_f64_bits_be = Some(f64_bits(*price));
            projection.stop_end_unix_time = Some(*stop_end_unix_time);
            projection.check_duplicates = *check_duplicates;
            projection.condition_flags.push(condition_name(*condition));
            if instrument_group.is_some() {
                projection.condition_flags.push("instrument_group_present");
            }
            project_comment(&mut projection, comment.as_deref());
        }
        Intent::DeleteStopLimit {
            order_id,
            side,
            check_duplicates,
        } => {
            projection.broker_stop_id_domain_sha256 =
                Some(domain_hash(STOP_ID_HASH_DOMAIN, order_id.as_str()));
            projection.side = side.map(side_name);
            projection.check_duplicates = *check_duplicates;
        }
        Intent::Classified { .. } | Intent::Routed { .. } => {
            unreachable!("wrapper intents were unwrapped before projection")
        }
    }
    projection
}

fn project_comment(projection: &mut Stage5fIntentProjection, comment: Option<&str>) {
    let Some(comment) = comment else {
        return;
    };
    projection.comment_present = true;
    projection.comment_domain_sha256 = Some(domain_hash(COMMENT_HASH_DOMAIN, comment));
    if !comment.starts_with("HYB|") {
        return;
    }
    for component in comment.split('|').skip(1) {
        let Some((key, value)) = component.split_once('=') else {
            continue;
        };
        match key {
            "c" => {
                projection.cycle_id_domain_sha256 = Some(domain_hash(CYCLE_HASH_DOMAIN, value));
            }
            "o" => projection.owner = Some(value.to_string()),
            "r" => projection.role = Some(value.to_string()),
            _ => {}
        }
    }
}

fn domain_hash(domain: &[u8], value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn f64_bits(value: f64) -> String {
    assert!(
        value.is_finite(),
        "Stage 5F projection rejects non-finite f64"
    );
    assert_ne!(
        value.to_bits(),
        (-0.0_f64).to_bits(),
        "negative zero is not canonical"
    );
    format!("{:016x}", value.to_bits())
}

fn intent_class_name(value: IntentClass) -> &'static str {
    match value {
        IntentClass::Entry => "entry",
        IntentClass::Exit => "exit",
        IntentClass::CancelCleanup => "cancel_cleanup",
        IntentClass::ProtectiveRepair => "protective_repair",
    }
}

fn base_action_name(value: &Intent) -> &'static str {
    match value {
        Intent::Place { .. } => "place",
        Intent::Market { .. } => "market",
        Intent::Cancel { .. } => "cancel",
        Intent::Replace { .. } => "replace",
        Intent::CreateStopLimit { .. } => "create_stop_limit",
        Intent::DeleteStopLimit { .. } => "delete_stop_limit",
        Intent::Classified { .. } | Intent::Routed { .. } => "wrapper",
    }
}

fn side_name(value: OrderSide) -> &'static str {
    match value {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    }
}

fn condition_name(value: StopLimitCondition) -> &'static str {
    match value {
        StopLimitCondition::More => "more",
        StopLimitCondition::Less => "less",
        StopLimitCondition::MoreOrEqual => "more_or_equal",
        StopLimitCondition::LessOrEqual => "less_or_equal",
    }
}

fn assert_candidate(
    result: &Stage5fCandidateResult,
    disposition: &str,
    outcome: &str,
    intent_class: Option<&str>,
) {
    assert_eq!(
        result.disposition, disposition,
        "{} disposition",
        result.row_id
    );
    assert_eq!(result.b3f_outcome, outcome, "{} outcome", result.row_id);
    match disposition {
        "accepted" => {
            assert_eq!(result.callback_count, 1);
            assert_eq!(result.observer_count, 1);
            assert_eq!(result.settlement_attempt_count, 1);
            assert!(result.accepted_post_state_fingerprint.is_some());
            assert!(result.ordered_intent_vector_sha256.is_some());
            assert!(result.settlement_identity_sha256.is_some());
        }
        "blocked_before_callback" => {
            assert_eq!(result.callback_count, 0);
            assert_eq!(result.observer_count, 0);
            assert_eq!(result.settlement_attempt_count, 0);
            assert!(result.accepted_post_state_fingerprint.is_none());
            assert!(result.ordered_intent_vector.is_empty());
        }
        "terminal_after_callback" => {
            assert_eq!(result.callback_count, 1);
            assert_eq!(result.settlement_attempt_count, 1);
            assert!(result.accepted_post_state_fingerprint.is_none());
            assert!(result.settlement_identity_sha256.is_none());
        }
        other => panic!("unknown Stage 5F candidate disposition: {other}"),
    }
    if let Some(intent_class) = intent_class {
        assert_eq!(result.ordered_intent_vector.len(), 1);
        assert_eq!(
            result.ordered_intent_vector[0]["intent_class"],
            Value::String(intent_class.to_string())
        );
    }
}

#[test]
fn stage5f_v2_all_state_seeds_roundtrip_exact() {
    let path = fixture_root().join("states/imoexf-hybrid-state-seeds.json");
    let catalog: Stage5fStateCatalogV2 = load_typed(&path);
    assert_eq!(catalog.schema_version, 2);
    assert_eq!(catalog.fixture_kind, "stage5f-hybrid-state-seed-catalog-v2");
    validate_v1_binding(
        &catalog.source_v1,
        "tests/fixtures/stage5/stage5f/v1/states/imoexf-hybrid-state-seeds.json",
        "bb732fcebc0da78d3acdc88a3ceeb3db11a6a5a0719a92aeb91bcdcaf11729b4",
    );
    validate_catalog_target(&catalog.target);
    assert_eq!(catalog.seeds.len(), 14, "Stage 5F R1 requires all 14 seeds");

    let mut input = load_scenario("F01");
    input.state_defaults = catalog.seed_defaults;
    let mut unique_seed_ids = std::collections::BTreeSet::new();
    for seed in catalog.seeds {
        assert!(
            unique_seed_ids.insert(seed.seed_id.clone()),
            "duplicate Stage 5F seed id: {}",
            seed.seed_id
        );
        validate_state_seed(&input.state_defaults, &seed);
        input.position_qty = parse_f64(&seed.position_qty);
        input.state_seed = seed;

        let first = materialize_strategy(&input);
        let first_state = serde_json::to_value(Strategy::state(&first))
            .expect("first materialized state serializes");
        let first_private = first
            .stage5d_export_runtime_private_extension()
            .expect("first private state exports");
        let first_config = first.stage5d_canonical_config_fingerprint();

        let second = materialize_strategy(&input);
        assert_eq!(
            serde_json::to_value(Strategy::state(&second))
                .expect("second materialized state serializes"),
            first_state,
            "{} public state materialization is not deterministic",
            input.state_seed.seed_id
        );
        assert_eq!(
            second
                .stage5d_export_runtime_private_extension()
                .expect("second private state exports"),
            first_private,
            "{} private state materialization is not deterministic",
            input.state_seed.seed_id
        );
        assert_eq!(
            second.stage5d_canonical_config_fingerprint(),
            first_config,
            "{} config fingerprint drift",
            input.state_seed.seed_id
        );
    }
    assert_eq!(unique_seed_ids.len(), 14);
}

fn run_stage5f_full_restart_representative(
    row_id: &str,
    mode: crate::stage5d_persistence::stage5f_test_seams::Stage5fRepresentativeRiskgateMode,
) -> crate::stage5d_persistence::stage5f_test_seams::Stage5fFullRestartOutcome {
    let input = load_scenario(row_id);
    let source = materialize_strategy(&input);
    let fast_state =
        serde_json::to_value(Strategy::state(&source)).expect("Stage 5F fast state serializes");
    let fast_private = source
        .stage5d_export_runtime_private_extension()
        .expect("Stage 5F fast private state exports");
    let fast_config_fingerprint = source.stage5d_canonical_config_fingerprint();
    let persisted_at = input.event_at - Duration::seconds(3);
    let outcome = crate::stage5d_persistence::stage5f_test_seams::run_full_restart_oracle(
        source,
        HybridIntradayRuntimeStrategy::new(stage5f_config()),
        input.strategy_id.clone(),
        input.account_id.clone(),
        input.target.clone(),
        persisted_at,
        mode,
    );
    let projection = match &outcome {
        crate::stage5d_persistence::stage5f_test_seams::Stage5fFullRestartOutcome::Ready(ready) => {
            &ready.projection
        }
        crate::stage5d_persistence::stage5f_test_seams::Stage5fFullRestartOutcome::Blocked {
            projection,
            ..
        } => projection,
    };
    assert_eq!(projection.account_id, input.account_id);
    assert_eq!(projection.instrument_id, input.target);
    assert_eq!(
        projection.profile_binding,
        "ImoexfPrimaryRiskgateHigh180Lb120|High180|ShadowPnlLb120Positive|NormalAppend"
    );
    assert_eq!(
        projection.canonical_config_fingerprint,
        fast_config_fingerprint
    );
    assert_eq!(projection.source_state, fast_state);
    assert_eq!(projection.source_private, fast_private);
    assert_eq!(
        projection.semantic_payload_fingerprint,
        crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&projection.source_state)
            .expect("Stage 5F fast semantic fingerprint")
    );
    assert_eq!(
        projection.recovery_index_fingerprint,
        crate::stage5c_paper_host::stage5c_recovery_index_fingerprint(
            &projection.known_order_ids,
            &projection.pending_requests,
        )
        .expect("Stage 5F recovery-index fingerprint")
    );
    assert!(projection.known_order_ids.is_empty());
    assert_eq!(
        projection.pending_requests,
        projection
            .source_private
            .pending_entry
            .iter()
            .filter_map(|entry| entry.request_id)
            .chain(
                projection
                    .source_private
                    .pending_exit
                    .iter()
                    .map(|exit| exit.request_id),
            )
            .collect::<Vec<_>>()
    );
    assert_eq!(projection.riskgate_identity.strategy_id, input.strategy_id);
    assert_eq!(
        projection.riskgate_identity.profile_id,
        "imoexf_primary_high180_lb120"
    );
    assert_eq!(projection.riskgate_identity.mr_variant, "high180");
    assert_eq!(
        projection.riskgate_generation,
        crate::hybrid_intraday::RISK_GATE_STATE_GENERATION
    );
    assert!(projection
        .riskgate_ledger_tail_hash
        .starts_with("stage5d_riskgate_ledger_tail_sha256:"));
    assert_eq!(
        projection.lifecycle_watermarks.last_semantic_bar_ts,
        projection.source_private.last_processed_bar_ts
    );
    assert_eq!(
        projection.lifecycle_watermarks.last_broker_event_ts,
        Some(persisted_at - Duration::seconds(1))
    );
    outcome
}

fn settle_stage5f_full_restart_representative(
    row_id: &str,
    ready: Box<crate::stage5d_persistence::stage5f_test_seams::Stage5fFullRestartReady>,
) {
    use crate::stage5e_no_io_lifecycle::callback_authority::issue_stage5e_callback_authority_at;
    let input = load_scenario(row_id);
    let ready = *ready;
    assert!(ready.projection.riskgate_recovery_frontier.is_some());
    let (recovered, accepted) =
        crate::stage5c_paper_host::stage5f_test_seams::sequence_inputs_from_restored_strategy(
            ready.restored,
            input.lifecycle_at,
            input.bar.clone(),
        );
    let b3c = crate::stage5e_no_io_lifecycle::schedule_window_evidence::stage5f_test_seams::b3c_from_sequence_inputs(
        recovered,
        accepted,
        input.target,
        input.bar.close_time_utc,
        input.lifecycle_at,
    );
    let authority = issue_stage5e_callback_authority_at(b3c, input.callback_at)
        .unwrap_or_else(|_| panic!("{row_id}: representative B3C authority must issue"));
    let boundary = invoke_and_settle_stage5f_callback(
        row_id,
        authority,
        input.callback_at,
        Stage5fMutation::None,
    );
    let observed = boundary
        .observed
        .expect("representative exact callback must be observed");
    let Stage5fSettlementOutcome::Settled(receipt) = boundary.settlement else {
        panic!("{row_id}: representative callback must settle");
    };
    let (_, intent_count, history_len, canonical, _) = receipt.test_identity_proof_shape();
    assert_eq!(observed.len(), intent_count);
    assert_eq!(history_len, 1);
    assert!(canonical);
}

#[test]
fn stage5f_v2_full_restart_flat_equivalence() {
    use crate::stage5d_persistence::stage5f_test_seams::{
        Stage5fFullRestartOutcome, Stage5fRepresentativeRiskgateMode,
    };
    let Stage5fFullRestartOutcome::Ready(ready) =
        run_stage5f_full_restart_representative("F01", Stage5fRepresentativeRiskgateMode::Valid)
    else {
        panic!("F01 full restart must be ready");
    };
    assert!(ready.projection.pending_requests.is_empty());
    settle_stage5f_full_restart_representative("F01", ready);
}

#[test]
fn stage5f_v2_full_restart_nonflat_owner_cycle_equivalence() {
    use crate::stage5d_persistence::stage5f_test_seams::{
        Stage5fFullRestartOutcome, Stage5fRepresentativeRiskgateMode,
    };
    let Stage5fFullRestartOutcome::Ready(ready) =
        run_stage5f_full_restart_representative("F04", Stage5fRepresentativeRiskgateMode::Valid)
    else {
        panic!("F04 full restart must be ready");
    };
    assert_eq!(
        ready.projection.source_state["HybridIntradayRuntime"]["current_owner"],
        "intraday_breakout"
    );
    assert!(ready.projection.source_state["HybridIntradayRuntime"]["active_cycle_id"].is_string());
    settle_stage5f_full_restart_representative("F04", ready);
}

#[test]
fn stage5f_v2_full_restart_pending_equivalence() {
    use crate::stage5d_persistence::stage5f_test_seams::{
        Stage5fFullRestartOutcome, Stage5fRepresentativeRiskgateMode,
    };
    let Stage5fFullRestartOutcome::Ready(ready) =
        run_stage5f_full_restart_representative("F26", Stage5fRepresentativeRiskgateMode::Valid)
    else {
        panic!("F26 full restart must be ready");
    };
    assert_eq!(ready.projection.pending_requests.len(), 1);
    assert!(ready.projection.source_private.pending_entry.is_some());
    settle_stage5f_full_restart_representative("F26", ready);
}

#[test]
fn stage5f_v2_full_restart_missing_riskgate_is_typed_blocker() {
    use crate::stage5d_persistence::stage5f_test_seams::{
        Stage5fFullRestartOutcome, Stage5fRepresentativeRiskgateMode,
    };
    let Stage5fFullRestartOutcome::Blocked { reason, projection } =
        run_stage5f_full_restart_representative(
            "F24",
            Stage5fRepresentativeRiskgateMode::MissingAuthority,
        )
    else {
        panic!("F24 full restart must block before callback");
    };
    assert_eq!(
        reason,
        crate::stage5d_persistence::Stage5dRiskGateInjectionBlockReason::LedgerEvidenceInvalid
    );
    assert!(projection.riskgate_recovery_frontier.is_none());
}

#[test]
fn stage5f_f01_no_signal_zero_intent() {
    let result = characterize("F01", Stage5fMutation::None);
    assert_candidate(&result, "accepted", "settled", None);
    assert!(result.ordered_intent_vector.is_empty());
}

#[test]
fn stage5f_f02_bo_long_entry() {
    let result = characterize("F02", Stage5fMutation::None);
    assert_candidate(&result, "accepted", "settled", Some("entry"));
    assert_eq!(result.ordered_intent_vector[0]["owner"], "BO");
    assert_eq!(result.ordered_intent_vector[0]["side"], "buy");
}

#[test]
fn stage5f_f04_bo_long_normal_exit() {
    let result = characterize("F04", Stage5fMutation::None);
    assert_candidate(&result, "accepted", "settled", Some("exit"));
    assert_eq!(result.ordered_intent_vector[0]["owner"], "BO");
    assert_eq!(result.ordered_intent_vector[0]["side"], "sell");
}

#[test]
fn stage5f_f24_riskgate_missing_authority_blocks_before_callback() {
    let result = characterize("F24", Stage5fMutation::None);
    assert_candidate(
        &result,
        "blocked_before_callback",
        "LedgerEvidenceInvalid",
        None,
    );
}

#[test]
fn stage5f_f31_callback_validation_terminal() {
    let result = characterize("F31", Stage5fMutation::CallbackValidation);
    assert_candidate(
        &result,
        "terminal_after_callback",
        "CallbackValidationError",
        None,
    );
    assert_eq!(result.observer_count, 0, "validation stops before on_bar");
}

#[test]
fn stage5f_f32_b3f_identity_or_chronology_preflight_terminal() {
    let result = characterize("F32", Stage5fMutation::B3fChronology);
    assert_candidate(
        &result,
        "terminal_after_callback",
        "ChronologyMismatch",
        None,
    );
    assert_eq!(result.observer_count, 1);
}

#[test]
fn stage5f_f33_stage5c_intent_validation_terminal() {
    let result = characterize("F33", Stage5fMutation::Stage5cDuplicateIntent);
    assert_candidate(
        &result,
        "terminal_after_callback",
        "Stage5cIntentValidationFailed",
        None,
    );
    assert_eq!(result.observer_count, 1);
}

#[test]
fn stage5f_c_candidate_matrix_evidence() {
    let results = stage5f_candidate_results();
    assert_eq!(results.len(), 7);
    println!(
        "STAGE5F_CANDIDATE_JSON_BEGIN\n{}\nSTAGE5F_CANDIDATE_JSON_END",
        serde_json::to_string_pretty(&results).expect("candidate evidence serializes")
    );
    let frozen_candidate = load_json(&candidate_results_path());
    assert_eq!(
        serde_json::to_value(&results).expect("candidate results serialize"),
        frozen_candidate["results"],
        "Stage 5F-c source output drifted from the separately generated candidate"
    );
}

fn stage5f_candidate_results() -> Vec<Stage5fCandidateResult> {
    vec![
        characterize("F01", Stage5fMutation::None),
        characterize("F02", Stage5fMutation::None),
        characterize("F04", Stage5fMutation::None),
        characterize("F24", Stage5fMutation::None),
        characterize("F31", Stage5fMutation::CallbackValidation),
        characterize("F32", Stage5fMutation::B3fChronology),
        characterize("F33", Stage5fMutation::Stage5cDuplicateIntent),
    ]
}

#[test]
fn stage5f_v2_candidate_repeat_is_byte_identical() {
    let first = serde_json::to_vec_pretty(&stage5f_candidate_results())
        .expect("first candidate run serializes");
    let second = serde_json::to_vec_pretty(&stage5f_candidate_results())
        .expect("second candidate run serializes");
    assert_eq!(first, second, "pinned candidate run must be byte-identical");
}

#[test]
fn stage5f_observer_is_linear_redacted_and_returns_to_idle() {
    let scope = arm_observer_once();
    let intents = vec![Intent::Market {
        qty: 3.0,
        side: OrderSide::Buy,
        fill_price: None,
        comment: Some("HYB|sid=hybrid_imoexf|c=65a0000001|o=BO|r=ENTRY".to_string()),
    }
    .with_class(IntentClass::Entry)];
    observe_exact_on_bar_result(&intents);
    let observed = scope
        .consume_once()
        .expect("armed observer must return one result");
    assert_eq!(observed.len(), 1);
    assert_eq!(
        intents.len(),
        1,
        "observation must not change source vector"
    );

    let empty_scope = arm_observer_once();
    observe_exact_on_bar_result(&[]);
    assert_eq!(
        empty_scope
            .consume_once()
            .expect("zero vector is observed")
            .len(),
        0
    );
}

#[test]
fn stage5f_observer_rejects_second_callback_before_consume() {
    let scope = arm_observer_once();
    observe_exact_on_bar_result(&[]);
    let second = std::panic::catch_unwind(|| observe_exact_on_bar_result(&[]));
    assert!(second.is_err(), "second callback must fail closed");
    assert_eq!(
        scope
            .consume_once()
            .expect("first observation remains linearly owned")
            .len(),
        0
    );
}

#[test]
fn stage5f_unconsumed_scope_drop_clears_only_its_generation() {
    let scope = arm_observer_once();
    observe_exact_on_bar_result(&[]);
    drop(scope);

    let next = arm_observer_once();
    observe_exact_on_bar_result(&[]);
    assert_eq!(
        next.consume_once()
            .expect("new generation must not inherit stale observation")
            .len(),
        0
    );
}
