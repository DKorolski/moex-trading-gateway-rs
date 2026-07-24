//! Stage 5E-b1.1's private, observation-only live-bar-after-history boundary.
//!
//! This module intentionally has no public API. Its linear capability proves a
//! contextually valid `Live` bar after canonical history; it does not claim
//! market-gap continuity or any callback-ready authorization.

use chrono::{DateTime, NaiveDate, Utc};
use sha2::{Digest, Sha256};

use crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy;
use crate::stage5c_paper_host::{Stage5cPendingRecoveryReceipt, Stage5eNoIoBridgeSeal};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eContextualAdmissionError {
    NotLive,
    NotStrictlyAfterHistory,
    InstrumentMismatch,
    TickSizeMismatch,
    AdmissionExpired,
    FutureBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage5eSessionObservationError {
    ScheduleNotOpen,
    ScheduleNotFresh,
    InvalidObservedWindow,
    BarOutsideObservedOpenWindow,
}

// STAGE5E-NO-IO-VALIDATOR-BEGIN: contextual-admission-v1
#[allow(clippy::too_many_arguments)] // Pure validation keeps the independent context bindings explicit.
pub(crate) fn validate_contextual_live_bar_after_history(
    origin: broker_core::HybridRuntimeBarOrigin,
    bar_instrument: &broker_core::InstrumentId,
    target_instrument: &broker_core::InstrumentId,
    bar_tick_size: f64,
    admission_tick_size: f64,
    last_history_bar_close: i64,
    bar_close: i64,
    admission_expires_at: DateTime<Utc>,
    lifecycle_now: DateTime<Utc>,
) -> Result<(), Stage5eContextualAdmissionError> {
    if origin != broker_core::HybridRuntimeBarOrigin::Live {
        return Err(Stage5eContextualAdmissionError::NotLive);
    }
    if bar_instrument != target_instrument {
        return Err(Stage5eContextualAdmissionError::InstrumentMismatch);
    }
    // Both values are issued by the canonical Stage 5C admission/data contract;
    // exact IEEE-754 bits intentionally reject independently recomputed values.
    if bar_tick_size.to_bits() != admission_tick_size.to_bits() {
        return Err(Stage5eContextualAdmissionError::TickSizeMismatch);
    }
    if lifecycle_now > admission_expires_at {
        return Err(Stage5eContextualAdmissionError::AdmissionExpired);
    }
    if bar_close > lifecycle_now.timestamp() {
        return Err(Stage5eContextualAdmissionError::FutureBar);
    }
    if bar_close <= last_history_bar_close {
        return Err(Stage5eContextualAdmissionError::NotStrictlyAfterHistory);
    }
    Ok(())
}
// STAGE5E-NO-IO-VALIDATOR-END: contextual-admission-v1

pub(crate) struct Stage5eObservedLiveBarAfterHistory {
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
}

// STAGE5E-NO-IO-CAPABILITY-PROOF-BEGIN: zero-side-effects-v1
impl Stage5eObservedLiveBarAfterHistory {
    pub(crate) fn from_stage5c_context(
        _seal: Stage5eNoIoBridgeSeal,
        strategy: HybridIntradayRuntimeStrategy,
        recovery_receipt: Stage5cPendingRecoveryReceipt,
        bar: broker_core::HybridRuntimeBarEvent,
        tick_size: f64,
    ) -> Self {
        Self {
            strategy,
            recovery_receipt,
            bar,
            tick_size,
        }
    }
}

impl Stage5eObservedLiveBarAfterHistory {
    pub(crate) fn bar_close_ts(&self) -> i64 {
        self.bar.close_time_utc
    }

    pub(crate) fn callback_count(&self) -> usize {
        0
    }

    pub(crate) fn intent_count(&self) -> usize {
        0
    }

    pub(crate) fn strategy_was_called(&self) -> bool {
        false
    }

    pub(crate) fn executable_intent_created(&self) -> bool {
        false
    }
}
// STAGE5E-NO-IO-CAPABILITY-PROOF-END: zero-side-effects-v1

// STAGE5E-NO-IO-SESSION-ELIGIBILITY-BEGIN: observed-open-session-v1
mod session_eligibility {
    use super::{DateTime, Stage5eSessionObservationError, Utc};

    /// A linear, fresh observation that the candidate bar belongs to an explicitly
    /// observed open session. This is not a calendar implementation: its interval
    /// is supplied by a later schedule mapper and is rejected when stale, unknown,
    /// or not open. `until` is the last allowed bar-close, so both bounds are
    /// intentionally inclusive.
    #[derive(Debug, PartialEq, Eq)]
    pub(super) struct Stage5eObservedOpenSession {
        bar_close_ts: i64,
    }

    pub(super) fn observe_open_session_for_bar(
        session_state: broker_core::BrokerMarketSessionState,
        schedule_freshness: broker_core::Stage4BrokerTruthFreshnessProbe,
        observed_open_from_bar_close: i64,
        observed_open_until_bar_close: i64,
        bar_close_ts: i64,
        lifecycle_now: DateTime<Utc>,
    ) -> Result<Stage5eObservedOpenSession, Stage5eSessionObservationError> {
        if session_state != broker_core::BrokerMarketSessionState::Open {
            return Err(Stage5eSessionObservationError::ScheduleNotOpen);
        }
        let Some(observed_ts) = schedule_freshness.observed_ts else {
            return Err(Stage5eSessionObservationError::ScheduleNotFresh);
        };
        let schedule_age_ms = lifecycle_now
            .signed_duration_since(observed_ts)
            .num_milliseconds();
        if !schedule_freshness.available
            || schedule_age_ms < 0
            || schedule_age_ms as u64 > schedule_freshness.max_age_ms
        {
            return Err(Stage5eSessionObservationError::ScheduleNotFresh);
        }
        if observed_open_from_bar_close >= observed_open_until_bar_close {
            return Err(Stage5eSessionObservationError::InvalidObservedWindow);
        }
        if bar_close_ts < observed_open_from_bar_close
            || bar_close_ts > observed_open_until_bar_close
        {
            return Err(Stage5eSessionObservationError::BarOutsideObservedOpenWindow);
        }
        Ok(Stage5eObservedOpenSession { bar_close_ts })
    }

    impl Stage5eObservedOpenSession {
        pub(super) fn bar_close_ts(&self) -> i64 {
            self.bar_close_ts
        }

        pub(super) fn callback_count(&self) -> usize {
            0
        }

        pub(super) fn intent_count(&self) -> usize {
            0
        }

        pub(super) fn strategy_was_called(&self) -> bool {
            false
        }

        pub(super) fn executable_intent_created(&self) -> bool {
            false
        }
    }
}
// STAGE5E-NO-IO-SESSION-ELIGIBILITY-END: observed-open-session-v1

// STAGE5E-B3-SCHEDULE-WINDOW-BEGIN: sealed-contract-v1
#[allow(dead_code)] // Stage 5E-b3a contract precedes its separately reviewed mapper bridge.
mod schedule_window_evidence {
    use super::{DateTime, Digest, NaiveDate, Sha256, Utc};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MarketBarCloseTime(i64);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct LifecycleInstant(DateTime<Utc>);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TradingDay(NaiveDate);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ScheduleSourceIdentity {
        BrokerReported,
        ExchangeCalendar,
        BrokerNeutralStaticRegistry,
        ReconciledBrokerAndExchange,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum NormalizedSessionType {
        TradableOpen,
        BreakOrClearing,
        Maintenance,
        Unknown,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct NormalizedScheduleSession {
        session_type: NormalizedSessionType,
        start: MarketBarCloseTime,
        end: MarketBarCloseTime,
    }

    /// Upstream read-only adapter DTO. This type is deliberately private to
    /// Stage 5E until a separately reviewed FINAM normalizer produces it.
    struct NormalizedInstrumentScheduleSnapshot {
        instrument: broker_core::InstrumentId,
        broker_symbol: String,
        venue_mic: String,
        board: String,
        sessions: Vec<NormalizedScheduleSession>,
        source_contract_version: String,
        source_observed_at: LifecycleInstant,
        source_expires_at: LifecycleInstant,
        raw_response_sha256: [u8; 32],
        normalized_payload_sha256: [u8; 32],
        instrument_registry_version: String,
    }

    enum NormalizedScheduleAvailability {
        Available(NormalizedInstrumentScheduleSnapshot),
        ScheduleSourceUnavailable,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ScheduleFingerprint([u8; 32]);

    enum Stage4ScheduleProjectionError {
        NotAccepted,
        SourceUnavailable,
        FreshnessUnavailable,
        ObservedInFuture,
        Expired,
    }

    enum ScheduleWindowMappingError {
        InstrumentMismatch,
        InvalidWindow,
    }

    struct AcceptedStage4ScheduleEvidence {
        instrument: broker_core::InstrumentId,
        observed_at: LifecycleInstant,
        expires_at: LifecycleInstant,
        identity: String,
    }

    /// Private mapper input. A later b3a bridge may construct it only from
    /// accepted Stage 4 evidence and an approved broker-neutral definition.
    struct TrustedScheduleDefinition {
        instrument: broker_core::InstrumentId,
        venue: String,
        trading_day: TradingDay,
        open_from: MarketBarCloseTime,
        open_until: MarketBarCloseTime,
        source: ScheduleSourceIdentity,
        source_version: String,
        schedule_epoch: u64,
    }

    struct Stage5eScheduleWindowEvidence {
        definition: TrustedScheduleDefinition,
        stage4_identity: String,
        observed_at: LifecycleInstant,
        expires_at: LifecycleInstant,
        fingerprint: ScheduleFingerprint,
    }

    fn project_accepted_stage4_schedule(
        evidence: &broker_core::Stage4AcceptedPaperHostEvidence,
        lifecycle_now: LifecycleInstant,
    ) -> Result<AcceptedStage4ScheduleEvidence, Stage4ScheduleProjectionError> {
        let report = evidence.report();
        if report.status != broker_core::Stage4BootstrapEvidenceReportStatus::Accepted {
            return Err(Stage4ScheduleProjectionError::NotAccepted);
        }
        let schedule = report
            .source_sections
            .iter()
            .find(|section| {
                section.section == broker_core::Stage4BrokerTruthFreshnessSection::Schedule
            })
            .ok_or(Stage4ScheduleProjectionError::SourceUnavailable)?;
        if schedule.source_status != broker_core::Stage4BrokerTruthSourceStatus::Present {
            return Err(Stage4ScheduleProjectionError::SourceUnavailable);
        }
        if schedule.freshness_status != broker_core::Stage4BrokerTruthFreshnessStatus::Fresh {
            return Err(Stage4ScheduleProjectionError::FreshnessUnavailable);
        }
        let age_ms = schedule
            .age_ms
            .ok_or(Stage4ScheduleProjectionError::FreshnessUnavailable)?;
        if age_ms < 0 {
            return Err(Stage4ScheduleProjectionError::ObservedInFuture);
        }
        if lifecycle_now.0 > evidence.required_source_expires_at() {
            return Err(Stage4ScheduleProjectionError::Expired);
        }
        let observed_at = report.checked_ts - chrono::Duration::milliseconds(age_ms);
        let identity = format!(
            "stage4-schedule-v1:{}:{}:{}",
            report.schema_version,
            report.checked_ts.timestamp_millis(),
            report.target_instrument.symbol,
        );
        Ok(AcceptedStage4ScheduleEvidence {
            instrument: report.target_instrument.clone(),
            observed_at: LifecycleInstant(observed_at),
            expires_at: LifecycleInstant(evidence.required_source_expires_at()),
            identity,
        })
    }

    fn map_trusted_schedule_window(
        stage4: AcceptedStage4ScheduleEvidence,
        definition: TrustedScheduleDefinition,
    ) -> Result<Stage5eScheduleWindowEvidence, ScheduleWindowMappingError> {
        if definition.instrument != stage4.instrument {
            return Err(ScheduleWindowMappingError::InstrumentMismatch);
        }
        if definition.open_from.0 >= definition.open_until.0 {
            return Err(ScheduleWindowMappingError::InvalidWindow);
        }
        let fingerprint = deterministic_fingerprint(&definition, &stage4.identity);
        Ok(Stage5eScheduleWindowEvidence {
            definition,
            stage4_identity: stage4.identity,
            observed_at: stage4.observed_at,
            expires_at: stage4.expires_at,
            fingerprint,
        })
    }

    fn deterministic_fingerprint(
        definition: &TrustedScheduleDefinition,
        stage4_identity: &str,
    ) -> ScheduleFingerprint {
        let mut hasher = Sha256::new();
        hasher.update(b"stage5e-schedule-window-evidence-v1\0");
        hasher.update(definition.instrument.symbol.as_bytes());
        hasher.update(b"\0");
        hasher.update(definition.venue.as_bytes());
        hasher.update(b"\0");
        hasher.update(definition.trading_day.0.to_string().as_bytes());
        hasher.update(definition.open_from.0.to_be_bytes());
        hasher.update(definition.open_until.0.to_be_bytes());
        hasher.update([definition.source as u8]);
        hasher.update(definition.source_version.as_bytes());
        hasher.update(definition.schedule_epoch.to_be_bytes());
        hasher.update(stage4_identity.as_bytes());
        ScheduleFingerprint(hasher.finalize().into())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use broker_core::{Exchange, InstrumentId, Market};

        fn definition(symbol: &str, open_from: i64, open_until: i64) -> TrustedScheduleDefinition {
            TrustedScheduleDefinition {
                instrument: InstrumentId {
                    symbol: symbol.to_string(),
                    venue_symbol: Some(format!("{symbol}@RTSX")),
                    exchange: Exchange::Moex,
                    market: Market::Futures,
                },
                venue: "RTSX".to_string(),
                trading_day: TradingDay(NaiveDate::from_ymd_opt(2026, 7, 24).unwrap()),
                open_from: MarketBarCloseTime(open_from),
                open_until: MarketBarCloseTime(open_until),
                source: ScheduleSourceIdentity::BrokerNeutralStaticRegistry,
                source_version: "fixture-v1".to_string(),
                schedule_epoch: 1,
            }
        }

        #[test]
        fn fingerprint_is_deterministic_and_covers_window_identity() {
            let base = definition("IMOEXF", 100, 200);
            assert_eq!(
                deterministic_fingerprint(&base, "stage4-a"),
                deterministic_fingerprint(&base, "stage4-a")
            );
            assert_ne!(
                deterministic_fingerprint(&base, "stage4-a"),
                deterministic_fingerprint(&definition("IMOEXF", 100, 201), "stage4-a")
            );
            let mut changed_version = definition("IMOEXF", 100, 200);
            changed_version.source_version = "fixture-v2".to_string();
            assert_ne!(
                deterministic_fingerprint(&base, "stage4-a"),
                deterministic_fingerprint(&changed_version, "stage4-a")
            );
            assert_ne!(
                deterministic_fingerprint(&base, "stage4-a"),
                deterministic_fingerprint(&base, "stage4-b")
            );
        }

        #[test]
        fn mapper_rejects_cross_instrument_and_invalid_window() {
            let stage4 = AcceptedStage4ScheduleEvidence {
                instrument: definition("IMOEXF", 100, 200).instrument,
                observed_at: LifecycleInstant(Utc::now()),
                expires_at: LifecycleInstant(Utc::now()),
                identity: "stage4-a".to_string(),
            };
            assert!(matches!(
                map_trusted_schedule_window(stage4, definition("RI", 100, 200)),
                Err(ScheduleWindowMappingError::InstrumentMismatch)
            ));
            let stage4 = AcceptedStage4ScheduleEvidence {
                instrument: definition("IMOEXF", 100, 200).instrument,
                observed_at: LifecycleInstant(Utc::now()),
                expires_at: LifecycleInstant(Utc::now()),
                identity: "stage4-a".to_string(),
            };
            assert!(matches!(
                map_trusted_schedule_window(stage4, definition("IMOEXF", 200, 200)),
                Err(ScheduleWindowMappingError::InvalidWindow)
            ));
        }
    }
}
// STAGE5E-B3-SCHEDULE-WINDOW-END: sealed-contract-v1

#[cfg(test)]
mod tests {
    use super::{
        session_eligibility::observe_open_session_for_bar,
        validate_contextual_live_bar_after_history, Stage5eContextualAdmissionError,
        Stage5eSessionObservationError,
    };
    use broker_core::{
        BrokerMarketSessionState, Exchange, InstrumentId, Market, Stage4BrokerTruthFreshnessProbe,
    };
    use chrono::{TimeZone, Utc};

    fn instrument(symbol: &str) -> InstrumentId {
        InstrumentId {
            symbol: symbol.to_string(),
            venue_symbol: Some(format!("{symbol}@RTSX")),
            exchange: Exchange::Moex,
            market: Market::Other("test".to_string()),
        }
    }

    fn valid() -> Result<(), Stage5eContextualAdmissionError> {
        validate_contextual_live_bar_after_history(
            broker_core::HybridRuntimeBarOrigin::Live,
            &instrument("IMOEXF"),
            &instrument("IMOEXF"),
            0.5,
            0.5,
            1_000,
            1_600,
            Utc.timestamp_opt(2_000, 0).single().unwrap(),
            Utc.timestamp_opt(1_700, 0).single().unwrap(),
        )
    }

    #[test]
    fn contextual_admission_requires_live_exact_binding_and_same_domain_freshness() {
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Replay,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::NotLive)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("RI"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::InstrumentMismatch)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.1,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::TickSizeMismatch)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_000,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::NotStrictlyAfterHistory)
        );
        assert!(valid().is_ok());
    }

    #[test]
    fn contextual_admission_rejects_expired_admission_and_future_market_bar() {
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_600,
                Utc.timestamp_opt(1_600, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::AdmissionExpired)
        );
        assert_eq!(
            validate_contextual_live_bar_after_history(
                broker_core::HybridRuntimeBarOrigin::Live,
                &instrument("IMOEXF"),
                &instrument("IMOEXF"),
                0.5,
                0.5,
                1_000,
                1_800,
                Utc.timestamp_opt(2_000, 0).single().unwrap(),
                Utc.timestamp_opt(1_700, 0).single().unwrap(),
            ),
            Err(Stage5eContextualAdmissionError::FutureBar)
        );
    }

    fn fresh_schedule() -> Stage4BrokerTruthFreshnessProbe {
        Stage4BrokerTruthFreshnessProbe::fresh(
            Utc.timestamp_opt(1_700, 0).single().unwrap(),
            1_000_000,
            true,
        )
    }

    #[test]
    fn open_session_observation_requires_fresh_explicit_open_window() {
        let observed = observe_open_session_for_bar(
            BrokerMarketSessionState::Open,
            fresh_schedule(),
            1_500,
            1_800,
            1_600,
            Utc.timestamp_opt(1_900, 0).single().unwrap(),
        )
        .expect("fresh open session accepts contained bar");
        assert_eq!(observed.bar_close_ts(), 1_600);
        assert_eq!(observed.callback_count(), 0);
        assert_eq!(observed.intent_count(), 0);
        assert!(!observed.strategy_was_called());
        assert!(!observed.executable_intent_created());
    }

    #[test]
    fn session_observation_blocks_non_open_stale_and_out_of_window_evidence() {
        let now = Utc.timestamp_opt(1_900, 0).single().unwrap();
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Break,
                fresh_schedule(),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotOpen)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Unknown,
                Stage4BrokerTruthFreshnessProbe::unknown(1_000, true),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotOpen)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                Stage4BrokerTruthFreshnessProbe::fresh(
                    Utc.timestamp_opt(1, 0).single().unwrap(),
                    1_000,
                    true,
                ),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotFresh)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_800,
                1_500,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::InvalidObservedWindow)
        );
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_500,
                1_800,
                1_801,
                now,
            ),
            Err(Stage5eSessionObservationError::BarOutsideObservedOpenWindow)
        );
    }

    #[test]
    fn session_observation_has_explicit_inclusive_window_and_all_non_open_states_block() {
        let now = Utc.timestamp_opt(1_900, 0).single().unwrap();
        for state in [
            BrokerMarketSessionState::Break,
            BrokerMarketSessionState::Maintenance,
            BrokerMarketSessionState::Closed,
            BrokerMarketSessionState::Unknown,
        ] {
            assert_eq!(
                observe_open_session_for_bar(state, fresh_schedule(), 1_500, 1_800, 1_600, now),
                Err(Stage5eSessionObservationError::ScheduleNotOpen)
            );
        }
        for bar_close in [1_500, 1_800] {
            assert!(observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                fresh_schedule(),
                1_500,
                1_800,
                bar_close,
                now,
            )
            .is_ok());
        }
        assert_eq!(
            observe_open_session_for_bar(
                BrokerMarketSessionState::Open,
                Stage4BrokerTruthFreshnessProbe::fresh(
                    Utc.timestamp_opt(1_899, 999).single().unwrap(),
                    0,
                    true,
                ),
                1_500,
                1_800,
                1_600,
                now,
            ),
            Err(Stage5eSessionObservationError::ScheduleNotFresh)
        );
    }
}
