//! Stage 8B-P1-f O2 fresh first-boot source materializer.
//!
//! Network access is confined to the dedicated broker-finam GET-only client.
//! This module has no Redis, guardian, systemd or broker-write capability.

use std::collections::{BTreeMap, BTreeSet};

use broker_core::event::Bar;
use broker_core::{BarAggregationAction, CanonicalBarAggregator, Market, MarketDataSourceKind};
use broker_finam::{
    classify_finam_order_status, map_bar, AccessToken, AccountOrdersResponse, AccountResponse,
    AssetParamsResponse, AssetScheduleResponse, BarsResponse, FinamOrderStatusClass,
    Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteV1,
    STAGE8B_P1F_O2_VENUE_SYMBOL,
};
use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const M1_SEMANTIC_DOMAIN: &str = "moex.stage8b.p1f.exact-m1.v1";
const MAX_CANDIDATE_AGE_SECONDS: i64 = 900;
const MAX_BARS_CHUNK_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_BARS_RANGE_SECONDS: i64 = 400 * 24 * 60 * 60;
pub const STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL: &str = "INJECT_FROM_ACCOUNT_CREDENTIAL";
pub const STAGE8B_P1F_O2_ACCOUNT_ALIAS: &str = "finam-paper-primary";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageSessionV1 {
    session_date: String,
    windows: Vec<CoverageWindowV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageWindowV1 {
    first_close_time_utc: i64,
    last_close_time_utc: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum Stage8bP1fO2MaterializerErrorV1 {
    #[error("O2 GET collection failed")]
    Get,
    #[error("O2 source template is malformed or has an unexpected identity")]
    Template,
    #[error("O2 account snapshot is incomplete, non-flat or mismatched")]
    AccountTruth,
    #[error("O2 orders snapshot is incomplete, active or ambiguous")]
    OrdersTruth,
    #[error("O2 instrument params are incomplete or mismatched")]
    InstrumentTruth,
    #[error("O2 schedule snapshot is incomplete or mismatched")]
    ScheduleTruth,
    #[error("O2 bars do not contain one fresh complete aligned M10")]
    BarsTruth,
    #[error("O2 materialized source failed the accepted production parser")]
    SourceRejected,
    #[error("O2 materialization failure: {0:?}")]
    Diagnostic(Box<Stage8bP1fO2FailureContextV1>),
}

/// Bounded, allowlisted diagnostic projection. No upstream strings, response
/// bodies, account identities, headers or credentials can be stored here.
#[derive(Debug, Clone, Serialize)]
pub struct Stage8bP1fO2FailureContextV1 {
    stage: &'static str,
    reason_code: &'static str,
    last_validated_stage: &'static str,
    completed_typed_gets: usize,
    completed_chunks: usize,
    chunk_index: Option<usize>,
    chunk_start_utc: Option<i64>,
    chunk_end_utc: Option<i64>,
    m1_count: Option<usize>,
    first_available_open_utc: Option<i64>,
    last_available_open_utc: Option<i64>,
    session_date: Option<chrono::NaiveDate>,
    window_index: Option<usize>,
    window_first_close_utc: Option<i64>,
    window_last_close_utc: Option<i64>,
    m10_close_utc: Option<i64>,
    first_missing_m1_open_utc: Option<i64>,
    candidate_close_utc: Option<i64>,
    candidate_age_seconds: Option<i64>,
}

impl Stage8bP1fO2FailureContextV1 {
    fn new(stage: &'static str, reason_code: &'static str) -> Self {
        Self {
            stage,
            reason_code,
            last_validated_stage: "none",
            completed_typed_gets: 0,
            completed_chunks: 0,
            chunk_index: None,
            chunk_start_utc: None,
            chunk_end_utc: None,
            m1_count: None,
            first_available_open_utc: None,
            last_available_open_utc: None,
            session_date: None,
            window_index: None,
            window_first_close_utc: None,
            window_last_close_utc: None,
            m10_close_utc: None,
            first_missing_m1_open_utc: None,
            candidate_close_utc: None,
            candidate_age_seconds: None,
        }
    }
}

impl Stage8bP1fO2MaterializerErrorV1 {
    fn detailed(stage: &'static str, code: &'static str) -> Self {
        Self::Diagnostic(Box::new(Stage8bP1fO2FailureContextV1::new(stage, code)))
    }

    fn context(self) -> Stage8bP1fO2FailureContextV1 {
        let (stage, code) = match self {
            Self::Diagnostic(context) => return *context,
            Self::Get => ("collection", "get_or_observation_rejected"),
            Self::Template => ("canonical_validation", "template_rejected"),
            Self::AccountTruth => ("canonical_validation", "account_truth_rejected"),
            Self::OrdersTruth => ("canonical_validation", "orders_truth_rejected"),
            Self::InstrumentTruth => ("canonical_validation", "instrument_truth_rejected"),
            Self::ScheduleTruth => ("canonical_validation", "schedule_truth_rejected"),
            Self::BarsTruth => ("mapping", "bars_rejected"),
            Self::SourceRejected => ("canonical_validation", "source_rejected"),
        };
        Stage8bP1fO2FailureContextV1::new(stage, code)
    }

    fn after(self, validated: &'static str) -> Self {
        let mut context = self.context();
        context.last_validated_stage = validated;
        Self::Diagnostic(Box::new(context))
    }

    /// One bounded JSON journal record. Even malformed caller-provided hash or
    /// time strings are discarded, not echoed. Numeric times are UTC seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn diagnostic_json(
        self,
        manifest_sha256: &str,
        policy_sha256: &str,
        template_sha256: &str,
        bars_start: &str,
        bars_end: &str,
        trusted_now: DateTime<Utc>,
    ) -> String {
        fn safe_hash(value: &str) -> Option<&str> {
            valid_sha256(value).then_some(value)
        }
        let value = json!({
            "schema_version": 1, "domain": "stage8b-p1f-o2-failure-diagnostic-v1",
            "manifest_sha256": safe_hash(manifest_sha256),
            "policy_sha256": safe_hash(policy_sha256),
            "template_sha256": safe_hash(template_sha256),
            "symbol": STAGE8B_P1F_O2_VENUE_SYMBOL,
            "requested_bars_start_utc": parse_timestamp(bars_start).map(|t| t.timestamp()),
            "requested_bars_end_utc": parse_timestamp(bars_end).map(|t| t.timestamp()),
            "trusted_now_utc": trusted_now.timestamp(), "failure": self.context(),
        });
        let encoded = value.to_string();
        if encoded.len() <= 4096 {
            encoded
        } else {
            "{\"schema_version\":1,\"domain\":\"stage8b-p1f-o2-failure-diagnostic-v1\",\"reason_code\":\"diagnostic_size_limit\"}".into()
        }
    }
}

#[derive(Default)]
struct CollectionProgress {
    gets: usize,
    chunks: usize,
    last_collected: Option<&'static str>,
    chunk: Option<(usize, i64, i64)>,
    bars: Option<(usize, Option<i64>, Option<i64>)>,
}

impl CollectionProgress {
    fn error(&self, error: Stage8bP1fO2MaterializerErrorV1) -> Stage8bP1fO2MaterializerErrorV1 {
        let mut c = error.context();
        c.completed_typed_gets = self.gets;
        c.completed_chunks = self.chunks;
        if c.last_validated_stage == "none" {
            c.last_validated_stage = self.last_collected.unwrap_or("none");
        }
        if let Some((index, start, end)) = self.chunk {
            c.chunk_index = Some(index);
            c.chunk_start_utc = Some(start);
            c.chunk_end_utc = Some(end);
        }
        if let Some((count, first, last)) = self.bars {
            c.m1_count = Some(count);
            c.first_available_open_utc = first;
            c.last_available_open_utc = last;
        }
        Stage8bP1fO2MaterializerErrorV1::Diagnostic(Box::new(c))
    }

    fn observe_bars(&mut self, bars: &BarsResponse) {
        let mut timestamps = bars
            .bars
            .iter()
            .filter_map(|b| parse_timestamp(&b.timestamp).map(|t| t.timestamp()));
        let first = timestamps.next();
        let (first, last) = timestamps.fold((first, first), |(low, high), ts| {
            (
                Some(low.unwrap_or(ts).min(ts)),
                Some(high.unwrap_or(ts).max(ts)),
            )
        });
        self.bars = Some((bars.bars.len(), first, last));
    }
}

fn merge_bars_chunk(
    bars: &mut BTreeMap<String, broker_finam::dto::Bar>,
    chunk: BarsResponse,
) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    if chunk.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL {
        return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
            "collection",
            "chunk_symbol_mismatch",
        ));
    }
    for bar in chunk.bars {
        match bars.entry(bar.timestamp.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(bar);
            }
            std::collections::btree_map::Entry::Occupied(entry) if entry.get() == &bar => {}
            std::collections::btree_map::Entry::Occupied(_) => {
                return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
                    "collection",
                    "chunk_duplicate_conflict",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fO2RouteEvidenceV1 {
    pub route: String,
    pub request_sha256: String,
    pub response_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage8bP1fO2MaterializationEvidenceV1 {
    pub schema_version: u16,
    pub domain: String,
    pub captured_at_utc: String,
    pub account_id_sha256: String,
    pub venue_symbol: String,
    pub target_position_qty: String,
    pub target_active_orders_count: u64,
    pub account_active_orders_count: u64,
    pub active_orders_complete: bool,
    pub selected_m1_count: usize,
    pub candidate_redis_id: String,
    pub candidate_semantic_id_sha256: String,
    pub source_bundle_sha256: String,
    pub route_evidence: Vec<Stage8bP1fO2RouteEvidenceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1fO2MaterializedSourceV1 {
    pub exact_source_bytes: Vec<u8>,
    pub evidence: Stage8bP1fO2MaterializationEvidenceV1,
}

/// Executes the five accepted O2 GET route kinds and feeds their typed
/// snapshots into the pure materializer. Bars use deterministic bounded time
/// chunks of the same closed route kind. No caller-provided method, URL or
/// route exists.
pub async fn collect_stage8b_p1f_o2_source_v1(
    account_id: &str,
    token: &AccessToken,
    source_template_bytes: &[u8],
    bars_start_utc: &str,
    bars_end_utc: &str,
    trusted_now: DateTime<Utc>,
) -> Result<Stage8bP1fO2MaterializedSourceV1, Stage8bP1fO2MaterializerErrorV1> {
    let mut progress = CollectionProgress::default();
    let collected = async {
        let client = Stage8bP1fO2GetOnlyClientV1::new(account_id)
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        let (account, account_observation) = client
            .fetch_typed::<AccountResponse>(token, Stage8bP1fO2GetRouteV1::Account)
            .await
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        progress.gets += 1;
        progress.last_collected = Some("account_collected");
        let (orders, orders_observation) = client
            .fetch_typed::<AccountOrdersResponse>(token, Stage8bP1fO2GetRouteV1::AccountOrders)
            .await
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        progress.gets += 1;
        progress.last_collected = Some("orders_collected");
        let (params, params_observation) = client
            .fetch_typed::<AssetParamsResponse>(token, Stage8bP1fO2GetRouteV1::AssetParams)
            .await
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        progress.gets += 1;
        progress.last_collected = Some("params_collected");
        let (schedule, schedule_observation) = client
            .fetch_typed::<AssetScheduleResponse>(token, Stage8bP1fO2GetRouteV1::AssetSchedule)
            .await
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
        progress.gets += 1;
        progress.last_collected = Some("schedule_collected");
        let bars_start = parse_canonical_timestamp(bars_start_utc)?;
        let bars_end = parse_canonical_timestamp(bars_end_utc)?;
        let range_seconds = bars_end.signed_duration_since(bars_start).num_seconds();
        if range_seconds <= 0 || range_seconds > MAX_BARS_RANGE_SECONDS {
            return Err(Stage8bP1fO2MaterializerErrorV1::Get);
        }
        let mut cursor = bars_start;
        let mut bars_by_timestamp = BTreeMap::new();
        let mut bars_observations = Vec::new();
        while cursor < bars_end {
            let chunk_end = (cursor + Duration::seconds(MAX_BARS_CHUNK_SECONDS)).min(bars_end);
            progress.chunk = Some((progress.chunks, cursor.timestamp(), chunk_end.timestamp()));
            progress.bars = None;
            let (chunk, observation) = client
                .fetch_typed::<BarsResponse>(
                    token,
                    Stage8bP1fO2GetRouteV1::Bars {
                        start_time: canonical_timestamp(cursor),
                        end_time: canonical_timestamp(chunk_end),
                    },
                )
                .await
                .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Get)?;
            progress.gets += 1;
            progress.observe_bars(&chunk);
            merge_bars_chunk(&mut bars_by_timestamp, chunk)?;
            progress.chunks += 1;
            progress.last_collected = Some("bar_chunk_merged");
            bars_observations.push(observation);
            cursor = chunk_end;
        }
        let bars = BarsResponse {
            bars: bars_by_timestamp.into_values().collect(),
            symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.to_string(),
        };
        progress.chunk = None;
        progress.observe_bars(&bars);
        progress.last_collected = Some("collection_complete");
        let mut observations = vec![
            account_observation,
            orders_observation,
            params_observation,
            schedule_observation,
        ];
        observations.extend(bars_observations);

        materialize_stage8b_p1f_o2_source_v1(
            source_template_bytes,
            account_id,
            STAGE8B_P1F_O2_ACCOUNT_ALIAS,
            account,
            orders,
            params,
            schedule,
            bars,
            trusted_now,
            observations,
        )
    }
    .await;
    collected.map_err(|error| progress.error(error))
}

#[allow(clippy::too_many_arguments)]
pub fn materialize_stage8b_p1f_o2_source_v1(
    source_template_bytes: &[u8],
    expected_account_id: &str,
    source_account_alias: &str,
    account: AccountResponse,
    orders: AccountOrdersResponse,
    params: AssetParamsResponse,
    schedule: AssetScheduleResponse,
    bars: BarsResponse,
    trusted_now: DateTime<Utc>,
    observations: Vec<Stage8bP1fO2GetObservationV1>,
) -> Result<Stage8bP1fO2MaterializedSourceV1, Stage8bP1fO2MaterializerErrorV1> {
    let mut source: Value = serde_json::from_slice(source_template_bytes)
        .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
    let operational_identity = source
        .get("operational_identity_sha256")
        .and_then(Value::as_str)
        .filter(|value| valid_sha256(value))
        .ok_or(Stage8bP1fO2MaterializerErrorV1::Template)?
        .to_string();
    let template_account = source
        .get("broker_truth")
        .and_then(Value::as_object)
        .and_then(|truth| truth.get("account_id"))
        .and_then(Value::as_str)
        .ok_or(Stage8bP1fO2MaterializerErrorV1::Template)?;
    if template_account != STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL
        || source_account_alias != STAGE8B_P1F_O2_ACCOUNT_ALIAS
        || account.account_id != expected_account_id
    {
        return Err(Stage8bP1fO2MaterializerErrorV1::AccountTruth);
    }
    validate_flat_target(&account)?;
    validate_zero_active_orders(expected_account_id, &orders)?;
    validate_params(expected_account_id, &params)?;
    validate_schedule(&schedule)?;
    validate_route_observations(&observations, &account, &orders, &params, &schedule, &bars)?;
    let mapped = map_exact_m1(&bars).map_err(|error| error.after("truth_and_routes"))?;
    let coverage_sessions = source
        .get("history_coverage")
        .and_then(Value::as_object)
        .and_then(|coverage| coverage.get("sessions"))
        .cloned()
        .ok_or(Stage8bP1fO2MaterializerErrorV1::Template)?;
    let (history_bars, history_inputs) = build_history(&mapped, coverage_sessions.clone())
        .map_err(|error| error.after("mapping"))?;
    let (runtime, _) = runtime_durable_service::Stage8bP1RuntimeProfileV1::build_hybrid_runtime()
        .map_err(|_| Stage8bP1fO2MaterializerErrorV1::SourceRejected)?;
    let riskgate_observations =
        strategy_runtime_core::rebuild_stage8b_p1e_riskgate_observations_v1(
            &runtime,
            &history_inputs,
        )
        .map_err(|_| Stage8bP1fO2MaterializerErrorV1::SourceRejected)?;
    let riskgate_observations = riskgate_observations
        .into_iter()
        .map(|observation| {
            json!({
                "session_date": observation.session_date.format("%Y-%m-%d").to_string(),
                "shadow_pnl_points": observation.shadow_pnl_points,
                "shadow_trade_count": observation.shadow_trade_count
            })
        })
        .collect::<Vec<_>>();
    let candidate = build_candidate_from_mapped(&mapped, &operational_identity, trusted_now)
        .map_err(|error| error.after("history_and_riskgate"))?;

    let captured_at = canonical_timestamp(trusted_now);
    source["captured_at_utc"] = Value::String(captured_at.clone());
    source["broker_truth"] = json!({
        "checked_at_utc": captured_at,
        "account_id": source_account_alias,
        "instrument": STAGE8B_P1F_O2_VENUE_SYMBOL,
        "target_position_qty": "0",
        "target_positions_complete": true,
        "target_active_orders_count": 0,
        "account_active_orders_count": 0,
        "active_orders_complete": true,
        "instrument_price_step": runtime_durable_service::STAGE8B_P1_TICK_SIZE
    });
    let history_hash = canonical_value_sha256(&Value::Array(history_bars.clone()));
    let coverage_hash = canonical_value_sha256(&coverage_sessions);
    let riskgate_hash = canonical_value_sha256(&Value::Array(riskgate_observations.clone()));
    source["history_provenance"] = json!({
        "source_mode": "finam_derived_m1_to_m10",
        "source_timeframe_sec": 60,
        "target_timeframe_sec": 600,
        "aggregation_complete": true,
        "gap_absence_proven": true
    });
    source["history_coverage"] = json!({
        "source_mode": "config-bound-explicit-session-windows-v1",
        "sessions_sha256": coverage_hash,
        "sessions": coverage_sessions
    });
    source["history_bars"] = Value::Array(history_bars);
    source["riskgate_history"] = json!({
        "source_mode": "source-compatible-high180-shadow-history-v1",
        "state_generation": "runtime-ledger-v1",
        "history_bars_sha256": history_hash,
        "session_observations_sha256": riskgate_hash,
        "session_observations": riskgate_observations
    });
    source["candidate"] = candidate.value;
    let exact_source_bytes =
        serde_json::to_vec(&source).map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
    let validated = runtime_durable_service::validate_stage8b_p1e_first_boot_source_bytes_v1(
        &exact_source_bytes,
        &operational_identity,
        source_account_alias,
        trusted_now,
    )
    .map_err(|_| Stage8bP1fO2MaterializerErrorV1::SourceRejected)?;
    if validated.candidate_semantic_id_sha256() != candidate.semantic_id_sha256 {
        return Err(Stage8bP1fO2MaterializerErrorV1::SourceRejected);
    }
    let source_bundle_sha256 = sha256_hex(&exact_source_bytes);
    let route_evidence = observations
        .into_iter()
        .map(|observation| Stage8bP1fO2RouteEvidenceV1 {
            route: format!("{:?}", observation.route),
            request_sha256: observation.request_sha256,
            response_sha256: observation.response_sha256,
        })
        .collect();
    Ok(Stage8bP1fO2MaterializedSourceV1 {
        exact_source_bytes,
        evidence: Stage8bP1fO2MaterializationEvidenceV1 {
            schema_version: 1,
            domain: "stage8b-p1f-o2-materialization-evidence-v1".into(),
            captured_at_utc: canonical_timestamp(trusted_now),
            account_id_sha256: sha256_hex(expected_account_id.as_bytes()),
            venue_symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            target_position_qty: "0".into(),
            target_active_orders_count: 0,
            account_active_orders_count: 0,
            active_orders_complete: true,
            selected_m1_count: 10,
            candidate_redis_id: candidate.redis_id,
            candidate_semantic_id_sha256: candidate.semantic_id_sha256,
            source_bundle_sha256,
            route_evidence,
        },
    })
}

fn validate_flat_target(account: &AccountResponse) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    for position in &account.positions {
        if position.symbol.as_deref() != Some(STAGE8B_P1F_O2_VENUE_SYMBOL) {
            continue;
        }
        let quantity = position
            .quantity
            .as_ref()
            .or(position.balance.as_ref())
            .ok_or(Stage8bP1fO2MaterializerErrorV1::AccountTruth)?
            .value
            .parse::<Decimal>()
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::AccountTruth)?;
        if !quantity.is_zero() {
            return Err(Stage8bP1fO2MaterializerErrorV1::AccountTruth);
        }
    }
    Ok(())
}

fn validate_zero_active_orders(
    account_id: &str,
    orders: &AccountOrdersResponse,
) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    for order in &orders.orders {
        if order.order.account_id != account_id {
            return Err(Stage8bP1fO2MaterializerErrorV1::OrdersTruth);
        }
        if !matches!(
            classify_finam_order_status(&order.status),
            FinamOrderStatusClass::TerminalFilled
                | FinamOrderStatusClass::TerminalCanceled
                | FinamOrderStatusClass::TerminalRejected
                | FinamOrderStatusClass::TerminalExpired
        ) {
            return Err(Stage8bP1fO2MaterializerErrorV1::OrdersTruth);
        }
    }
    Ok(())
}

