//! Stage 5F test-only observation and source-characterization harness.
//!
//! This module is compiled only for tests. It observes the immutable intent
//! slice returned by the exact broker-neutral `Strategy::on_bar` expression,
//! stores only a redacted projection, and never controls runtime flow.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use broker_core::{BrokerAccountId, Exchange, InstrumentId, Market, StrategyRequestId};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::hybrid_intraday::{
    HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
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

struct Stage5fScenarioInput {
    state_defaults: Value,
    state_seed: Value,
    riskgate_seed: Value,
    row_id: String,
    scenario_id: String,
    callback_at: DateTime<Utc>,
    bar: broker_core::HybridRuntimeBarEvent,
    strategy_id: String,
    account_id: BrokerAccountId,
    target: InstrumentId,
    position_qty: f64,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/stage5/stage5f/v1")
}

fn source_validity_corrections_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/stage-5/stage5f-c-source-validity-corrections.json")
}

fn candidate_results_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/stage-5/stage5f-c-candidate-results.json")
}

fn load_json(path: &Path) -> Value {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("cannot parse {}: {error}", path.display()))
}

fn required_str<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("fixture string missing at {pointer}"))
}

fn required_bool(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("fixture bool missing at {pointer}"))
}

fn required_u64(value: &Value, pointer: &str) -> u64 {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("fixture integer missing at {pointer}"))
}

fn parse_utc(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap_or_else(|error| panic!("invalid fixture timestamp {value}: {error}"))
        .with_timezone(&Utc)
}

fn parse_f64(value: &str) -> f64 {
    let parsed = value
        .parse::<f64>()
        .unwrap_or_else(|error| panic!("invalid fixture decimal {value}: {error}"));
    assert!(parsed.is_finite(), "fixture decimal must be finite");
    parsed
}

fn catalog_seed<'a>(catalog: &'a Value, seed_id: &str) -> &'a Value {
    catalog["seeds"]
        .as_array()
        .expect("seed catalog array")
        .iter()
        .find(|seed| seed["seed_id"].as_str() == Some(seed_id))
        .unwrap_or_else(|| panic!("fixture seed not found: {seed_id}"))
}

