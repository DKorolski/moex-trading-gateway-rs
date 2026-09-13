//! Stage 8B-P1-e deterministic first-boot composition.
//!
//! This module owns F02-F16.  It performs no filesystem, Redis, FINAM or
//! broker effect; the durable service supplies an already authenticated F00
//! source and creates the durable root only after this function succeeds.

use broker_core::{
    BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState, BrokerSymbol,
    BrokerTruthSnapshot, Exchange, InstrumentId, InstrumentMapEntry, InternalSymbol, Market,
    Stage3StrategyBarProvenance, Stage4AdoptionDisposition,
    Stage4BootstrapEvidenceSourceStatusSection, Stage4BrokerTruthBootstrapInput,
    Stage4BrokerTruthFreshnessInput, Stage4BrokerTruthFreshnessProbe,
    Stage4BrokerTruthFreshnessSection, Stage4BrokerTruthSafetyBoundary,
    Stage4BrokerTruthSourceStatus,
};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    runtime_compat::BarEvent, HybridIntradayRuntimeStrategy, Stage5cHistoryBatchInput,
    Stage5cPaperHostAdmissionInput, Stage5cPendingRecoveryClaimProofInput,
    Stage5cPendingRecoveryEvidenceInput, Stage5cPendingStreamClaimBoundary,
    Stage5cPendingStreamKind, Stage5cSemanticBarInput, Stage5dLifecycleWatermarks,
    Stage5gCleanRestartExportInput, Stage5gTimerReadyPaperStrategy,
};