fn validate_params(
    account_id: &str,
    params: &AssetParamsResponse,
) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    if params.account_id.as_deref() != Some(account_id)
        || params.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL
        || !matches!(
            (params.is_tradable, params.tradeable),
            (Some(true), _) | (_, Some(true))
        )
        || params.is_tradable == Some(false)
        || params.tradeable == Some(false)
    {
        return Err(Stage8bP1fO2MaterializerErrorV1::InstrumentTruth);
    }
    Ok(())
}

fn validate_schedule(
    schedule: &AssetScheduleResponse,
) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    if schedule.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL || schedule.sessions.is_empty() {
        return Err(Stage8bP1fO2MaterializerErrorV1::ScheduleTruth);
    }
    for session in &schedule.sessions {
        let interval = session
            .interval
            .as_ref()
            .ok_or(Stage8bP1fO2MaterializerErrorV1::ScheduleTruth)?;
        let start = interval
            .start_time
            .as_deref()
            .and_then(parse_timestamp)
            .ok_or(Stage8bP1fO2MaterializerErrorV1::ScheduleTruth)?;
        let end = interval
            .end_time
            .as_deref()
            .and_then(parse_timestamp)
            .ok_or(Stage8bP1fO2MaterializerErrorV1::ScheduleTruth)?;
        if start >= end {
            return Err(Stage8bP1fO2MaterializerErrorV1::ScheduleTruth);
        }
    }
    Ok(())
}