fn apply_source_validity_correction(raw: &mut Value, row_id: &str) -> Option<Value> {
    let corrections = load_json(&source_validity_corrections_path());
    assert_eq!(required_u64(&corrections, "/schema_version"), 1);
    assert_eq!(
        required_str(&corrections, "/stage"),
        "5F-c-controlled-paper-invocation"
    );
    assert_eq!(
        required_str(&corrections, "/status"),
        "candidate_input_correction_pending_review"
    );
    assert_eq!(
        required_str(&corrections, "/base_scenario_catalog/path"),
        "tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json"
    );
    assert_eq!(
        required_str(&corrections, "/base_scenario_catalog/sha256"),
        "e83f10b58ba6c72efbf95d561edc9f7de84ce8e092129f6a9b449d2683e84184"
    );
    let records = corrections["corrections"]
        .as_array()
        .expect("Stage 5F-c correction records array");
    assert_eq!(records.len(), 2, "Stage 5F-c correction count drift");
    let correction = records
        .iter()
        .find(|value| value["row_id"].as_str() == Some(row_id))?;
    assert!(
        correction["reason"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "Stage 5F-c correction requires rationale"
    );
    let overrides = correction["overrides"]
        .as_object()
        .expect("Stage 5F-c overrides object");
    assert_eq!(
        overrides.keys().map(String::as_str).collect::<Vec<_>>(),
        ["bar", "clock", "state_seed"],
        "Stage 5F-c override surface drift"
    );
    for section in ["bar", "clock"] {
        let values = overrides[section]
            .as_object()
            .expect("Stage 5F-c override section object");
        let target = raw[section]
            .as_object_mut()
            .expect("Stage 5F-c target section object");
        for (key, value) in values {
            assert!(target.contains_key(key), "Stage 5F-c unknown override key");
            target.insert(key.clone(), value.clone());
        }
    }
    Some(correction.clone())
}

fn load_scenario(row_id: &str) -> Stage5fScenarioInput {
    let root = fixture_root();
    let scenarios = load_json(&root.join("scenarios/atomic-hybrid-scenarios.json"));
    let states = load_json(&root.join("states/imoexf-hybrid-state-seeds.json"));
    let riskgates = load_json(&root.join("riskgate/imoexf-high180-riskgate-seeds.json"));
    let mut raw = scenarios["records"]
        .as_array()
        .expect("scenario records array")
        .iter()
        .find(|record| record["row_id"].as_str() == Some(row_id))
        .unwrap_or_else(|| panic!("scenario row not found: {row_id}"))
        .clone();
    let correction = apply_source_validity_correction(&mut raw, row_id);

    assert_eq!(required_u64(&raw, "/schema_version"), 1);
    assert_eq!(required_str(&raw, "/target/strategy_id"), "hybrid_imoexf");
    assert_eq!(required_str(&raw, "/target/account_id"), "ACC_TEST_0001");
    assert_eq!(
        required_str(&raw, "/target/profile"),
        "imoexf_primary_riskgate_high180_lb120"
    );
    assert!(required_bool(&raw, "/target/paper_only"));

    let target = InstrumentId {
        symbol: required_str(&raw, "/target/instrument/symbol").to_string(),
        venue_symbol: Some(required_str(&raw, "/target/instrument/venue_symbol").to_string()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    };
    let bar = broker_core::HybridRuntimeBarEvent {
        instrument: target.clone(),
        close_time_utc: parse_utc(required_str(&raw, "/bar/close_time_utc")).timestamp(),
        open: parse_f64(required_str(&raw, "/bar/open")),
        high: parse_f64(required_str(&raw, "/bar/high")),
        low: parse_f64(required_str(&raw, "/bar/low")),
        close: parse_f64(required_str(&raw, "/bar/close")),
        volume: parse_f64(required_str(&raw, "/bar/volume")),
        origin: broker_core::HybridRuntimeBarOrigin::Live,
        is_final: required_bool(&raw, "/bar/is_final"),
        timeframe_sec: u32::try_from(required_u64(&raw, "/bar/timeframe_sec"))
            .expect("timeframe fits u32"),
    };
    let state_seed_id = required_str(&raw, "/pre_state/seed_id");
    let riskgate_seed_id = required_str(&raw, "/riskgate/seed_id");
    let state_defaults = states["seed_defaults"].clone();
    let mut state_seed = catalog_seed(&states, state_seed_id).clone();
    if let Some(correction) = correction {
        let overrides = correction["overrides"]["state_seed"]
            .as_object()
            .expect("Stage 5F-c state-seed override object");
        let target = state_seed
            .as_object_mut()
            .expect("Stage 5F-c state seed object");
        for (key, value) in overrides {
            assert!(
                target.contains_key(key),
                "Stage 5F-c unknown state-seed override key"
            );
            target.insert(key.clone(), value.clone());
        }
    }
    let riskgate_seed = catalog_seed(&riskgates, riskgate_seed_id).clone();
    let position_qty = parse_f64(required_str(&state_seed, "/position_qty"));

    Stage5fScenarioInput {
        state_defaults,
        row_id: row_id.to_string(),
        scenario_id: required_str(&raw, "/scenario_id").to_string(),
        callback_at: parse_utc(required_str(&raw, "/clock/callback_ts_utc")),
        strategy_id: required_str(&raw, "/target/strategy_id").to_string(),
        account_id: BrokerAccountId::new(required_str(&raw, "/target/account_id")),
        target,
        bar,
        state_seed,
        riskgate_seed,
        position_qty,
    }
}

fn stage5f_config() -> HybridIntradayRuntimeConfig {
    HybridIntradayRuntimeConfig {
        symbol: "IMOEXF".to_string(),
        profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
        mr_variant: MeanReversionVariant::High180,
        mr_gate_policy: MrGatePolicy::ShadowPnlLb120Positive,
        risk_gate_mode: RiskGateMode::NormalAppend,
        risk_gate_seed_file: None,
        risk_gate_ledger_key: None,
        model_session_start_time: None,
        model_session_end_time: None,
        qty: 3.0,
        live_order_style: MarketBuyAndCloseLiveOrderStyle::Market,
        tick_size: 0.5,
        marketable_limit_offset_ticks: 0,
        timezone_offset_hours: 3,
        session_close_hour: 23,
        session_close_minute: 49,
        weekends_off: true,
        stop_end_buffer_sec: 60,
        repair_deadline_sec: 180,
        sl_escalate_timeout_sec: 30,
        max_repair_retries: 3,
        repair_backoff_base_sec: 5,
        repair_backoff_max_sec: 60,
        pending_timeout_sec: 30,
        partial_entry_fill_timeout_ms: 3_000,
        mr_config: MeanReversionConfig::default(),
        breakout_config: IntradayBreakoutConfig::default(),
        orchestrator_config: HybridOrchestratorConfig::default(),
    }
}

fn source_cycle_id(seed: &Value) -> Option<String> {
    seed["active_cycle_id"].as_str().map(|value| {
        assert_eq!(value.len(), 10, "source cycle id must be exactly 10 bytes");
        assert!(
            value.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "source cycle id must be production-valid ASCII hex"
        );
        value.to_string()
    })
}

fn materialize_strategy(input: &Stage5fScenarioInput) -> HybridIntradayRuntimeStrategy {
    let owner = input.state_seed["current_owner"].clone();
    let side = input.state_seed["current_side"].clone();
    let active_cycle_id = source_cycle_id(&input.state_seed)
        .map(Value::String)
        .unwrap_or(Value::Null);
    let defaults = &input.state_defaults;
    let state_json = json!({
        "HybridIntradayRuntime": {
            "active_cycle_id": active_cycle_id,
            "next_cycle_seq": required_u64(defaults, "/next_cycle_seq"),
            "last_position_qty": input.position_qty,
            "current_owner": owner,
            "current_side": side,
            "entry_ready": required_bool(defaults, "/entry_ready"),
            "safe_mode_close_only": required_bool(defaults, "/safe_mode_close_only"),
            "last_bar_close": parse_f64(required_str(defaults, "/last_bar_close")),
            "prev_day_close": parse_f64(required_str(defaults, "/prev_day_close")),
            "last_day_local": required_str(defaults, "/last_day_local"),
            "current_day_high": parse_f64(required_str(defaults, "/current_day_high")),
            "current_day_low": parse_f64(required_str(defaults, "/current_day_low")),
            "current_day_close": parse_f64(required_str(defaults, "/current_day_close")),
            "prev_day_range": parse_f64(required_str(defaults, "/prev_day_range")),
            "prev_day_return": 0.0,
            "day_before_close": parse_f64(required_str(defaults, "/day_before_close")),
            "today_start_local": required_str(defaults, "/today_start_local"),
            "was_long_today": input.state_seed["was_long_today"],
            "was_short_today": input.state_seed["was_short_today"],
            "overnight_exit_armed_date": input.state_seed["overnight_exit_armed_date"],
            "risk_gate_mr_enabled_current_session": true,
            "risk_gate_rolling_sum_lb120": 158.60000000000008,
            "risk_gate_last_finalized_session_date": "2026-01-05",
            "risk_gate_ledger_rows_count": 221
        }
    });
    let state = serde_json::from_value(state_json).expect("Stage 5F state must decode exactly");
    let mut strategy = HybridIntradayRuntimeStrategy::new(stage5f_config());
    Strategy::set_state(&mut strategy, state);

    let last_processed = parse_utc(required_str(
        &input.state_seed,
        "/last_processed_bar_ts_utc",
    ));
    let mut private = strategy
        .stage5d_export_runtime_private_extension()
        .expect("Stage 5F state must export its private extension");
    private.last_processed_bar_ts = Some(last_processed);
    strategy
        .stage5d_apply_runtime_private_extension(&private)
        .expect("Stage 5F private extension must apply exactly");
    strategy
}

fn inject_valid_riskgate(
    strategy: &mut HybridIntradayRuntimeStrategy,
    input: &Stage5fScenarioInput,
) -> Result<(), &'static str> {
    match required_str(&input.riskgate_seed, "/authority_state") {
        "complete" => {
            assert_eq!(
                required_str(&input.riskgate_seed, "/risk_gate_mode"),
                "normal_append"
            );
            assert!(!required_bool(&input.riskgate_seed, "/enforced_for_entry"));
            Strategy::on_risk_gate_state(
                strategy,
                &RiskGateRuntimeState {
                    profile_id: "imoexf_primary_high180_lb120".to_string(),
                    last_finalized_session_date: Some(
                        chrono::NaiveDate::parse_from_str(
                            required_str(&input.riskgate_seed, "/last_finalized_session_date"),
                            "%Y-%m-%d",
                        )
                        .expect("riskgate session date"),
                    ),
                    rolling_sum_lb120: Some(parse_f64(required_str(
                        &input.riskgate_seed,
                        "/rolling_sum_lb120",
                    ))),
                    mr_enabled_current_session: Some(required_bool(
                        &input.riskgate_seed,
                        "/mr_enabled_current_session",
                    )),
                    mr_enabled_next_session: Some(required_bool(
                        &input.riskgate_seed,
                        "/mr_enabled_next_session",
                    )),
                    ledger_rows_count: usize::try_from(required_u64(
                        &input.riskgate_seed,
                        "/ledger_rows_count",
                    ))
                    .expect("ledger count fits usize"),
                },
            );
            Ok(())
        }
        "missing" => Err("riskgate_authority_missing"),
        "inconsistent" => Err("riskgate_authority_inconsistent"),
        "materialization_terminal" => Err("authority_or_materialization_terminal"),
        other => panic!("unknown riskgate authority state: {other}"),
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

enum Stage5fMutation {
    None,
    CallbackValidation,
    B3fChronology,
    Stage5cDuplicateIntent,
}

fn characterize(row_id: &str, mutation: Stage5fMutation) -> Stage5fCandidateResult {
    use crate::stage5e_no_io_lifecycle::callback_authority::{
        callback_settlement::validate_and_settle_stage5e_paper_callback_escrow,
        invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
    };

    let input = load_scenario(row_id);
    let mut strategy = materialize_strategy(&input);
    let pre_state_fingerprint = stage5f_state_fingerprint(&strategy);
    if let Err(outcome) = inject_valid_riskgate(&mut strategy, &input) {
        return Stage5fCandidateResult {
            schema_version: 1,
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
            b3f_outcome: outcome.to_string(),
            settlement_identity_sha256: None,
        };
    }

    let position_qty = Decimal::from_str(required_str(&input.state_seed, "/position_qty"))
        .expect("position quantity decimal");
    let predecessor_close_ts = input.bar.close_time_utc - 600;
    let bar_close_ts = input.bar.close_time_utc;
    let (recovered, accepted) =
        crate::stage5c_paper_host::stage5f_test_sequence_inputs_from_owned_strategy(
            strategy,
            input.strategy_id.clone(),
            input.account_id.clone(),
            input.target.clone(),
            0.5,
            position_qty,
            input.callback_at,
            predecessor_close_ts,
            input.bar.clone(),
        );
    let mut b3c = crate::stage5e_no_io_lifecycle::schedule_window_evidence::
        stage5f_test_b3c_from_sequence_inputs(
            recovered,
            accepted,
            input.target,
            bar_close_ts,
            input.callback_at,
        );
    if matches!(mutation, Stage5fMutation::CallbackValidation) {
        crate::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence::
            stage5f_test_force_callback_validation_error(&mut b3c);
    }
    let authority = issue_stage5e_callback_authority_at(b3c, input.callback_at)
        .unwrap_or_else(|_| panic!("{row_id}: B3C must issue callback authority"));
    let authority_pre_fingerprint = authority.test_ownership_fingerprint().0;
    assert_eq!(
        authority_pre_fingerprint, pre_state_fingerprint,
        "{row_id}: state changed before callback"
    );
    let scope = arm_observer_once();
    crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
    let mut escrow = invoke_stage5e_authorized_paper_callback_at(authority, input.callback_at)
        .unwrap_or_else(|_| panic!("{row_id}: callback must reach opaque escrow"));
    let callback_count = crate::stage5c_paper_host::stage5e_test_b3e_callback_count();
    assert_eq!(callback_count, 1, "{row_id}: exact callback cardinality");
    let observed = scope.consume_once();
    let observer_count = usize::from(observed.is_some());
    let accepted_post_state_fingerprint = Some(escrow.test_strategy_state_fingerprint());
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
    let settlement = validate_and_settle_stage5e_paper_callback_escrow(escrow);
    match settlement {
        Ok(receipt) => {
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
                schema_version: 1,
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
        Err(terminal) => {
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
                schema_version: 1,
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
        "riskgate_authority_missing",
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
    let results = vec![
        characterize("F01", Stage5fMutation::None),
        characterize("F02", Stage5fMutation::None),
        characterize("F04", Stage5fMutation::None),
        characterize("F24", Stage5fMutation::None),
        characterize("F31", Stage5fMutation::CallbackValidation),
        characterize("F32", Stage5fMutation::B3fChronology),
        characterize("F33", Stage5fMutation::Stage5cDuplicateIntent),
    ];
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
