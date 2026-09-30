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
    /// Computed and cross-validated by the durable-service canonical P1 M10
    /// boundary. It is not a free-form source assertion.
    pub validated_candidate_semantic_id_sha256: String,
    pub history_bars: Vec<Stage8bP1eFirstBootBarInputV1>,
    pub riskgate_observations: Vec<Stage8bP1eRiskGateObservationInputV1>,
    pub candidate: Stage8bP1eFirstBootBarInputV1,
}

pub struct Stage8bP1eFirstBootCompositionV1 {
    source: Stage5gTimerReadyPaperStrategy,
    export_input: Stage5gCleanRestartExportInput,
    fresh_runtime: HybridIntradayRuntimeStrategy,
}

/// Rebuilds the source-compatible High180 session observations used by the
/// wire-V2 first-boot bundle. This facade is pure: it has no filesystem,
/// Redis, broker or runtime-effect authority and delegates to the same shadow
/// kernel that the accepted first-boot composition cross-validates.
pub fn rebuild_stage8b_p1e_riskgate_observations_v1(
    runtime: &HybridIntradayRuntimeStrategy,
    history_bars: &[Stage8bP1eFirstBootBarInputV1],
) -> Result<Vec<Stage8bP1eRiskGateObservationInputV1>, Stage8bP1eFirstBootCompositionError> {
    let oracle_bars = history_bars
        .iter()
        .map(history_bar_event)
        .collect::<Result<Vec<_>, _>>()?;
    runtime
        .stage8b_p1_rebuild_riskgate_history(&oracle_bars)
        .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?
        .into_iter()
        .map(|observation| {
            let shadow_pnl_points = crate::hybrid_intraday::format_riskgate_authority_decimal(
                observation.shadow_pnl_points,
            )
            .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
            Ok(Stage8bP1eRiskGateObservationInputV1 {
                session_date: observation.session_date,
                shadow_pnl_points,
                shadow_trade_count: observation.shadow_trade_count,
            })
        })
        .collect()
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
        || !is_sha256_hex(&input.validated_candidate_semantic_id_sha256)
        || input.runtime.stage5c_config_fingerprint()
            != input.fresh_runtime.stage5c_config_fingerprint()
        || input.broker_truth_checked_at > input.captured_at
    {
        return Err(Stage8bP1eFirstBootCompositionError::Identity);
    }
    let riskgate_disabled = input.runtime.stage8b_p1_bo_only_riskgate_disabled();
    if riskgate_disabled
        && (!input.riskgate_observations.is_empty()
            || input.riskgate_session_observations_sha256 != canonical_observations_sha256(&[]))
    {
        return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
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
    .and_then(|history| history.with_strategy_model_bar_start_labels())
    .map_err(|_| Stage8bP1eFirstBootCompositionError::History)?;
    let warmed = crate::stage5c_paper_host::warmup_stage5c_history_at(
        restored,
        accepted_history,
        input.captured_at,
    )
    .map_err(|_| Stage8bP1eFirstBootCompositionError::History)?;

    let (warmed, observations) = if riskgate_disabled {
        // There is no history oracle or riskgate callback in this branch. The
        // empty authority is checked again after Replay and during persistence.
        crate::stage5d_persistence::stage8b_p1_build_disabled_riskgate_authority(
            warmed.stage8b_p1_strategy(),
            STRATEGY_ID,
        )
        .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
        (warmed, None)
    } else {
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
        let pre_candidate_authority =
            crate::stage5d_persistence::stage8b_p1_build_riskgate_authority(
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

        (warmed, Some(observations))
    };

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
    let candidate_model_bar_label_utc = strategy_model_bar_label_utc(&input.candidate)?;
    let candidate_session = moscow_date(candidate_model_bar_label_utc)
        .ok_or(Stage8bP1eFirstBootCompositionError::Candidate)?;
    let history_tail_session = input
        .history_bars
        .last()
        .and_then(|bar| strategy_model_bar_label_utc(bar).ok())
        .and_then(moscow_date)
        .ok_or(Stage8bP1eFirstBootCompositionError::History)?;
    if candidate_session < history_tail_session
        || (!riskgate_disabled && candidate_session == history_tail_session)
        || input.candidate.close_time_utc > input.captured_at.timestamp()
    {
        return Err(Stage8bP1eFirstBootCompositionError::Candidate);
    }
    let accepted_candidate = crate::accept_stage5c_semantic_bar(Stage5cSemanticBarInput {
        bar: candidate_event,
        provenance: Stage3StrategyBarProvenance::finam_derived_m1_to_m10_complete(),
        tick_size: 0.5,
    })
    .map_err(|_| Stage8bP1eFirstBootCompositionError::Candidate)?
    .with_strategy_model_bar_label_utc(candidate_model_bar_label_utc)
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

    let final_authority = if let Some(observations) = observations {
        let authority = crate::stage5d_persistence::stage8b_p1_build_riskgate_authority(
            source.stage5g_runtime_strategy(),
            STRATEGY_ID,
            &observations,
            input.captured_at,
        )
        .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?;
        let candidate_session_text = candidate_session.format("%Y-%m-%d").to_string();
        if authority.current_shadow_session_date() != Some(candidate_session_text.as_str()) {
            return Err(Stage8bP1eFirstBootCompositionError::RiskGate);
        }
        authority
    } else {
        crate::stage5d_persistence::stage8b_p1_build_disabled_riskgate_authority(
            source.stage5g_runtime_strategy(),
            STRATEGY_ID,
        )
        .map_err(|_| Stage8bP1eFirstBootCompositionError::RiskGate)?
    };
    let (riskgate, riskgate_evidence) = final_authority.into_export_parts();
    let snapshot_id = first_boot_snapshot_id(
        &input.operational_identity_sha256,
        &input.validated_candidate_semantic_id_sha256,
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
            persisted_event_watermark: Some(input.validated_candidate_semantic_id_sha256),
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
        close_time_utc: strategy_model_bar_label_utc(bar)?,
        close: parse_decimal(&bar.close)?,
        o: parse_decimal(&bar.open)?,
        h: parse_decimal(&bar.high)?,
        l: parse_decimal(&bar.low)?,
        v: parse_decimal(&bar.volume)?,
        origin: crate::runtime_compat::DataOrigin::History,
    })
}

fn strategy_model_bar_label_utc(
    bar: &Stage8bP1eFirstBootBarInputV1,
) -> Result<i64, Stage8bP1eFirstBootCompositionError> {
    bar.close_time_utc
        .checked_sub(600)
        .ok_or(Stage8bP1eFirstBootCompositionError::History)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_intraday::{
        HybridOrchestratorConfig, IntradayBreakoutConfig, MeanReversionConfig,
    };
    use crate::hybrid_intraday_runtime::{
        HybridIntradayProfile, HybridIntradayRuntimeConfig, MeanReversionVariant, MrGatePolicy,
        RiskGateMode,
    };
    use crate::runtime_compat::Strategy;
    use crate::stage5g_clean_restart::{
        export_stage5g_clean_restart, restore_stage5g_clean_restart, Stage5gCleanRestartSource,
        Stage5gLifecycleCommitmentKey,
    };
    use chrono::{Datelike, Duration, NaiveTime, Weekday};

    fn config(disabled: bool) -> HybridIntradayRuntimeConfig {
        HybridIntradayRuntimeConfig {
            symbol: INTERNAL_SYMBOL.to_string(),
            profile: HybridIntradayProfile::ImoexfPrimaryRiskgateHigh180Lb120,
            mr_variant: MeanReversionVariant::High180,
            live_mr_entries_enabled: false,
            mr_gate_policy: if disabled {
                MrGatePolicy::Disabled
            } else {
                MrGatePolicy::ShadowPnlLb120Positive
            },
            risk_gate_mode: if disabled {
                RiskGateMode::Disabled
            } else {
                RiskGateMode::NormalAppend
            },
            risk_gate_seed_file: None,
            risk_gate_ledger_key: None,
            model_session_start_time: NaiveTime::from_hms_opt(7, 0, 0),
            model_session_end_time: NaiveTime::from_hms_opt(23, 49, 59),
            qty: 1.0,
            live_order_style: crate::BrokerNeutralMarketOrderStyle::Market,
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
            pending_timeout_sec: 60,
            partial_entry_fill_timeout_ms: 3000,
            mr_config: MeanReversionConfig::default(),
            breakout_config: IntradayBreakoutConfig {
                k: 0.53,
                stop1_range: 0.51,
                stop2_range: 0.35,
                big_move_threshold: 0.025,
                min_range: 1.01,
                min_range_mode: crate::hybrid_intraday::MinRangeMode::Absolute,
                exclude_weekends: true,
                wait_hours: 3.0,
            },
            orchestrator_config: HybridOrchestratorConfig::default(),
        }
    }

    fn bar(day: NaiveDate, model_minute: i64) -> Stage8bP1eFirstBootBarInputV1 {
        let model_start =
            day.and_hms_opt(4, 0, 0).unwrap().and_utc() + Duration::minutes(model_minute);
        Stage8bP1eFirstBootBarInputV1 {
            close_time_utc: (model_start + Duration::minutes(10)).timestamp(),
            open: "100".to_string(),
            high: "102".to_string(),
            low: "98".to_string(),
            close: "100".to_string(),
            volume: "10".to_string(),
        }
    }

    fn input_with_config(
        config: HybridIntradayRuntimeConfig,
        sessions: usize,
        prefix: bool,
    ) -> Stage8bP1eFirstBootCompositionInputV1 {
        let mut day = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        let mut history_bars = Vec::new();
        for _ in 0..sessions {
            for minute in (0..=1000).step_by(10) {
                history_bars.push(bar(day, minute));
            }
            day += Duration::days(1);
            while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
                day += Duration::days(1);
            }
        }
        if prefix {
            for minute in (0..180).step_by(10) {
                history_bars.push(bar(day, minute));
            }
        }
        let candidate = bar(day, if prefix { 180 } else { 0 });
        let captured_at = Utc.timestamp_opt(candidate.close_time_utc + 1, 0).unwrap();
        Stage8bP1eFirstBootCompositionInputV1 {
            runtime: HybridIntradayRuntimeStrategy::new(config.clone()),
            fresh_runtime: HybridIntradayRuntimeStrategy::new(config),
            account_id: BrokerAccountId::new("core-disabled-paper"),
            operational_identity_sha256: "11".repeat(32),
            captured_at,
            broker_truth_checked_at: captured_at,
            history_bars_sha256: canonical_history_sha256(&history_bars),
            riskgate_session_observations_sha256: canonical_observations_sha256(&[]),
            validated_candidate_semantic_id_sha256: "22".repeat(32),
            history_bars,
            riskgate_observations: Vec::new(),
            candidate,
        }
    }

    fn input(
        disabled: bool,
        sessions: usize,
        prefix: bool,
    ) -> Stage8bP1eFirstBootCompositionInputV1 {
        input_with_config(config(disabled), sessions, prefix)
    }

    fn add_observations(input: &mut Stage8bP1eFirstBootCompositionInputV1) {
        input.riskgate_observations =
            rebuild_stage8b_p1e_riskgate_observations_v1(&input.runtime, &input.history_bars)
                .unwrap();
        input.riskgate_session_observations_sha256 =
            canonical_observations_sha256(&input.riskgate_observations);
    }

    fn key() -> Stage5gLifecycleCommitmentKey {
        Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x5a; 32]).unwrap()
    }

    fn export(
        input: Stage8bP1eFirstBootCompositionInputV1,
    ) -> (Vec<u8>, HybridIntradayRuntimeStrategy) {
        let (source, export, fresh) = build_stage8b_p1_first_boot_composition_v1(input)
            .unwrap()
            .into_parts();
        let bytes = export_stage5g_clean_restart(
            Stage5gCleanRestartSource::P1BootstrapReady(source),
            export,
            &key(),
        )
        .unwrap();
        (bytes, fresh)
    }

    #[test]
    fn disabled_four_session_first_boot_has_empty_authority_and_roundtrips_without_oracle() {
        let calls = HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls();
        let (source, export, fresh) =
            build_stage8b_p1_first_boot_composition_v1(input(true, 4, false))
                .unwrap()
                .into_parts();
        assert!(export.riskgate_evidence.ledger_records.is_empty());
        assert!(!export.riskgate_evidence.seed_loaded);
        assert_eq!(export.riskgate_evidence.current_shadow_session_date, None);
        assert_eq!(export.riskgate_evidence.current_shadow_pnl_points, "0.0");
        assert_eq!(export.riskgate.materialized_state.ledger_rows_count, 0);
        assert_eq!(export.riskgate.materialized_state.rolling_sum_lb120, None);
        assert_eq!(
            export
                .riskgate
                .materialized_state
                .mr_enabled_current_session,
            None
        );
        assert!(export.riskgate.durable_finalization_outbox.is_empty());
        let state =
            serde_json::to_value(Strategy::state(source.stage5g_runtime_strategy())).unwrap();
        assert_eq!(state["HybridIntradayRuntime"]["entry_ready"], true);
        assert!(state["HybridIntradayRuntime"]["current_owner"].is_null());
        assert!(state["HybridIntradayRuntime"]["pending_entry_request_id"].is_null());
        let expected =
            crate::stage5c_paper_host::stage5c_semantic_value_fingerprint(&state).unwrap();
        let bytes = export_stage5g_clean_restart(
            Stage5gCleanRestartSource::P1BootstrapReady(source),
            export.clone(),
            &key(),
        )
        .unwrap();
        let restored = restore_stage5g_clean_restart(&bytes, &key(), fresh).unwrap();
        assert_eq!(
            restored.reconstructed_runtime_state_fingerprint_sha256(),
            expected
        );
        assert!(!restored.intent_sink_attached());
        assert!(!restored.redis_command_stream_attached());
        assert!(!restored.finam_transport_attached());
        let restored_source = restored.into_stage8b_p1_timer_ready().unwrap();
        let reexported = export_stage5g_clean_restart(
            Stage5gCleanRestartSource::P1BootstrapReady(restored_source),
            export,
            &key(),
        )
        .unwrap();
        let again = restore_stage5g_clean_restart(
            &reexported,
            &key(),
            HybridIntradayRuntimeStrategy::new(config(true)),
        )
        .unwrap();
        assert_eq!(
            again.reconstructed_runtime_state_fingerprint_sha256(),
            expected
        );
        assert_eq!(
            HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls(),
            calls
        );
    }

    #[test]
    fn disabled_current_session_prefix_preserves_model_start_and_close_bound_watermark() {
        let boot_input = input(true, 4, true);
        let candidate_close = boot_input.candidate.close_time_utc;
        let (source, export, _) = build_stage8b_p1_first_boot_composition_v1(boot_input)
            .unwrap()
            .into_parts();
        let state =
            serde_json::to_value(Strategy::state(source.stage5g_runtime_strategy())).unwrap();
        assert!(state["HybridIntradayRuntime"]["today_start_local"]
            .as_str()
            .unwrap()
            .ends_with("07:00:00"));
        assert_eq!(state["HybridIntradayRuntime"]["prev_day_close"], 100.0);
        assert_eq!(state["HybridIntradayRuntime"]["prev_day_range"], 4.0);
        assert_eq!(state["HybridIntradayRuntime"]["prev_day_return"], 0.0);
        assert_eq!(
            export
                .lifecycle_watermarks
                .last_semantic_bar_ts
                .unwrap()
                .timestamp(),
            candidate_close
        );
        for offset in [0, -600] {
            let mut duplicate = input(true, 4, true);
            duplicate.candidate.close_time_utc =
                duplicate.history_bars.last().unwrap().close_time_utc + offset;
            assert!(build_stage8b_p1_first_boot_composition_v1(duplicate).is_err());
        }
    }

    #[test]
    fn disabled_observations_must_be_empty_with_canonical_empty_hash() {
        for supplied in [false, true] {
            let mut input = input(true, 4, false);
            if supplied {
                input
                    .riskgate_observations
                    .push(Stage8bP1eRiskGateObservationInputV1 {
                        session_date: NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
                        shadow_pnl_points: "0.0".to_string(),
                        shadow_trade_count: 0,
                    });
                input.riskgate_session_observations_sha256 =
                    canonical_observations_sha256(&input.riskgate_observations);
            } else {
                input.riskgate_session_observations_sha256 = "aa".repeat(32);
            }
            let calls = HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls();
            assert!(matches!(
                build_stage8b_p1_first_boot_composition_v1(input),
                Err(Stage8bP1eFirstBootCompositionError::RiskGate)
            ));
            assert_eq!(
                HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls(),
                calls
            );
        }
    }

    #[test]
    fn no_riskgate_shortcut_requires_all_three_actual_config_fields() {
        for missing in 0..3 {
            let mut config = config(true);
            match missing {
                0 => config.mr_gate_policy = MrGatePolicy::ShadowPnlLb120Positive,
                1 => config.risk_gate_mode = RiskGateMode::NormalAppend,
                _ => config.live_mr_entries_enabled = true,
            }
            let input = input_with_config(config, 4, false);
            assert!(!input.runtime.stage8b_p1_bo_only_riskgate_disabled());
            let calls = HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls();
            assert!(matches!(
                build_stage8b_p1_first_boot_composition_v1(input),
                Err(Stage8bP1eFirstBootCompositionError::RiskGate)
            ));
            assert_eq!(
                HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls(),
                calls + 1
            );
        }
    }

    #[test]
    fn enabled_first_boot_retains_oracle_history_floor_and_cross_profile_restore_rejection() {
        let mut short = input(false, 4, false);
        add_observations(&mut short);
        assert!(matches!(
            build_stage8b_p1_first_boot_composition_v1(short),
            Err(Stage8bP1eFirstBootCompositionError::RiskGate)
        ));
        let mut legacy = input(false, 121, false);
        add_observations(&mut legacy);
        let calls = HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls();
        let (legacy_bytes, fresh) = export(legacy);
        assert_eq!(
            HybridIntradayRuntimeStrategy::stage8b_p1_test_riskgate_oracle_calls(),
            calls + 1
        );
        assert!(restore_stage5g_clean_restart(&legacy_bytes, &key(), fresh).is_ok());
        assert!(restore_stage5g_clean_restart(
            &legacy_bytes,
            &key(),
            HybridIntradayRuntimeStrategy::new(config(true))
        )
        .is_err());
        let (disabled_bytes, _) = export(input(true, 4, false));
        assert!(restore_stage5g_clean_restart(
            &disabled_bytes,
            &key(),
            HybridIntradayRuntimeStrategy::new(config(false))
        )
        .is_err());
        let mut forged = input(false, 121, false);
        add_observations(&mut forged);
        forged.riskgate_observations[0].shadow_trade_count += 1;
        forged.riskgate_session_observations_sha256 =
            canonical_observations_sha256(&forged.riskgate_observations);
        assert!(matches!(
            build_stage8b_p1_first_boot_composition_v1(forged),
            Err(Stage8bP1eFirstBootCompositionError::RiskGate)
        ));
        let mut same_session = input(false, 121, true);
        add_observations(&mut same_session);
        assert!(matches!(
            build_stage8b_p1_first_boot_composition_v1(same_session),
            Err(Stage8bP1eFirstBootCompositionError::Candidate)
        ));
    }

    #[test]
    fn disabled_restart_rejects_nonempty_semantic_or_materialized_riskgate_even_when_rehashed() {
        use crate::stage5d_persistence::{
            stage5d_decode_canonical_restart_bytes_requiring_stage5g,
            stage5d_reconstruct_runtime_from_clean_restart,
        };
        let (bytes, _) = export(input(true, 4, false));
        let mutations = [
            (
                "/riskgate/durable_finalization_outbox",
                serde_json::json!([{"session_date": "2025-01-09", "generation": 1, "state": "prepared", "identity_hash": "unexpected"}]),
            ),
            (
                "/runtime_private_extension/runtime_pending_finalizations",
                serde_json::json!([{"session_date": "2025-01-09", "shadow_pnl_points": "0.0", "shadow_trade_count": 0}]),
            ),
            ("/riskgate/materialized_state/seed_loaded", serde_json::json!(true)),
            ("/riskgate/materialized_state/rolling_sum_lb120", serde_json::json!("0.0")),
            ("/riskgate/materialized_state/mr_enabled_next_session", serde_json::json!(false)),
            ("/strategy_state/strategy_state_json/HybridIntradayRuntime/risk_gate_rolling_sum_lb120", serde_json::json!(0.0)),
            ("/strategy_state/strategy_state_json/HybridIntradayRuntime/risk_gate_shadow_trade_count", serde_json::json!(1)),
            ("/strategy_state/strategy_state_json/HybridIntradayRuntime/risk_gate_shadow_pnl_points", serde_json::json!(-0.0)),
        ];
        for (pointer, value) in mutations {
            let mut decoded =
                stage5d_decode_canonical_restart_bytes_requiring_stage5g(&bytes).unwrap();
            let mut envelope = serde_json::to_value(&decoded.envelope).unwrap();
            // seed_loaded is omitted when false by the legacy serializer.
            if pointer == "/riskgate/materialized_state/seed_loaded" {
                envelope["riskgate"]["materialized_state"]["seed_loaded"] = value;
            } else {
                *envelope.pointer_mut(pointer).unwrap() = value;
            }
            decoded.envelope = serde_json::from_value(envelope).unwrap();
            decoded.envelope.payload_checksum_sha256 =
                decoded.envelope.compute_payload_checksum_sha256().unwrap();
            // Exercise semantic reconstruction below authentication, so a HMAC
            // rejection cannot hide an accidental empty-authority bypass.
            assert!(
                stage5d_reconstruct_runtime_from_clean_restart(
                    decoded,
                    HybridIntradayRuntimeStrategy::new(config(true))
                )
                .is_err(),
                "{pointer}"
            );
        }
    }

    #[test]
    fn disabled_export_rejects_nonempty_authority_instead_of_normalizing_it() {
        for mutation in 0..5 {
            let (source, mut export, _) =
                build_stage8b_p1_first_boot_composition_v1(input(true, 4, false))
                    .unwrap()
                    .into_parts();
            match mutation {
                0 => export.riskgate.materialized_state.seed_loaded = true,
                1 => export.riskgate.materialized_state.rolling_sum_lb120 = Some("0.0".to_string()),
                2 => {
                    export.riskgate_evidence.current_shadow_session_date =
                        Some("2025-01-10".to_string())
                }
                3 => export.riskgate_evidence.current_shadow_pnl_points = "1.0".to_string(),
                _ => export.riskgate_evidence.seed_loaded = true,
            }
            assert!(
                export_stage5g_clean_restart(
                    Stage5gCleanRestartSource::P1BootstrapReady(source),
                    export,
                    &key()
                )
                .is_err(),
                "mutation {mutation}"
            );
        }
    }
}