fn validate_route_observations(
    observations: &[Stage8bP1fO2GetObservationV1],
    account: &AccountResponse,
    orders: &AccountOrdersResponse,
    params: &AssetParamsResponse,
    schedule: &AssetScheduleResponse,
    bars: &BarsResponse,
) -> Result<(), Stage8bP1fO2MaterializerErrorV1> {
    if observations.len() < 5 {
        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
    }
    let mut counts = BTreeMap::new();
    let mut combined_bars = BTreeMap::new();
    let kinds = observations
        .iter()
        .map(|observation| {
            if !valid_sha256(&observation.request_sha256)
                || !valid_sha256(&observation.response_sha256)
                || sha256_hex(&observation.exact_response_bytes) != observation.response_sha256
            {
                return Err(Stage8bP1fO2MaterializerErrorV1::Get);
            }
            *counts
                .entry(format!("{:?}", observation.route))
                .or_insert(0_usize) += 1;
            let typed_matches = match observation.route {
                broker_finam::Stage8bP1fO2GetRouteKindV1::Account => {
                    serde_json::from_slice::<AccountResponse>(&observation.exact_response_bytes)
                        .is_ok_and(|value| value == *account)
                }
                broker_finam::Stage8bP1fO2GetRouteKindV1::AccountOrders => {
                    serde_json::from_slice::<AccountOrdersResponse>(
                        &observation.exact_response_bytes,
                    )
                    .is_ok_and(|value| value == *orders)
                }
                broker_finam::Stage8bP1fO2GetRouteKindV1::AssetParams => {
                    serde_json::from_slice::<AssetParamsResponse>(&observation.exact_response_bytes)
                        .is_ok_and(|value| value == *params)
                }
                broker_finam::Stage8bP1fO2GetRouteKindV1::AssetSchedule => {
                    serde_json::from_slice::<AssetScheduleResponse>(
                        &observation.exact_response_bytes,
                    )
                    .is_ok_and(|value| value == *schedule)
                }
                broker_finam::Stage8bP1fO2GetRouteKindV1::Bars => {
                    let Ok(value) =
                        serde_json::from_slice::<BarsResponse>(&observation.exact_response_bytes)
                    else {
                        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
                    };
                    if value.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL {
                        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
                    }
                    for bar in value.bars {
                        match combined_bars.entry(bar.timestamp.clone()) {
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                entry.insert(bar);
                            }
                            std::collections::btree_map::Entry::Occupied(entry)
                                if entry.get() == &bar => {}
                            std::collections::btree_map::Entry::Occupied(_) => {
                                return Err(Stage8bP1fO2MaterializerErrorV1::Get);
                            }
                        }
                    }
                    true
                }
            };
            if !typed_matches {
                return Err(Stage8bP1fO2MaterializerErrorV1::Get);
            }
            Ok(format!("{:?}", observation.route))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if kinds
        != [
            "Account",
            "AccountOrders",
            "AssetParams",
            "AssetSchedule",
            "Bars",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    {
        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
    }
    for kind in ["Account", "AccountOrders", "AssetParams", "AssetSchedule"] {
        if counts.get(kind) != Some(&1) {
            return Err(Stage8bP1fO2MaterializerErrorV1::Get);
        }
    }
    if counts.get("Bars").copied().unwrap_or(0) == 0
        || combined_bars.into_values().collect::<Vec<_>>() != bars.bars
    {
        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
    }
    Ok(())
}

fn map_exact_m1(response: &BarsResponse) -> Result<Vec<Bar>, Stage8bP1fO2MaterializerErrorV1> {
    if response.symbol != STAGE8B_P1F_O2_VENUE_SYMBOL {
        return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
            "mapping",
            "symbol_mismatch",
        ));
    }
    let mut mapped = response
        .bars
        .iter()
        .map(|bar| {
            let mut mapped = map_bar(STAGE8B_P1F_O2_VENUE_SYMBOL, bar, 60).map_err(|_| {
                Stage8bP1fO2MaterializerErrorV1::detailed("mapping", "m1_mapping_rejected")
            })?;
            mapped.instrument.market = Market::Futures;
            mapped.source_kind = MarketDataSourceKind::HistoricalPoll;
            Ok(mapped)
        })
        .collect::<Result<Vec<_>, _>>()?;
    mapped.sort_by_key(|bar| bar.open_ts);
    if mapped
        .windows(2)
        .any(|pair| pair[0].open_ts >= pair[1].open_ts)
    {
        return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
            "mapping",
            "m1_chronology_conflict",
        ));
    }
    Ok(mapped)
}