const STRATEGY_ID: &str = "hybrid_imoexf";
const INTERNAL_SYMBOL: &str = "IMOEXF";
const VENUE_SYMBOL: &str = "IMOEXF@RTSX";
const MAX_TRUTH_AGE_MS: u64 = 300_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eFirstBootBarInputV1 {
    pub close_time_utc: i64,
    pub open: String,
    pub high: String,
    pub low: String,
    pub close: String,
    pub volume: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage8bP1eRiskGateObservationInputV1 {
    pub session_date: NaiveDate,
    pub shadow_pnl_points: String,
    pub shadow_trade_count: u32,
}

pub struct Stage8bP1eFirstBootCompositionInputV1 {
    pub runtime: HybridIntradayRuntimeStrategy,
    pub fresh_runtime: HybridIntradayRuntimeStrategy,
    pub account_id: BrokerAccountId,
    pub operational_identity_sha256: String,
    pub captured_at: DateTime<Utc>,
    pub broker_truth_checked_at: DateTime<Utc>,
    pub history_bars_sha256: String,
    pub riskgate_session_observations_sha256: String,
    pub candidate_semantic_id_sha256: String,
    pub history_bars: Vec<Stage8bP1eFirstBootBarInputV1>,
    pub riskgate_observations: Vec<Stage8bP1eRiskGateObservationInputV1>,
    pub candidate: Stage8bP1eFirstBootBarInputV1,
}

pub struct Stage8bP1eFirstBootCompositionV1 {
    source: Stage5gTimerReadyPaperStrategy,
    export_input: Stage5gCleanRestartExportInput,
    fresh_runtime: HybridIntradayRuntimeStrategy,
}

impl Stage8bP1eFirstBootCompositionV1 {
    pub fn into_parts(
        self,
    ) -> (
        Stage5gTimerReadyPaperStrategy,
        Stage5gCleanRestartExportInput,
        HybridIntradayRuntimeStrategy,
    ) {
        (self.source, self.export_input, self.fresh_runtime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Stage8bP1eFirstBootCompositionError {
    #[error("Stage 8B-P1-e first-boot identity is invalid")]
    Identity,
    #[error("Stage 8B-P1-e first-boot Stage 4 admission failed")]
    BrokerTruth,
    #[error("Stage 8B-P1-e first-boot Stage 5C lifecycle failed")]
    Lifecycle,
    #[error("Stage 8B-P1-e first-boot history hash or warmup failed")]
    History,
    #[error("Stage 8B-P1-e first-boot riskgate oracle mismatch")]
    RiskGate,
    #[error("Stage 8B-P1-e first-boot Replay candidate failed")]
    Candidate,
    #[error("Stage 8B-P1-e first-boot clean-restart authority failed")]
    Restart,
}

/// Builds the exact F02-F16 first-boot material from authenticated source
/// observations.  All derived state is recomputed; no network or durable
/// effect is available to this function.
pub fn build_stage8b_p1_first_boot_composition_v1(
    input: Stage8bP1eFirstBootCompositionInputV1,
) -> Result<Stage8bP1eFirstBootCompositionV1, Stage8bP1eFirstBootCompositionError> {
    if input.account_id.as_str().is_empty()
        || !is_sha256_hex(&input.operational_identity_sha256)
        || !is_sha256_hex(&input.history_bars_sha256)
        || !is_sha256_hex(&input.riskgate_session_observations_sha256)
        || !is_sha256_hex(&input.candidate_semantic_id_sha256)
        || input.runtime.stage5c_config_fingerprint()
            != input.fresh_runtime.stage5c_config_fingerprint()
        || input.broker_truth_checked_at > input.captured_at
    {
        return Err(Stage8bP1eFirstBootCompositionError::Identity);
    }
    let instrument = instrument_id();
    let instrument_spec = instrument_spec();
    let truth = BrokerTruthSnapshot {
        account_id: input.account_id.clone(),
        orders: Vec::new(),
        positions: Vec::new(),
        cash: None,
        trades: Vec::new(),
        instruments: vec![instrument_spec.clone()],
        received_ts: input.broker_truth_checked_at,
    };
    let fresh = |required| {
        Stage4BrokerTruthFreshnessProbe::fresh(
            input.broker_truth_checked_at,
            MAX_TRUTH_AGE_MS,
            required,
        )
    };
    let validated =
        broker_core::validate_stage4_broker_truth_bootstrap(Stage4BrokerTruthBootstrapInput {
            broker_truth: &truth,
            broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
            target_instrument: instrument.clone(),
            restored_runtime_state: None,
            freshness: Stage4BrokerTruthFreshnessInput {
                positions: fresh(true),
                orders: fresh(true),
                trades: fresh(false),
                cash: fresh(false),
                instruments: fresh(true),
                schedule: fresh(true),
            },
            schedule_state: BrokerMarketSessionState::Open,
            adoption: Stage4AdoptionDisposition::default(),
            external_issues: Vec::new(),
            safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
            checked_ts: input.captured_at,
        });
    let source_sections = [
        (Stage4BrokerTruthFreshnessSection::Positions, true),
        (Stage4BrokerTruthFreshnessSection::Orders, true),
        (Stage4BrokerTruthFreshnessSection::Trades, false),
        (Stage4BrokerTruthFreshnessSection::Cash, false),
        (Stage4BrokerTruthFreshnessSection::Instruments, true),
        (Stage4BrokerTruthFreshnessSection::Schedule, true),
    ]
    .into_iter()
    .map(
        |(section, required_for_bootstrap)| Stage4BootstrapEvidenceSourceStatusSection {
            section,
            source_status: Stage4BrokerTruthSourceStatus::Present,
            required_for_bootstrap,
        },
    )
    .collect::<Vec<_>>();
    let evidence =
        broker_core::build_stage4_accepted_paper_host_evidence(&validated, &source_sections)
            .map_err(|_| Stage8bP1eFirstBootCompositionError::BrokerTruth)?;
    let admission = crate::stage5c_paper_host::admit_stage5c_paper_host_at(
        Stage5cPaperHostAdmissionInput {
            stage4_evidence: evidence,
            strategy_id: STRATEGY_ID.to_string(),
            instrument_spec: &instrument_spec,
            configured_account_id: &input.account_id,
            configured_target_instrument: &instrument,
            configured_tick_size: 0.5,
            allow_live_orders: false,
        },
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::BrokerTruth)?;
    let loaded = crate::prepare_stage5c_without_runtime_state(input.runtime, admission);
    let bootstrapped =
        crate::stage5c_paper_host::notify_stage5c_bootstrap_at(loaded, input.captured_at)
            .map_err(|_| Stage8bP1eFirstBootCompositionError::Lifecycle)?;
    let restored = crate::stage5c_paper_host::notify_stage5c_runtime_state_restored_at(
        bootstrapped,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Lifecycle)?;

    if canonical_history_sha256(&input.history_bars) != input.history_bars_sha256 {
        return Err(Stage8bP1eFirstBootCompositionError::History);
    }
    let history_events = input
        .history_bars
        .iter()
        .map(|bar| runtime_bar(bar, broker_core::HybridRuntimeBarOrigin::History))
        .collect::<Result<Vec<_>, _>>()?;
    let accepted_history = crate::accept_stage5c_history_batch(Stage5cHistoryBatchInput {
        bars: history_events,
        provenance: Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
    })
    .map_err(|_| Stage8bP1eFirstBootCompositionError::History)?;
    let warmed = crate::stage5c_paper_host::warmup_stage5c_history_at(
        restored,
        accepted_history,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::History)?;

    let oracle_bars = input
        .history_bars
        .iter()
        .map(history_bar_event)
        .collect::<Result<Vec<_>, _>>()?;
    let oracle = warmed
        .stage8b_p1_rebuild_riskgate_history(&oracle_bars)
        .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
    let observations = validate_riskgate_observations(
        &oracle,
        &input.riskgate_observations,
        &input.riskgate_session_observations_sha256,
    )?;
    let pre_candidate_authority = crate::stage5d_persistence::stage8b_p1_build_riskgate_authority(
        warmed.stage8b_p1_strategy(),
        STRATEGY_ID,
        &observations,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
    if pre_candidate_authority
        .current_shadow_session_date()
        .is_some()
        || pre_candidate_authority.current_shadow_pnl_points() != "0.0"
    {
        return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
    }
    let warmed = crate::stage5c_paper_host::stage8b_p1_apply_riskgate_to_warmed(
        warmed,
        pre_candidate_authority.runtime_state(),
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;

    let group = format!("paper-runtime:{}:{STRATEGY_ID}", input.account_id);
    let streams = [
        (Stage5cPendingStreamKind::Ack, "cmd.acks"),
        (Stage5cPendingStreamKind::Order, "broker.orders"),
        (Stage5cPendingStreamKind::StopOrder, "broker.stop_orders"),
        (Stage5cPendingStreamKind::Position, "broker.positions"),
    ]
    .into_iter()
    .map(|(stream_kind, prefix)| Stage5cPendingStreamClaimBoundary {
        stream_kind,
        stream_name: format!("{prefix}.{}", input.account_id),
        consumer_group: group.clone(),
        terminal_claim_cursor: "0-0".to_string(),
        snapshot_boundary_entry_id: "0-0".to_string(),
        claimed_count: 0,
    })
    .collect();
    let claim = crate::prove_stage5c_pending_recovery_claim(
        &warmed,
        Stage5cPendingRecoveryClaimProofInput {
            strategy_id: STRATEGY_ID.to_string(),
            account_id: input.account_id.clone(),
            target_instrument: instrument.clone(),
            snapshot_received_ts: input.broker_truth_checked_at,
            completed_ts: input.captured_at,
            streams,
        },
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Lifecycle)?;
    let pending =
        crate::accept_stage5c_pending_recovery_evidence(Stage5cPendingRecoveryEvidenceInput {
            events: Vec::new(),
            claim_proof: claim,
        })
        .map_err(|_| Stage8bP1eFirstBootCompositionError::Lifecycle)?;
    let recovered = crate::stage5c_paper_host::recover_stage5c_pending_streams_at(
        warmed,
        pending,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Lifecycle)?;

    let candidate_event = runtime_bar(
        &input.candidate,
        broker_core::HybridRuntimeBarOrigin::Replay,
    )?;
    let candidate_session = moscow_date(input.candidate.close_time_utc)
        .ok_or(Stage8bP1eFirstBootCompositionError::Candidate)?;
    let history_tail_session = input
        .history_bars
        .last()
        .and_then(|bar| moscow_date(bar.close_time_utc))
        .ok_or(Stage8bP1eFirstBootCompositionError::History)?;
    if candidate_session <= history_tail_session
        || input.candidate.close_time_utc > input.captured_at.timestamp()
    {
        return Err(Stage8bP1eFirstBootCompositionError::Candidate);
    }
    let accepted_candidate = crate::accept_stage5c_semantic_bar(Stage5cSemanticBarInput {
        bar: candidate_event,
        provenance: Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
        tick_size: 0.5,
    })
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Candidate)?;
    let result = crate::stage5c_paper_host::stage8b_p1_apply_first_replay_bar_at(
        recovered,
        accepted_candidate,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Candidate)?;
    if result.captured_intent_count() != 0
        || result.origin() != broker_core::HybridRuntimeBarOrigin::Replay
        || result.execution_eligible()
    {
        return Err(Stage8bP1eFirstBootCompositionError::Candidate);
    }
    let settled = crate::settle_stage5c_semantic_result(result)
        .map_err(|_| Stage8bP1eFirstBootCompositionError::Candidate)?;
    let checkpoint_ts_utc_ms = input
        .candidate
        .close_time_utc
        .checked_mul(1_000)
        .ok_or(Stage8bP1eFirstBootCompositionError::Candidate)?;
    let source = crate::stage5g_timer::attach_stage8b_p1_initial_replay_timer_ready(
        settled,
        checkpoint_ts_utc_ms,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Candidate)?;

    let final_authority = crate::stage5d_persistence::stage8b_p1_build_riskgate_authority(
        source.stage5g_runtime_strategy(),
        STRATEGY_ID,
        &observations,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
    let candidate_session_text = candidate_session.format("%Y-%m-%d").to_string();
    if final_authority.current_shadow_session_date() != Some(candidate_session_text.as_str()) {
        return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
    }
    let (riskgate, riskgate_evidence) = final_authority.into_export_parts();
    let snapshot_id = first_boot_snapshot_id(
        &input.operational_identity_sha256,
        &input.candidate_semantic_id_sha256,
    );
    let export_input = Stage5gCleanRestartExportInput {
        snapshot_id,
        snapshot_revision: 1,
        previous_revision: None,
        write_generation: 1,
        persisted_at_ts_utc: input.captured_at,
        source_commit_or_build_id:
            crate::stage5d_persistence::STAGE5D_RUNTIME_SEMANTIC_COMPATIBILITY_ID.to_string(),
        lifecycle_watermarks: Stage5dLifecycleWatermarks {
            persisted_event_watermark: Some(input.candidate_semantic_id_sha256),
            last_semantic_bar_ts: Utc
                .timestamp_opt(input.candidate.close_time_utc, 0)
                .single(),
            last_broker_event_ts: None,
        },
        riskgate,
        riskgate_evidence,
    };
    Ok(Stage8bP1eFirstBootCompositionV1 {
        source,
        export_input,
        fresh_runtime: input.fresh_runtime,
    })
}

fn instrument_id() -> InstrumentId {
    InstrumentId {
        symbol: INTERNAL_SYMBOL.to_string(),
        venue_symbol: Some(VENUE_SYMBOL.to_string()),
        exchange: Exchange::Moex,
        market: Market::Futures,
    }
}

fn instrument_spec() -> BrokerInstrumentSpec {
    BrokerInstrumentSpec {
        instrument: InstrumentMapEntry {
            internal_symbol: InternalSymbol(INTERNAL_SYMBOL.to_string()),
            broker: BrokerKind::Finam,
            broker_symbol: BrokerSymbol(VENUE_SYMBOL.to_string()),
            exchange: Exchange::Moex,
            market: Market::Futures,
            price_step: Decimal::new(5, 1),
            qty_step: Decimal::ONE,
            lot_size: Decimal::ONE,
            min_qty: Decimal::ONE,
            step_value: Decimal::new(5, 0),
            currency: "RUB".to_string(),
            schedule_id: "RTSX".to_string(),
            expiration_date: None,
            is_tradable: true,
        },
        broker_asset_id: Some("IMOEXF@RTSX".to_string()),
        board: Some("RTSX".to_string()),
        long_initial_margin: None,
        short_initial_margin: None,
    }
}

fn parse_decimal(value: &str) -> Result<f64, Stage8bP1eFirstBootCompositionError> {
    Decimal::from_str_exact(value)
        .ok()
        .and_then(|value| value.to_f64())
        .filter(|value| value.is_finite())
        .ok_or(Stage8bP1eFirstBootCompositionError::History)
}

fn runtime_bar(
    bar: &Stage8bP1eFirstBootBarInputV1,
    origin: broker_core::HybridRuntimeBarOrigin,
) -> Result<broker_core::HybridRuntimeBarEvent, Stage8bP1eFirstBootCompositionError> {
    Ok(broker_core::HybridRuntimeBarEvent {
        instrument: instrument_id(),
        close_time_utc: bar.close_time_utc,
        open: parse_decimal(&bar.open)?,
        high: parse_decimal(&bar.high)?,
        low: parse_decimal(&bar.low)?,
        close: parse_decimal(&bar.close)?,
        volume: parse_decimal(&bar.volume)?,
        origin,
        is_final: true,
        timeframe_sec: 600,
    })
}

fn history_bar_event(
    bar: &Stage8bP1eFirstBootBarInputV1,
) -> Result<BarEvent, Stage8bP1eFirstBootCompositionError> {
    Ok(BarEvent {
        symbol: INTERNAL_SYMBOL.to_string(),
        close_time_utc: bar.close_time_utc,
        close: parse_decimal(&bar.close)?,
        o: parse_decimal(&bar.open)?,
        h: parse_decimal(&bar.high)?,
        l: parse_decimal(&bar.low)?,
        v: parse_decimal(&bar.volume)?,
        origin: crate::runtime_compat::DataOrigin::History,
    })
}

fn validate_riskgate_observations(
    oracle: &[crate::runtime_compat::RiskGateSessionFinalization],
    source: &[Stage8bP1eRiskGateObservationInputV1],
    expected_hash: &str,
) -> Result<Vec<(NaiveDate, f64, u32)>, Stage8bP1eFirstBootCompositionError> {
    if oracle.len() != source.len() || canonical_observations_sha256(source) != expected_hash {
        return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
    }
    oracle
        .iter()
        .zip(source)
        .map(|(rebuilt, supplied)| {
            let supplied_pnl = crate::hybrid_intraday::parse_riskgate_authority_decimal(
                &supplied.shadow_pnl_points,
            )
            .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
            let rebuilt_text = crate::hybrid_intraday::format_riskgate_authority_decimal(
                rebuilt.shadow_pnl_points,
            )
            .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
            if rebuilt.session_date != supplied.session_date
                || rebuilt_text != supplied.shadow_pnl_points
                || rebuilt.shadow_pnl_points.to_bits() != supplied_pnl.to_bits()
                || rebuilt.shadow_trade_count != supplied.shadow_trade_count
            {
                return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
            }
            Ok((
                supplied.session_date,
                supplied_pnl,
                supplied.shadow_trade_count,
            ))
        })
        .collect()
}

#[derive(Serialize)]
struct HistoryProjection<'a> {
    instrument: &'static str,
    timeframe_sec: u32,
    close_time_utc: i64,
    open: &'a str,
    high: &'a str,
    low: &'a str,
    close: &'a str,
    volume: &'a str,
    is_final: bool,
    origin: &'static str,
}

#[derive(Serialize)]
struct ObservationProjection<'a> {
    session_date: String,
    shadow_pnl_points: &'a str,
    shadow_trade_count: u32,
}

fn canonical_history_sha256(bars: &[Stage8bP1eFirstBootBarInputV1]) -> String {
    let values = bars
        .iter()
        .map(|bar| HistoryProjection {
            instrument: VENUE_SYMBOL,
            timeframe_sec: 600,
            close_time_utc: bar.close_time_utc,
            open: &bar.open,
            high: &bar.high,
            low: &bar.low,
            close: &bar.close,
            volume: &bar.volume,
            is_final: true,
            origin: "history",
        })
        .collect::<Vec<_>>();
    canonical_sha256(&values)
}

fn canonical_observations_sha256(observations: &[Stage8bP1eRiskGateObservationInputV1]) -> String {
    let values = observations
        .iter()
        .map(|value| ObservationProjection {
            session_date: value.session_date.format("%Y-%m-%d").to_string(),
            shadow_pnl_points: &value.shadow_pnl_points,
            shadow_trade_count: value.shadow_trade_count,
        })
        .collect::<Vec<_>>();
    canonical_sha256(&values)
}

fn canonical_sha256<T: Serialize>(value: &T) -> String {
    let value = serde_json::to_value(value).expect("typed first-boot projection serializes");
    let canonical = canonicalize(value);
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&canonical).expect("canonical first-boot projection serializes")
        )
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

fn first_boot_snapshot_id(operational_identity: &str, candidate_semantic_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"moex.stage8b.p1e.first-boot.snapshot.v1\0");
    hasher.update(operational_identity.as_bytes());
    hasher.update(b"\0");
    hasher.update(candidate_semantic_id.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn moscow_date(timestamp: i64) -> Option<NaiveDate> {
    Utc.timestamp_opt(timestamp, 0)
        .single()?
        .checked_add_signed(chrono::Duration::hours(3))
        .map(|value| value.date_naive())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