fn build_history(
    mapped: &[Bar],
    coverage_value: Value,
) -> Result<
    (
        Vec<Value>,
        Vec<strategy_runtime_core::Stage8bP1eFirstBootBarInputV1>,
    ),
    Stage8bP1fO2MaterializerErrorV1,
> {
    let coverage: Vec<CoverageSessionV1> = serde_json::from_value(coverage_value)
        .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
    if coverage.len() < runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS {
        return Err(Stage8bP1fO2MaterializerErrorV1::Template);
    }
    let by_open = mapped
        .iter()
        .map(|bar| (bar.open_ts.timestamp(), bar))
        .collect::<BTreeMap<_, _>>();
    if by_open.len() != mapped.len() {
        return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
            "history",
            "history_duplicate_open",
        ));
    }
    let mut history = Vec::new();
    let mut inputs = Vec::new();
    let mut prior_session = None;
    let mut prior_close = None;
    for session in coverage {
        let session_date = chrono::NaiveDate::parse_from_str(&session.session_date, "%Y-%m-%d")
            .map_err(|_| Stage8bP1fO2MaterializerErrorV1::Template)?;
        if prior_session.is_some_and(|prior| prior >= session_date) || session.windows.is_empty() {
            return Err(Stage8bP1fO2MaterializerErrorV1::Template);
        }
        prior_session = Some(session_date);
        for (window_index, window) in session.windows.into_iter().enumerate() {
            if window.first_close_time_utc <= 0
                || window.first_close_time_utc.rem_euclid(600) != 0
                || window.last_close_time_utc < window.first_close_time_utc
                || window.last_close_time_utc.rem_euclid(600) != 0
                || prior_close.is_some_and(|prior| prior >= window.first_close_time_utc)
            {
                return Err(Stage8bP1fO2MaterializerErrorV1::Template);
            }
            let mut close = window.first_close_time_utc;
            loop {
                let emitted = aggregate_exact_bucket(&by_open, close).map_err(|error| {
                    let mut context = error.context();
                    context.session_date = Some(session_date);
                    context.window_index = Some(window_index);
                    context.window_first_close_utc = Some(window.first_close_time_utc);
                    context.window_last_close_utc = Some(window.last_close_time_utc);
                    context.m10_close_utc = Some(close);
                    Stage8bP1fO2MaterializerErrorV1::Diagnostic(Box::new(context))
                })?;
                if (emitted.close_ts + chrono::Duration::hours(3)).date_naive() != session_date {
                    return Err(Stage8bP1fO2MaterializerErrorV1::Template);
                }
                let input = strategy_runtime_core::Stage8bP1eFirstBootBarInputV1 {
                    close_time_utc: emitted.close_ts.timestamp(),
                    open: emitted.open.normalize().to_string(),
                    high: emitted.high.normalize().to_string(),
                    low: emitted.low.normalize().to_string(),
                    close: emitted.close.normalize().to_string(),
                    volume: emitted.volume.normalize().to_string(),
                };
                history.push(json!({
                    "instrument": STAGE8B_P1F_O2_VENUE_SYMBOL,
                    "timeframe_sec": 600,
                    "close_time_utc": input.close_time_utc,
                    "open": input.open,
                    "high": input.high,
                    "low": input.low,
                    "close": input.close,
                    "volume": input.volume,
                    "is_final": true,
                    "origin": "history"
                }));
                inputs.push(input);
                prior_close = Some(close);
                if close == window.last_close_time_utc {
                    break;
                }
                close = close
                    .checked_add(600)
                    .ok_or(Stage8bP1fO2MaterializerErrorV1::Template)?;
            }
        }
    }
    Ok((history, inputs))
}

fn aggregate_exact_bucket(
    by_open: &BTreeMap<i64, &Bar>,
    close_time_utc: i64,
) -> Result<Bar, Stage8bP1fO2MaterializerErrorV1> {
    let first_open = close_time_utc.checked_sub(600).ok_or_else(|| {
        Stage8bP1fO2MaterializerErrorV1::detailed("history", "history_time_overflow")
    })?;
    let mut aggregator = CanonicalBarAggregator::new(600);
    for index in 0_i64..10 {
        let expected_open = first_open + index * 60;
        let bar = by_open.get(&expected_open).ok_or_else(|| {
            let mut context = Stage8bP1fO2FailureContextV1::new("history", "history_missing_m1");
            context.first_missing_m1_open_utc = Some(expected_open);
            Stage8bP1fO2MaterializerErrorV1::Diagnostic(Box::new(context))
        })?;
        match aggregator.observe_final_source_bar((*bar).clone()) {
            BarAggregationAction::Buffered { buffered_count, .. }
                if index < 9 && buffered_count == usize::try_from(index + 1).unwrap_or(0) => {}
            BarAggregationAction::Emitted { emitted }
                if index == 9 && emitted.close_ts.timestamp() == close_time_utc =>
            {
                return Ok(emitted)
            }
            _ => {
                return Err(Stage8bP1fO2MaterializerErrorV1::detailed(
                    "history",
                    "history_aggregation_rejected",
                ))
            }
        }
    }
    Err(Stage8bP1fO2MaterializerErrorV1::detailed(
        "history",
        "history_aggregation_incomplete",
    ))
}

struct CandidateV1 {
    value: Value,
    redis_id: String,
    semantic_id_sha256: String,
}

#[cfg(test)]
fn build_candidate(
    response: &BarsResponse,
    operational_identity: &str,
    trusted_now: DateTime<Utc>,
) -> Result<CandidateV1, Stage8bP1fO2MaterializerErrorV1> {
    let mapped = map_exact_m1(response)?;
    build_candidate_from_mapped(&mapped, operational_identity, trusted_now)
}

fn build_candidate_from_mapped(
    mapped: &[Bar],
    operational_identity: &str,
    trusted_now: DateTime<Utc>,
) -> Result<CandidateV1, Stage8bP1fO2MaterializerErrorV1> {
    let selected = mapped
        .windows(10)
        .rev()
        .find(|window| {
            window[0].open_ts.timestamp_millis().rem_euclid(600_000) == 0
                && window
                    .windows(2)
                    .all(|pair| pair[0].close_ts == pair[1].open_ts)
                && window[9].close_ts <= trusted_now
        })
        .ok_or_else(|| {
            Stage8bP1fO2MaterializerErrorV1::detailed(
                "candidate",
                "candidate_no_complete_closed_window",
            )
        })?;
    let candidate_error = |stage, code| {
        let mut context = Stage8bP1fO2FailureContextV1::new(stage, code);
        context.candidate_close_utc = Some(selected[9].close_ts.timestamp());
        context.candidate_age_seconds = Some(
            trusted_now
                .signed_duration_since(selected[9].close_ts)
                .num_seconds(),
        );
        Stage8bP1fO2MaterializerErrorV1::Diagnostic(Box::new(context))
    };
    if trusted_now
        .signed_duration_since(selected[9].close_ts)
        .num_seconds()
        > MAX_CANDIDATE_AGE_SECONDS
    {
        return Err(candidate_error("candidate", "candidate_stale"));
    }
    let exact_m1 = selected
        .iter()
        .map(|bar| {
            serde_json::to_vec(bar)
                .map_err(|_| candidate_error("canonical_validation", "m1_serialization_rejected"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_m1 = selected
        .iter()
        .zip(exact_m1.iter())
        .map(
            |(bar, bytes)| runtime_durable_service::Stage8bP1CanonicalM10SourceM1 {
                redis_id: format!("{}-0", bar.close_ts.timestamp_millis()),
                semantic_id_sha256: domain_sha256(M1_SEMANTIC_DOMAIN, bytes),
                payload_sha256: sha256_hex(bytes),
                open_ts_utc_ms: bar.open_ts.timestamp_millis(),
                close_ts_utc_ms: bar.close_ts.timestamp_millis(),
            },
        )
        .collect::<Vec<_>>();
    let mut aggregator = CanonicalBarAggregator::new(600);
    let mut emitted = None;
    for (index, bar) in selected.iter().enumerate() {
        match aggregator.observe_final_source_bar(bar.clone()) {
            BarAggregationAction::Buffered { buffered_count, .. }
                if index < 9 && buffered_count == index + 1 => {}
            BarAggregationAction::Emitted { emitted: bar } if index == 9 => emitted = Some(bar),
            _ => {
                return Err(candidate_error(
                    "candidate",
                    "candidate_aggregation_rejected",
                ))
            }
        }
    }
    let emitted =
        emitted.ok_or_else(|| candidate_error("candidate", "candidate_aggregation_incomplete"))?;
    let canonical = runtime_durable_service::build_stage8b_p1_canonical_m10(
        runtime_durable_service::Stage8bP1CanonicalM10BuildInput {
            operational_identity_sha256: operational_identity.to_string(),
            open_ts_utc_ms: emitted.open_ts.timestamp_millis(),
            close_ts_utc_ms: emitted.close_ts.timestamp_millis(),
            open: emitted.open.normalize().to_string(),
            high: emitted.high.normalize().to_string(),
            low: emitted.low.normalize().to_string(),
            close: emitted.close.normalize().to_string(),
            volume: emitted.volume.normalize().to_string(),
            source_m1: source_m1.clone(),
        },
    )
    .map_err(|_| candidate_error("canonical_validation", "canonical_m10_build_rejected"))?;
    let parsed =
        runtime_durable_service::parse_stage8b_p1_canonical_m10(&canonical, operational_identity)
            .map_err(|_| candidate_error("canonical_validation", "canonical_m10_parse_rejected"))?;
    let redis_id = parsed.redis_id().to_string();
    let semantic_id_sha256 = parsed.semantic_id_sha256().to_string();
    Ok(CandidateV1 {
        value: json!({
            "instrument": STAGE8B_P1F_O2_VENUE_SYMBOL,
            "timeframe_sec": 600,
            "close_time_utc": emitted.close_ts.timestamp(),
            "open": emitted.open.normalize().to_string(),
            "high": emitted.high.normalize().to_string(),
            "low": emitted.low.normalize().to_string(),
            "close": emitted.close.normalize().to_string(),
            "volume": emitted.volume.normalize().to_string(),
            "is_final": true,
            "origin": "replay",
            "redis_id": redis_id.clone(),
            "semantic_id_sha256": semantic_id_sha256.clone(),
            "payload_sha256": parsed.payload_sha256(),
            "open_ts_utc_ms": emitted.open_ts.timestamp_millis(),
            "close_ts_utc_ms": emitted.close_ts.timestamp_millis(),
            "source_m1": source_m1
        }),
        redis_id,
        semantic_id_sha256,
    })
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn parse_canonical_timestamp(
    value: &str,
) -> Result<DateTime<Utc>, Stage8bP1fO2MaterializerErrorV1> {
    let parsed = parse_timestamp(value).ok_or(Stage8bP1fO2MaterializerErrorV1::Get)?;
    if canonical_timestamp(parsed) != value {
        return Err(Stage8bP1fO2MaterializerErrorV1::Get);
    }
    Ok(parsed)
}

fn canonical_timestamp(value: DateTime<Utc>) -> String {
    Utc.timestamp_opt(value.timestamp(), 0)
        .single()
        .expect("valid UTC timestamp")
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_value_sha256(value: &Value) -> String {
    let canonical = canonicalize(value.clone());
    sha256_hex(
        &serde_json::to_vec(&canonical)
            .expect("canonical O2 materialization projection remains serializable"),
    )
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        other => other,
    }
}

fn domain_sha256(domain: &str, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use broker_finam::{dto, Stage8bP1fO2GetRouteKindV1};
    use chrono::{Datelike, NaiveDate, Weekday};

    fn orders(status: &str) -> AccountOrdersResponse {
        serde_json::from_value(json!({
            "orders": [{
                "exec_id": null,
                "executed_quantity": null,
                "initial_quantity": {"value":"1"},
                "order": {
                    "account_id":"ACC_TEST_0001", "client_order_id":null,
                    "comment":null, "legs":[], "limit_price":null,
                    "quantity":{"value":"1"}, "side":"ORDER_SIDE_BUY",
                    "stop_condition":null, "symbol":"IMOEXF@RTSX",
                    "time_in_force":"TIME_IN_FORCE_DAY", "type":"ORDER_TYPE_LIMIT",
                    "valid_before":null
                },
                "order_id":"ORDER-1", "remaining_quantity":{"value":"0"},
                "status":status, "transact_at":null
            }]
        }))
        .unwrap()
    }

    #[test]
    fn complete_terminal_orders_are_accepted_but_active_or_unknown_fail_closed() {
        validate_zero_active_orders("ACC_TEST_0001", &orders("ORDER_STATUS_FILLED")).unwrap();
        for status in [
            "ORDER_STATUS_ACTIVE",
            "ORDER_STATUS_PENDING_CANCEL",
            "NEW_STATUS",
        ] {
            assert!(validate_zero_active_orders("ACC_TEST_0001", &orders(status)).is_err());
        }
    }

    #[test]
    fn fresh_contiguous_bars_build_exact_candidate_and_gaps_fail_closed() {
        let now = Utc
            .with_ymd_and_hms(2026, 9, 27, 12, 11, 0)
            .single()
            .unwrap();
        let bars = (0..10)
            .map(|index| dto::Bar {
                open: dto::DecimalValue {
                    value: format!("{}", 2200 + index),
                },
                high: dto::DecimalValue {
                    value: format!("{}", 2201 + index),
                },
                low: dto::DecimalValue {
                    value: format!("{}", 2199 + index),
                },
                close: dto::DecimalValue {
                    value: format!("{}", 2200 + index),
                },
                volume: dto::DecimalValue { value: "1".into() },
                timestamp: Utc
                    .with_ymd_and_hms(2026, 9, 27, 12, index, 0)
                    .single()
                    .unwrap()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            })
            .collect::<Vec<_>>();
        let response = BarsResponse {
            bars: bars.clone(),
            symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
        };
        let candidate = build_candidate(&response, &"11".repeat(32), now).unwrap();
        assert_eq!(
            candidate.redis_id,
            format!("{}-0", now.timestamp_millis() - 60_000)
        );
        let stale = build_candidate(&response, &"11".repeat(32), now + Duration::seconds(901))
            .err()
            .unwrap()
            .context();
        assert_eq!(stale.reason_code, "candidate_stale");
        assert_eq!(stale.stage, "candidate");
        assert_eq!(stale.candidate_close_utc, Some(now.timestamp() - 60));
        assert_eq!(stale.candidate_age_seconds, Some(961));
        let canonical = build_candidate(&response, "invalid-identity", now)
            .err()
            .unwrap()
            .context();
        assert_eq!(canonical.reason_code, "canonical_m10_build_rejected");
        assert_eq!(canonical.stage, "canonical_validation");
        let mut gapped = bars;
        gapped.remove(4);
        let absent = build_candidate(
            &BarsResponse {
                bars: gapped,
                symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            },
            &"11".repeat(32),
            now,
        )
        .err()
        .unwrap()
        .context();
        assert_eq!(absent.reason_code, "candidate_no_complete_closed_window");
        assert_eq!(absent.candidate_close_utc, None);
    }

    #[test]
    fn collection_diagnostics_are_bounded_redacted_and_count_only_completed_work() {
        let now = Utc.with_ymd_and_hms(2026, 9, 29, 12, 0, 0).unwrap();
        let sentinel = "RAW_TOKEN_ACCOUNT_HEADER_DO_NOT_LOG";
        let mut progress = CollectionProgress {
            gets: 4,
            chunks: 0,
            last_collected: Some("schedule_collected"),
            chunk: Some((0, now.timestamp() - 600, now.timestamp())),
            bars: None,
        };
        let denied = progress
            .error(Stage8bP1fO2MaterializerErrorV1::Get)
            .context();
        assert_eq!(denied.completed_typed_gets, 4);
        assert_eq!(denied.completed_chunks, 0);
        assert_eq!(denied.m1_count, None);
        let bar = dto::Bar {
            timestamp: canonical_timestamp(now - Duration::seconds(60)),
            open: dto::DecimalValue {
                value: "2200".into(),
            },
            high: dto::DecimalValue {
                value: "2201".into(),
            },
            low: dto::DecimalValue {
                value: "2199".into(),
            },
            close: dto::DecimalValue {
                value: "2200".into(),
            },
            volume: dto::DecimalValue { value: "1".into() },
        };
        let mut chunk = BarsResponse {
            symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            bars: vec![bar],
        };
        let mut merged = BTreeMap::new();
        merge_bars_chunk(&mut merged, chunk.clone()).unwrap();
        // Conflicting raw price is never placed in the diagnostic projection.
        chunk.bars[0].close.value = sentinel.repeat(10000);
        progress.gets += 1;
        progress.observe_bars(&chunk);
        let error = progress.error(merge_bars_chunk(&mut merged, chunk.clone()).unwrap_err());
        let output = error.diagnostic_json(
            sentinel,
            &"a".repeat(64),
            &"b".repeat(64),
            sentinel,
            &canonical_timestamp(now),
            now,
        );
        assert!(output.len() <= 4096 && !output.contains(sentinel));
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(json["failure"]["reason_code"], "chunk_duplicate_conflict");
        assert_eq!(json["failure"]["completed_typed_gets"], 5);
        assert_eq!(json["failure"]["completed_chunks"], 0);
        assert_eq!(
            json["failure"]["last_validated_stage"],
            "schedule_collected"
        );
        assert_eq!(json["failure"]["chunk_index"], 0);
        assert_eq!(json["failure"]["m1_count"], 1);
        assert_eq!(
            json["failure"]["first_available_open_utc"],
            now.timestamp() - 60
        );
        assert!(json["manifest_sha256"].is_null());
        assert!(json["requested_bars_start_utc"].is_null());
        chunk.symbol = sentinel.into();
        assert_eq!(
            merge_bars_chunk(&mut merged, chunk)
                .unwrap_err()
                .context()
                .reason_code,
            "chunk_symbol_mismatch"
        );
    }

    #[test]
    fn route_evidence_is_redacted() {
        let observation = Stage8bP1fO2GetObservationV1 {
            route: Stage8bP1fO2GetRouteKindV1::Account,
            request_sha256: "11".repeat(32),
            response_sha256: "22".repeat(32),
            exact_response_bytes: br#"{"account_id":"secret"}"#.to_vec(),
        };
        let evidence = Stage8bP1fO2RouteEvidenceV1 {
            route: format!("{:?}", observation.route),
            request_sha256: observation.request_sha256,
            response_sha256: observation.response_sha256,
        };
        let bytes = serde_json::to_vec(&evidence).unwrap();
        assert!(!bytes.windows(6).any(|window| window == b"secret"));
    }

    #[test]
    fn full_materializer_rebuilds_121_session_history_and_riskgate_from_exact_m1() {
        let operational_identity = "11".repeat(32);
        let account_id = "ACC_TEST_0001";
        let mut day = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let mut coverage = Vec::new();
        let mut bars = Vec::new();
        while coverage.len() < runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_MIN_HISTORY_SESSIONS
        {
            if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
                let first_open = Utc
                    .with_ymd_and_hms(day.year(), day.month(), day.day(), 6, 0, 0)
                    .single()
                    .unwrap();
                for minute in 0_i64..10 {
                    bars.push(dto::Bar {
                        open: dto::DecimalValue {
                            value: "2200".into(),
                        },
                        high: dto::DecimalValue {
                            value: "2201".into(),
                        },
                        low: dto::DecimalValue {
                            value: "2199".into(),
                        },
                        close: dto::DecimalValue {
                            value: "2200".into(),
                        },
                        volume: dto::DecimalValue { value: "1".into() },
                        timestamp: (first_open + chrono::Duration::minutes(minute))
                            .to_rfc3339_opts(SecondsFormat::Secs, true),
                    });
                }
                coverage.push(json!({
                    "session_date": day.format("%Y-%m-%d").to_string(),
                    "windows": [{
                        "first_close_time_utc": (first_open + chrono::Duration::minutes(10)).timestamp(),
                        "last_close_time_utc": (first_open + chrono::Duration::minutes(10)).timestamp()
                    }]
                }));
            }
            day = day.succ_opt().unwrap();
        }
        while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            day = day.succ_opt().unwrap();
        }
        let candidate_open = Utc
            .with_ymd_and_hms(day.year(), day.month(), day.day(), 6, 0, 0)
            .single()
            .unwrap();
        for minute in 0_i64..10 {
            bars.push(dto::Bar {
                open: dto::DecimalValue {
                    value: "2200".into(),
                },
                high: dto::DecimalValue {
                    value: "2201".into(),
                },
                low: dto::DecimalValue {
                    value: "2199".into(),
                },
                close: dto::DecimalValue {
                    value: "2200".into(),
                },
                volume: dto::DecimalValue { value: "1".into() },
                timestamp: (candidate_open + chrono::Duration::minutes(minute))
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            });
        }
        let trusted_now = candidate_open + chrono::Duration::minutes(11);
        let template = json!({
            "schema_version": runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_SCHEMA_VERSION,
            "domain": runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_DOMAIN,
            "operational_identity_sha256": operational_identity,
            "runtime_profile_sha256": runtime_durable_service::STAGE8B_P1E_RUNTIME_PROFILE_SHA256,
            "instrument_map_fingerprint_sha256": runtime_durable_service::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            "source_bundle_generation": 1,
            "captured_at_utc": canonical_timestamp(trusted_now),
            "broker_truth": {"account_id": STAGE8B_P1F_O2_ACCOUNT_TEMPLATE_SENTINEL},
            "history_provenance": {},
            "history_coverage": {
                "source_mode": "config-bound-explicit-session-windows-v1",
                "sessions_sha256": "00".repeat(32),
                "sessions": coverage
            },
            "history_bars": [],
            "riskgate_history": {},
            "candidate": {}
        });
        let account: AccountResponse = serde_json::from_value(json!({
            "account_id": account_id,
            "cash": [], "equity": null, "first_non_trade_date": null,
            "open_account_date": null, "portfolio_mc": null, "positions": [],
            "status": "ACCOUNT_STATUS_OPEN", "type": null, "unrealized_profit": null
        }))
        .unwrap();
        let orders = AccountOrdersResponse { orders: Vec::new() };
        let params: AssetParamsResponse = serde_json::from_value(json!({
            "account_id": account_id, "is_tradable": true,
            "long_collateral": null, "long_initial_margin": null,
            "long_risk_rate": null, "longable": null, "price_type": null,
            "short_collateral": null, "short_initial_margin": null,
            "short_risk_rate": null, "shortable": null,
            "symbol": STAGE8B_P1F_O2_VENUE_SYMBOL, "tradeable": true
        }))
        .unwrap();
        let schedule: AssetScheduleResponse = serde_json::from_value(json!({
            "symbol": STAGE8B_P1F_O2_VENUE_SYMBOL,
            "sessions": [{
                "interval": {
                    "start_time": canonical_timestamp(candidate_open),
                    "end_time": canonical_timestamp(candidate_open + chrono::Duration::hours(12))
                },
                "type": "SESSION_TYPE_MAIN"
            }]
        }))
        .unwrap();
        let bars = BarsResponse {
            bars,
            symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
        };
        let mut gapped = bars.clone();
        let missing = parse_timestamp(&gapped.bars.remove(4).timestamp)
            .unwrap()
            .timestamp();
        let mapped = map_exact_m1(&gapped).unwrap();
        let gap = build_history(&mapped, template["history_coverage"]["sessions"].clone())
            .unwrap_err()
            .context();
        assert_eq!(gap.stage, "history");
        assert_eq!(gap.reason_code, "history_missing_m1");
        assert_eq!(gap.first_missing_m1_open_utc, Some(missing));
        assert_eq!(gap.m10_close_utc, Some(missing + 360));
        assert_eq!(gap.window_index, Some(0));
        assert_eq!(gap.session_date.unwrap().to_string(), "2026-01-05");
        assert_eq!(gap.candidate_close_utc, None); // candidate has not been examined
        gapped.bars[0].timestamp = "secret-invalid-upstream-time".into();
        assert_eq!(
            map_exact_m1(&gapped).unwrap_err().context().reason_code,
            "m1_mapping_rejected"
        );
        let mut duplicate = bars.clone();
        duplicate.bars.push(duplicate.bars[0].clone());
        assert_eq!(
            map_exact_m1(&duplicate).unwrap_err().context().reason_code,
            "m1_chronology_conflict"
        );
        let split = bars.bars.len() / 2;
        let bars_chunks = [
            BarsResponse {
                bars: bars.bars[..split].to_vec(),
                symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            },
            BarsResponse {
                bars: bars.bars[split..].to_vec(),
                symbol: STAGE8B_P1F_O2_VENUE_SYMBOL.into(),
            },
        ];
        let mut exact_responses = vec![
            (
                Stage8bP1fO2GetRouteKindV1::Account,
                serde_json::to_vec(&account).unwrap(),
            ),
            (
                Stage8bP1fO2GetRouteKindV1::AccountOrders,
                serde_json::to_vec(&orders).unwrap(),
            ),
            (
                Stage8bP1fO2GetRouteKindV1::AssetParams,
                serde_json::to_vec(&params).unwrap(),
            ),
            (
                Stage8bP1fO2GetRouteKindV1::AssetSchedule,
                serde_json::to_vec(&schedule).unwrap(),
            ),
        ];
        exact_responses.extend(bars_chunks.iter().map(|chunk| {
            (
                Stage8bP1fO2GetRouteKindV1::Bars,
                serde_json::to_vec(chunk).unwrap(),
            )
        }));
        let observations = exact_responses
            .into_iter()
            .map(
                |(route, exact_response_bytes)| Stage8bP1fO2GetObservationV1 {
                    route,
                    request_sha256: "11".repeat(32),
                    response_sha256: sha256_hex(&exact_response_bytes),
                    exact_response_bytes,
                },
            )
            .collect();
        let materialized = materialize_stage8b_p1f_o2_source_v1(
            &serde_json::to_vec(&template).unwrap(),
            account_id,
            STAGE8B_P1F_O2_ACCOUNT_ALIAS,
            account,
            orders,
            params,
            schedule,
            bars,
            trusted_now,
            observations,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&materialized.exact_source_bytes).unwrap();
        assert_eq!(
            value["broker_truth"]["account_id"],
            STAGE8B_P1F_O2_ACCOUNT_ALIAS
        );
        assert_eq!(
            materialized.evidence.account_id_sha256,
            sha256_hex(account_id.as_bytes())
        );
        assert_eq!(value["history_bars"].as_array().unwrap().len(), 121);
        assert!(
            value["riskgate_history"]["session_observations"]
                .as_array()
                .unwrap()
                .len()
                >= 120
        );
        assert_eq!(materialized.evidence.route_evidence.len(), 6);
        assert_eq!(materialized.evidence.selected_m1_count, 10);
    }
}
