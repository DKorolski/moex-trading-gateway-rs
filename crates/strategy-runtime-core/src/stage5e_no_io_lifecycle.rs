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
    // These are mandatory linear Stage 5C-owned inputs.  There is no
    // empty-state/testing constructor: every observed live bar is produced by
    // the sealed Stage 5C recovery bridge below.
    strategy: HybridIntradayRuntimeStrategy,
    recovery_receipt: Stage5cPendingRecoveryReceipt,
    bar: broker_core::HybridRuntimeBarEvent,
    tick_size: f64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stage5eObservedLiveBarOwnershipFingerprint {
    strategy_state: String,
    recovery_receipt: Stage5eRecoveryReceiptIdentity,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stage5eRecoveryReceiptIdentity {
    recovered_ts: DateTime<Utc>,
    last_history_ts: i64,
    replayed_events: usize,
    duplicate_events: usize,
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

    #[cfg(test)]
    pub(crate) fn ownership_fingerprint(&self) -> Stage5eObservedLiveBarOwnershipFingerprint {
        use crate::runtime_compat::Strategy;

        Stage5eObservedLiveBarOwnershipFingerprint {
            strategy_state: crate::stage5c_paper_host::stage5c_semantic_payload_fingerprint(
                Strategy::state(&self.strategy),
            )
            .expect("Stage 5C-owned strategy state must remain serializable"),
            recovery_receipt: Stage5eRecoveryReceiptIdentity {
                recovered_ts: self.recovery_receipt.recovered_ts(),
                last_history_ts: self.recovery_receipt.warmup_receipt().last_history_ts(),
                replayed_events: self.recovery_receipt.replayed_events(),
                duplicate_events: self.recovery_receipt.duplicate_events(),
            },
        }
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

// STAGE5E-B3-SCHEDULE-WINDOW-BEGIN: sealed-contract-v5
// This is a private, no-I/O proof chain. A later reviewed broker adapter may
// supply the raw normalized DTO, but cannot construct any accepted receipt.
#[allow(dead_code)]
pub(crate) mod schedule_window_evidence {
    use super::{DateTime, Digest, NaiveDate, Sha256, Utc};

    // STAGE5E-B3C-PRODUCTION-BRIDGE-BEGIN: trusted-no-io-v1
    /// Opaque one-use capability issued only for the borrowed B3B preflight.
    /// Ownership stays in the Stage 5C receipt until every B3B check succeeds.
    pub(crate) struct Stage5eB3bPreflightSeal(());

    /// Opaque one-use capability issued only inside the B3B transition.
    pub(crate) struct Stage5eB3bConsumeSeal(());

    /// Borrowed validation-only view. It has no decomposition API and cannot
    /// outlive the exact Stage 5C-owned receipt from which it was issued.
    pub(crate) struct Stage5eB3bObservedLiveBarPreflight<'a> {
        strategy: &'a crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        recovery_receipt: &'a crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
        accepted_semantic_bar: &'a crate::stage5c_paper_host::Stage5cAcceptedSemanticBar,
        accepted_semantic_bar_identity: [u8; 32],
        bar_instrument: &'a broker_core::InstrumentId,
        bar_close_ts: i64,
        canonical_predecessor_close_ts: i64,
        schedule_projection: &'a Stage5eScheduleProjectionBridgeInput,
        sequence_classification: Stage5eScheduleSequenceClassification,
        optional_boundary_fingerprint: Option<[u8; 32]>,
        sequence_identity_fingerprint: [u8; 32],
        sequence_observed_at: DateTime<Utc>,
        sequence_expires_at: DateTime<Utc>,
    }

    impl<'a> Stage5eB3bObservedLiveBarPreflight<'a> {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn from_stage5c_observed(
            _seal: Stage5eB3bPreflightSeal,
            strategy: &'a crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
            recovery_receipt: &'a crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
            accepted_semantic_bar: &'a crate::stage5c_paper_host::Stage5cAcceptedSemanticBar,
            accepted_semantic_bar_identity: [u8; 32],
            bar_instrument: &'a broker_core::InstrumentId,
            bar_close_ts: i64,
            canonical_predecessor_close_ts: i64,
            schedule_projection: &'a Stage5eScheduleProjectionBridgeInput,
            sequence_classification: Stage5eScheduleSequenceClassification,
            optional_boundary_fingerprint: Option<[u8; 32]>,
            sequence_identity_fingerprint: [u8; 32],
            sequence_observed_at: DateTime<Utc>,
            sequence_expires_at: DateTime<Utc>,
        ) -> Self {
            Self {
                strategy,
                recovery_receipt,
                accepted_semantic_bar,
                accepted_semantic_bar_identity,
                bar_instrument,
                bar_close_ts,
                canonical_predecessor_close_ts,
                schedule_projection,
                sequence_classification,
                optional_boundary_fingerprint,
                sequence_identity_fingerprint,
                sequence_observed_at,
                sequence_expires_at,
            }
        }
    }

    /// Exact linear payload accepted by B3B. It has one crate-private
    /// constructor requiring the consume seal and no decomposition API.
    pub(crate) struct Stage5eB3bObservedLiveBarBridgePayload {
        strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
        accepted_semantic_bar: crate::stage5c_paper_host::Stage5cAcceptedSemanticBar,
        accepted_semantic_bar_identity: [u8; 32],
        bar_instrument: broker_core::InstrumentId,
        bar_close_ts: i64,
        canonical_predecessor_close_ts: i64,
        schedule_projection: Stage5eScheduleProjectionBridgeInput,
        sequence_classification: Stage5eScheduleSequenceClassification,
        optional_boundary_fingerprint: Option<[u8; 32]>,
        sequence_identity_fingerprint: [u8; 32],
        sequence_observed_at: DateTime<Utc>,
        sequence_expires_at: DateTime<Utc>,
    }

    impl Stage5eB3bObservedLiveBarBridgePayload {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn from_stage5c_observed(
            _seal: Stage5eB3bConsumeSeal,
            strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
            recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
            accepted_semantic_bar: crate::stage5c_paper_host::Stage5cAcceptedSemanticBar,
            accepted_semantic_bar_identity: [u8; 32],
            bar_instrument: broker_core::InstrumentId,
            bar_close_ts: i64,
            canonical_predecessor_close_ts: i64,
            schedule_projection: Stage5eScheduleProjectionBridgeInput,
            sequence_classification: Stage5eScheduleSequenceClassification,
            optional_boundary_fingerprint: Option<[u8; 32]>,
            sequence_identity_fingerprint: [u8; 32],
            sequence_observed_at: DateTime<Utc>,
            sequence_expires_at: DateTime<Utc>,
        ) -> Self {
            Self {
                strategy,
                recovery_receipt,
                accepted_semantic_bar,
                accepted_semantic_bar_identity,
                bar_instrument,
                bar_close_ts,
                canonical_predecessor_close_ts,
                schedule_projection,
                sequence_classification,
                optional_boundary_fingerprint,
                sequence_identity_fingerprint,
                sequence_observed_at,
                sequence_expires_at,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct MarketBarCloseTime(i64);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct LifecycleInstant(DateTime<Utc>);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct TradingDay(NaiveDate);

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ScheduleSourceIdentity {
        BrokerReported,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    struct NormalizedInstrumentScheduleSnapshot {
        instrument: broker_core::InstrumentId,
        broker_symbol: String,
        venue_mic: String,
        board: String,
        trading_day: TradingDay,
        sessions: Vec<NormalizedScheduleSession>,
        source: ScheduleSourceIdentity,
        source_contract_version: String,
        source_observed_at: LifecycleInstant,
        source_expires_at: LifecycleInstant,
        raw_response_sha256: [u8; 32],
        normalized_payload_sha256: [u8; 32],
        instrument_registry_version: String,
    }

    enum NormalizedScheduleAvailability {
        Available(Box<NormalizedInstrumentScheduleSnapshot>),
        ScheduleSourceUnavailable,
    }

    #[derive(Debug)]
    enum NormalizedScheduleValidationError {
        SourceUnavailable,
        MissingIdentityMetadata,
        EmptySessions,
        NoTradableOpen,
        UnknownSessionType,
        InvalidInterval,
        OverlappingIntervals,
        ObservedAfterExpiry,
        ObservedInFuture,
        Expired,
        CanonicalBrokerSymbolMismatch,
        PayloadFingerprintMismatch,
    }

    /// Opaque linear receipt: it is issued only after all snapshot invariants
    /// and its canonical payload fingerprint have been checked.
    struct ValidatedNormalizedInstrumentScheduleSnapshot {
        snapshot: NormalizedInstrumentScheduleSnapshot,
        sessions_fingerprint: [u8; 32],
        identity_fingerprint: [u8; 32],
    }

    /// Private input of the separately sealed registry bridge. It cannot be
    /// confused with the accepted receipt produced by that bridge.
    struct SealedInstrumentRegistryBridgeInput {
        instrument: broker_core::InstrumentId,
        broker_symbol: String,
        venue_mic: String,
        board: String,
        registry_version: String,
    }

    /// Opaque registry evidence consumed by the mapper. The only constructor
    /// is the exact-match sealed bridge below.
    struct AcceptedInstrumentRegistryEvidence {
        identity: SealedInstrumentRegistryBridgeInput,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ScheduleFingerprint([u8; 32]);

    #[derive(Debug)]
    enum Stage4ScheduleProjectionError {
        NotAccepted,
        SessionNotOpen,
        SourceUnavailable,
        FreshnessUnavailable,
        ReportCheckedInFuture,
        ObservedInFuture,
        Expired,
    }

    #[derive(Debug)]
    enum RegistryBindingError {
        IdentityMismatch,
    }

    #[derive(Debug)]
    enum ScheduleWindowMappingError {
        InstrumentMismatch,
        RegistryMismatch,
        Stage4NotOpen,
        Stage4Expired,
        SnapshotExpired,
        NoTradableOpenForRequestedBar,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ScheduleWindowObservedBarBindingError {
        InstrumentMismatch,
        BarOutsideInclusiveWindow,
        WindowExpired,
        ObservedBarInFuture,
        ClockBeforeEffectiveEvidenceObservation,
    }

    struct AcceptedStage4ScheduleEvidence {
        instrument: broker_core::InstrumentId,
        session_state: broker_core::BrokerMarketSessionState,
        observed_at: LifecycleInstant,
        expires_at: LifecycleInstant,
        identity: ScheduleFingerprint,
    }

    struct Stage5eScheduleWindowEvidence {
        instrument: broker_core::InstrumentId,
        broker_symbol: String,
        venue_mic: String,
        board: String,
        trading_day: TradingDay,
        source_contract_version: String,
        selected_session_type: NormalizedSessionType,
        open_from: MarketBarCloseTime,
        open_until: MarketBarCloseTime,
        /// The raw normalized schedule and Stage 4 freshness report are
        /// independent inputs.  Mapping requires both to remain fresh; the
        /// later observation is retained as the conservative effective time.
        normalized_observed_at: LifecycleInstant,
        stage4_observed_at: LifecycleInstant,
        effective_observed_at: LifecycleInstant,
        expires_at: LifecycleInstant,
        fingerprint: ScheduleFingerprint,
        /// Retained only inside the schedule owner.  A later sealed classifier
        /// consumes this projection; Stage 5C never receives these intervals.
        normalized_sessions: Vec<NormalizedScheduleSession>,
        normalized_sessions_fingerprint: [u8; 32],
        normalized_snapshot_identity_fingerprint: [u8; 32],
        stage4_dynamic_session_fingerprint: [u8; 32],
    }

    /// Opaque schedule projection returned to Stage 5C only after the schedule
    /// owner has classified every expected-close grid point. Raw normalized
    /// sessions never cross this boundary.
    pub(crate) struct Stage5eScheduleProjectionBridgeInput {
        schedule_window: Stage5eScheduleWindowEvidence,
    }

    /// The only input accepted by the Stage 5C sequence issuer. Construction
    /// remains inside this schedule owner and consumes the retained projection.
    pub(crate) struct Stage5eScheduleCandidateClassifier {
        projection: Stage5eScheduleProjectionBridgeInput,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eScheduleSequenceClassification {
        Contiguous,
        ApprovedNonTradableBoundary([u8; 32]),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eScheduleClassificationBlockReason {
        InvalidTimeframe,
        NonMonotonicSequence,
        UnalignedEndpoint,
        CrossTradingDay,
        CandidateCountOverflow,
        EndpointOrCandidateUncovered,
        EndpointOrCandidateAmbiguous,
        PredecessorCloseNotTradableOpen,
        CurrentCloseNotTradableOpen,
        InteriorTradableOpen,
        InteriorUnknown,
    }

    pub(crate) struct Stage5eScheduleClassificationApproved {
        classification: Stage5eScheduleSequenceClassification,
        projection: Stage5eScheduleProjectionBridgeInput,
    }

    pub(crate) struct Stage5eScheduleClassificationBlocked {
        reason: Stage5eScheduleClassificationBlockReason,
        returned_projection: Stage5eScheduleProjectionBridgeInput,
    }

    impl Stage5eScheduleClassificationApproved {
        pub(crate) fn into_classified_parts(
            self,
        ) -> (
            Stage5eScheduleSequenceClassification,
            Stage5eScheduleProjectionBridgeInput,
        ) {
            (self.classification, self.projection)
        }
    }

    impl Stage5eScheduleClassificationBlocked {
        pub(crate) fn reason(&self) -> Stage5eScheduleClassificationBlockReason {
            self.reason
        }

        pub(crate) fn into_retry(self) -> Stage5eScheduleProjectionBridgeInput {
            self.returned_projection
        }
    }

    impl Stage5eScheduleCandidateClassifier {
        pub(crate) fn classify_from_stage5c_seal_fields(
            self,
            predecessor_close_ts: i64,
            current_close_ts: i64,
            timeframe_sec: std::num::NonZeroU32,
        ) -> Result<Stage5eScheduleClassificationApproved, Box<Stage5eScheduleClassificationBlocked>>
        {
            match classify_expected_close_grid(
                &self.projection.schedule_window,
                predecessor_close_ts,
                current_close_ts,
                timeframe_sec,
            ) {
                Ok(classification) => Ok(Stage5eScheduleClassificationApproved {
                    classification,
                    projection: self.projection,
                }),
                Err(reason) => Err(Box::new(Stage5eScheduleClassificationBlocked {
                    reason,
                    returned_projection: self.projection,
                })),
            }
        }
    }

    pub(crate) struct Stage5eBoundScheduleWindowSequenceForObservedLiveBar {
        payload: Stage5eB3bObservedLiveBarBridgePayload,
        event_key_fingerprint: [u8; 32],
        effective_observed_at: DateTime<Utc>,
        effective_expires_at: DateTime<Utc>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eB3bBindingBlockReason {
        InstrumentMismatch,
        BarOutsideSelectedOpenWindow,
        ClockBeforeEffectiveObservation,
        EvidenceExpired,
        BarObservedInFuture,
        SequenceIdentityMissing,
        SequenceClassificationMismatch,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eB3bBlockDisposition {
        RetrySameReceipt,
        RefreshScheduleRequired,
        TerminalIntegrityBlock,
    }

    impl Stage5eB3bBindingBlockReason {
        pub(crate) fn disposition(self) -> Stage5eB3bBlockDisposition {
            match self {
                Self::ClockBeforeEffectiveObservation | Self::BarObservedInFuture => {
                    Stage5eB3bBlockDisposition::RetrySameReceipt
                }
                Self::EvidenceExpired | Self::BarOutsideSelectedOpenWindow => {
                    Stage5eB3bBlockDisposition::RefreshScheduleRequired
                }
                Self::InstrumentMismatch
                | Self::SequenceIdentityMissing
                | Self::SequenceClassificationMismatch => {
                    Stage5eB3bBlockDisposition::TerminalIntegrityBlock
                }
            }
        }
    }

    pub(crate) struct Stage5eScheduleWindowSequenceObservedBarBlocked {
        reason: Stage5eB3bBindingBlockReason,
        observed: crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence,
    }

    impl Stage5eScheduleWindowSequenceObservedBarBlocked {
        pub(crate) fn reason(&self) -> Stage5eB3bBindingBlockReason {
            self.reason
        }

        pub(crate) fn disposition(&self) -> Stage5eB3bBlockDisposition {
            self.reason.disposition()
        }

        pub(crate) fn into_retry(
            self,
        ) -> crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence {
            self.observed
        }
    }

    fn issue_stage5e_b3b_preflight_seal_inside_bind_schedule_window_sequence_to_observed_live_bar(
    ) -> Stage5eB3bPreflightSeal {
        Stage5eB3bPreflightSeal(())
    }

    fn issue_stage5e_b3b_consume_seal_inside_bind_schedule_window_sequence_to_observed_live_bar(
    ) -> Stage5eB3bConsumeSeal {
        Stage5eB3bConsumeSeal(())
    }

    pub(crate) fn bind_schedule_window_sequence_to_observed_live_bar(
        observed: crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence,
    ) -> Result<
        Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
        Box<Stage5eScheduleWindowSequenceObservedBarBlocked>,
    > {
        bind_schedule_window_sequence_to_observed_live_bar_with_now(observed, Utc::now())
    }

    #[cfg(test)]
    fn bind_schedule_window_sequence_to_observed_live_bar_at(
        observed: crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence,
        now: DateTime<Utc>,
    ) -> Result<
        Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
        Box<Stage5eScheduleWindowSequenceObservedBarBlocked>,
    > {
        bind_schedule_window_sequence_to_observed_live_bar_with_now(observed, now)
    }

    fn bind_schedule_window_sequence_to_observed_live_bar_with_now(
        observed: crate::stage5c_paper_host::Stage5eObservedLiveBarWithSequenceEvidence,
        now: DateTime<Utc>,
    ) -> Result<
        Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
        Box<Stage5eScheduleWindowSequenceObservedBarBlocked>,
    > {
        let approved = {
            let preflight = observed.preflight_for_b3b(
                issue_stage5e_b3b_preflight_seal_inside_bind_schedule_window_sequence_to_observed_live_bar(),
            );
            match validate_b3b_preflight(preflight, now) {
                Ok(approved) => approved,
                Err(reason) => {
                    return Err(Box::new(Stage5eScheduleWindowSequenceObservedBarBlocked {
                        reason,
                        observed,
                    }));
                }
            }
        };
        let payload = observed.consume_for_b3b(
            issue_stage5e_b3b_consume_seal_inside_bind_schedule_window_sequence_to_observed_live_bar(
            ),
        );
        Ok(Stage5eBoundScheduleWindowSequenceForObservedLiveBar {
            payload,
            event_key_fingerprint: approved.event_key_fingerprint,
            effective_observed_at: approved.effective_observed_at,
            effective_expires_at: approved.effective_expires_at,
        })
    }

    struct Stage5eB3bPreflightApproved {
        event_key_fingerprint: [u8; 32],
        effective_observed_at: DateTime<Utc>,
        effective_expires_at: DateTime<Utc>,
    }

    fn validate_b3b_preflight(
        preflight: Stage5eB3bObservedLiveBarPreflight<'_>,
        now: DateTime<Utc>,
    ) -> Result<Stage5eB3bPreflightApproved, Stage5eB3bBindingBlockReason> {
        let schedule = &preflight.schedule_projection.schedule_window;
        let _linear_ownership = (
            preflight.strategy,
            preflight.recovery_receipt,
            preflight.accepted_semantic_bar,
        );
        let effective_observed_at = schedule
            .effective_observed_at
            .0
            .max(preflight.sequence_observed_at);
        let effective_expires_at = schedule.expires_at.0.min(preflight.sequence_expires_at);
        if preflight.bar_instrument != &schedule.instrument {
            return Err(Stage5eB3bBindingBlockReason::InstrumentMismatch);
        }
        if preflight.canonical_predecessor_close_ts >= preflight.bar_close_ts {
            return Err(Stage5eB3bBindingBlockReason::SequenceClassificationMismatch);
        }
        if preflight.bar_close_ts < schedule.open_from.0
            || preflight.bar_close_ts > schedule.open_until.0
        {
            return Err(Stage5eB3bBindingBlockReason::BarOutsideSelectedOpenWindow);
        }
        if now < effective_observed_at {
            return Err(Stage5eB3bBindingBlockReason::ClockBeforeEffectiveObservation);
        }
        if now > effective_expires_at {
            return Err(Stage5eB3bBindingBlockReason::EvidenceExpired);
        }
        if preflight.bar_close_ts > now.timestamp() {
            return Err(Stage5eB3bBindingBlockReason::BarObservedInFuture);
        }
        if preflight.sequence_identity_fingerprint == [0; 32] {
            return Err(Stage5eB3bBindingBlockReason::SequenceIdentityMissing);
        }
        let classification_consistent = match (
            preflight.sequence_classification,
            preflight.optional_boundary_fingerprint,
        ) {
            (Stage5eScheduleSequenceClassification::Contiguous, None) => true,
            (
                Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary(expected),
                Some(actual),
            ) => expected == actual,
            _ => false,
        };
        if !classification_consistent {
            return Err(Stage5eB3bBindingBlockReason::SequenceClassificationMismatch);
        }
        let event_key_fingerprint = b3b_event_key_fingerprint(
            schedule.fingerprint.0,
            preflight.bar_instrument,
            preflight.bar_close_ts,
            preflight.sequence_identity_fingerprint,
        );
        Ok(Stage5eB3bPreflightApproved {
            event_key_fingerprint,
            effective_observed_at,
            effective_expires_at,
        })
    }

    pub(super) fn b3b_event_key_fingerprint(
        schedule_window_identity_fingerprint: [u8; 32],
        instrument: &broker_core::InstrumentId,
        semantic_bar_close_ts: i64,
        sequence_identity_fingerprint: [u8; 32],
    ) -> [u8; 32] {
        let mut encoder =
            CanonicalEncoder::new(b"stage5e-b3b-schedule-observed-sequence-binding-v2");
        encoder.field(1, &schedule_window_identity_fingerprint);
        encode_instrument(&mut encoder, instrument);
        encoder.field(10, &semantic_bar_close_ts.to_be_bytes());
        encoder.field(11, &sequence_identity_fingerprint);
        encoder.finish()
    }

    pub(crate) struct Stage5eB3fEventKeyValidatedProof(());
    pub(crate) struct Stage5eB3fEventKeyMismatch(());

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_stage5e_b3f_b3b_event_key_binding(
        audit_schedule_identity_fingerprint: &[u8; 32],
        audit_full_instrument_id: &broker_core::InstrumentId,
        retained_bar_close_i64: i64,
        audit_sequence_identity_fingerprint: &[u8; 32],
        audit_event_key_fingerprint: &[u8; 32],
        audit_b3b_event_key_fingerprint: &[u8; 32],
        _seal: &crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::Stage5ePaperSettlementPreflightSeal,
    ) -> Result<Stage5eB3fEventKeyValidatedProof, Stage5eB3fEventKeyMismatch> {
        let recomputed = b3b_event_key_fingerprint(
            *audit_schedule_identity_fingerprint,
            audit_full_instrument_id,
            retained_bar_close_i64,
            *audit_sequence_identity_fingerprint,
        );
        if recomputed != *audit_event_key_fingerprint
            || recomputed != *audit_b3b_event_key_fingerprint
        {
            return Err(Stage5eB3fEventKeyMismatch(()));
        }
        Ok(Stage5eB3fEventKeyValidatedProof(()))
    }

    pub(crate) mod b3c_evidence {
        use super::*;

        pub(crate) struct Stage5eB3eNestedInvocationMaterial {
            callback_now: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            issued_at: DateTime<Utc>,
            effective_observed_at: DateTime<Utc>,
            authority_expires_at: DateTime<Utc>,
            full_instrument_id: broker_core::InstrumentId,
            accepted_semantic_bar_identity: [u8; 32],
            b3b_event_key_fingerprint: [u8; 32],
            b3c_continuation_binding_id: [u8; 32],
            sequence_identity_fingerprint: [u8; 32],
        }

        #[allow(clippy::too_many_arguments)]
        pub(crate) fn construct_nested_invocation_material(
            callback_now: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            issued_at: DateTime<Utc>,
            effective_observed_at: DateTime<Utc>,
            authority_expires_at: DateTime<Utc>,
            full_instrument_id: broker_core::InstrumentId,
            accepted_semantic_bar_identity: [u8; 32],
            b3b_event_key_fingerprint: [u8; 32],
            b3c_continuation_binding_id: [u8; 32],
            sequence_identity_fingerprint: [u8; 32],
            _nested_consume_capability: &crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedConsumeSeal,
        ) -> Stage5eB3eNestedInvocationMaterial {
            Stage5eB3eNestedInvocationMaterial {
                callback_now,
                callback_authority_id,
                issued_at,
                effective_observed_at,
                authority_expires_at,
                full_instrument_id,
                accepted_semantic_bar_identity,
                b3b_event_key_fingerprint,
                b3c_continuation_binding_id,
                sequence_identity_fingerprint,
            }
        }

        pub(crate) struct Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            pub(super) b3b: Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
            pub(super) continuation_binding_id: [u8; 32],
            pub(super) bound_at: DateTime<Utc>,
            pub(super) effective_observed_at: DateTime<Utc>,
            pub(super) effective_expires_at: DateTime<Utc>,
        }

        // STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-BEGIN
        #[cfg(test)]
        pub(crate) mod stage5f_test_seams {
            use super::*;

            /// Applies the established B3E callback-validation mutation only
            /// after the schedule/sequence evidence has been accepted.
            pub(crate) fn force_callback_validation_error(
                receipt: &mut Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            ) {
                receipt
                    .b3b
                    .payload
                    .accepted_semantic_bar
                    .stage5e_test_force_callback_validation_error();
            }
        }
        // STAGE5F-TEST-CALLBACK-VALIDATION-SEAM-END

        impl Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            pub(crate) fn borrow_callback_authority_preflight(
                &self,
                seal: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eCallbackAuthorityIssueSeal,
            ) -> crate::stage5e_no_io_lifecycle::callback_authority::Stage5eCallbackAuthorityPreflight<'_>
            {
                let schedule = &self.b3b.payload.schedule_projection.schedule_window;
                crate::stage5e_no_io_lifecycle::callback_authority::Stage5eCallbackAuthorityPreflight::from_b3c_receipt(
                    seal,
                    self,
                    &self.b3b.payload.bar_instrument,
                    self.b3b.payload.accepted_semantic_bar_identity,
                    self.b3b.event_key_fingerprint,
                    self.continuation_binding_id,
                    self.b3b.payload.sequence_identity_fingerprint,
                    schedule.fingerprint.0,
                    self.b3b.payload.bar_close_ts,
                    self.effective_observed_at,
                    self.effective_expires_at,
                )
            }

            pub(crate) fn borrow_for_authorized_callback_preflight(
                &self,
                seal: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedPreflightSeal,
            ) -> crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedPreflight<'_>
            {
                let schedule = &self.b3b.payload.schedule_projection.schedule_window;
                crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedPreflight::from_b3c_receipt(
                    seal,
                    &self.b3b.payload.bar_instrument,
                    self.b3b.payload.accepted_semantic_bar_identity,
                    self.b3b.event_key_fingerprint,
                    self.continuation_binding_id,
                    schedule.fingerprint.0,
                    self.b3b.payload.sequence_identity_fingerprint,
                    self.b3b.payload.bar_close_ts,
                    self.bound_at,
                    self.effective_observed_at,
                    self.effective_expires_at,
                )
            }

            pub(crate) fn consume_for_authorized_callback_with_nested_seal_and_invocation_context(
                self,
                nested_consume_seal: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedConsumeSeal,
                invocation_context: crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eInvocationConsumeContext,
            ) -> Result<
                crate::stage5e_no_io_lifecycle::callback_authority::Stage5eAuthorizedPaperCallbackPayload,
                crate::stage5e_no_io_lifecycle::callback_authority::Stage5eCallbackInvocationTerminalBlock,
            >{
                let nested = invocation_context.consume_for_nested_b3c(&nested_consume_seal);
                let Stage5eB3eNestedInvocationMaterial {
                    callback_now,
                    callback_authority_id,
                    issued_at,
                    effective_observed_at,
                    authority_expires_at,
                    full_instrument_id,
                    accepted_semantic_bar_identity,
                    b3b_event_key_fingerprint,
                    b3c_continuation_binding_id,
                    sequence_identity_fingerprint,
                } = nested;
                let Self {
                    b3b,
                    continuation_binding_id,
                    bound_at,
                    effective_observed_at: b3c_effective_observed_at,
                    effective_expires_at: b3c_effective_expires_at,
                } = self;
                let Stage5eBoundScheduleWindowSequenceForObservedLiveBar {
                    payload,
                    event_key_fingerprint,
                    effective_observed_at: b3b_effective_observed_at,
                    effective_expires_at: b3b_effective_expires_at,
                } = b3b;
                let Stage5eB3bObservedLiveBarBridgePayload {
                    strategy,
                    recovery_receipt,
                    accepted_semantic_bar,
                    accepted_semantic_bar_identity: owned_bar_identity,
                    bar_instrument,
                    bar_close_ts: _,
                    canonical_predecessor_close_ts: _,
                    schedule_projection,
                    sequence_classification,
                    optional_boundary_fingerprint,
                    sequence_identity_fingerprint: owned_sequence_identity,
                    sequence_observed_at,
                    sequence_expires_at,
                } = payload;
                let schedule = schedule_projection.schedule_window;
                let stage5c_material =
                    match crate::stage5c_paper_host::consume_stage5c_for_authorized_callback(
                        strategy,
                        recovery_receipt,
                        accepted_semantic_bar,
                        crate::stage5c_paper_host::issue_stage5c_b3e_callback_material_seal(
                            &nested_consume_seal,
                        ),
                        callback_now,
                    ) {
                        Ok(material) => material,
                        Err(block) => {
                            return Err(crate::stage5e_no_io_lifecycle::callback_authority::map_stage5c_materialization_terminal_to_callback_terminal(
                            block,
                            &nested_consume_seal,
                        ));
                        }
                    };
                let audit_lineage = construct_audit_lineage_from_consumed_nested_material(
                    schedule.fingerprint.0,
                    sequence_classification,
                    optional_boundary_fingerprint,
                    owned_sequence_identity,
                    sequence_observed_at,
                    sequence_expires_at,
                    event_key_fingerprint,
                    b3b_effective_observed_at,
                    b3b_effective_expires_at,
                    continuation_binding_id,
                    bound_at,
                    b3c_effective_observed_at,
                    b3c_effective_expires_at,
                    callback_authority_id,
                    issued_at,
                    effective_observed_at,
                    authority_expires_at,
                    full_instrument_id,
                    accepted_semantic_bar_identity,
                    b3b_event_key_fingerprint,
                    b3c_continuation_binding_id,
                    sequence_identity_fingerprint,
                    bar_instrument,
                    owned_bar_identity,
                    &nested_consume_seal,
                );
                Ok(crate::stage5e_no_io_lifecycle::callback_authority::construct_stage5e_authorized_paper_callback_payload(
                    stage5c_material,
                    audit_lineage,
                    callback_now,
                    callback_authority_id,
                    &nested_consume_seal,
                ))
            }

            #[cfg(test)]
            pub(crate) fn test_ownership_fingerprint(
                &self,
            ) -> (String, DateTime<Utc>, i64, usize, usize) {
                (
                    crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(
                        &self.b3b.payload.strategy,
                    ),
                    self.b3b.payload.recovery_receipt.recovered_ts(),
                    self.b3b
                        .payload
                        .recovery_receipt
                        .warmup_receipt()
                        .last_history_ts(),
                    self.b3b.payload.recovery_receipt.replayed_events(),
                    self.b3b.payload.recovery_receipt.duplicate_events(),
                )
            }

            #[cfg(test)]
            pub(crate) fn test_zero_owned_schedule_identity(&mut self) {
                self.b3b
                    .payload
                    .schedule_projection
                    .schedule_window
                    .fingerprint
                    .0 = [0; 32];
            }

            #[cfg(test)]
            pub(crate) fn test_force_bound_at(&mut self, bound_at: DateTime<Utc>) {
                self.bound_at = bound_at;
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn construct_audit_lineage_from_consumed_nested_material(
            schedule_identity_fingerprint: [u8; 32],
            sequence_classification: Stage5eScheduleSequenceClassification,
            optional_boundary_fingerprint: Option<[u8; 32]>,
            owned_sequence_identity: [u8; 32],
            sequence_observed_at: DateTime<Utc>,
            sequence_expires_at: DateTime<Utc>,
            event_key_fingerprint: [u8; 32],
            b3b_effective_observed_at: DateTime<Utc>,
            b3b_effective_expires_at: DateTime<Utc>,
            continuation_binding_id: [u8; 32],
            bound_at: DateTime<Utc>,
            b3c_effective_observed_at: DateTime<Utc>,
            b3c_effective_expires_at: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            issued_at: DateTime<Utc>,
            effective_observed_at: DateTime<Utc>,
            authority_expires_at: DateTime<Utc>,
            full_instrument_id: broker_core::InstrumentId,
            accepted_semantic_bar_identity: [u8; 32],
            b3b_event_key_fingerprint: [u8; 32],
            b3c_continuation_binding_id: [u8; 32],
            sequence_identity_fingerprint: [u8; 32],
            owned_instrument: broker_core::InstrumentId,
            owned_bar_identity: [u8; 32],
            nested_consume_capability: &crate::stage5e_no_io_lifecycle::callback_authority::Stage5eB3eNestedConsumeSeal,
        ) -> crate::stage5e_no_io_lifecycle::callback_authority::Stage5eAuthorizedCallbackAuditLineage
        {
            crate::stage5e_no_io_lifecycle::callback_authority::construct_stage5e_authorized_callback_audit_lineage(
                schedule_identity_fingerprint,
                sequence_classification,
                optional_boundary_fingerprint,
                owned_sequence_identity,
                sequence_observed_at,
                sequence_expires_at,
                event_key_fingerprint,
                b3b_effective_observed_at,
                b3b_effective_expires_at,
                continuation_binding_id,
                bound_at,
                b3c_effective_observed_at,
                b3c_effective_expires_at,
                callback_authority_id,
                issued_at,
                effective_observed_at,
                authority_expires_at,
                full_instrument_id,
                accepted_semantic_bar_identity,
                b3b_event_key_fingerprint,
                b3c_continuation_binding_id,
                sequence_identity_fingerprint,
                owned_instrument,
                owned_bar_identity,
                nested_consume_capability,
            )
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Stage5eB3cBindingBlockReason {
            ClockBeforeEffectiveObservation,
            EvidenceExpired,
            BarObservedInFuture,
            MissingCanonicalIdentity,
        }

        pub(crate) struct Stage5eSessionCalendarSequenceBlocked {
            reason: Stage5eB3cBindingBlockReason,
            b3b: Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
        }

        impl Stage5eSessionCalendarSequenceBlocked {
            pub(crate) fn reason(&self) -> Stage5eB3cBindingBlockReason {
                self.reason
            }

            pub(crate) fn into_retry(self) -> Stage5eBoundScheduleWindowSequenceForObservedLiveBar {
                self.b3b
            }
        }

        pub(crate) fn bind_session_calendar_sequence_from_b3b(
            b3b: Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
        ) -> Result<
            Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            Box<Stage5eSessionCalendarSequenceBlocked>,
        > {
            validate_session_calendar_sequence_from_b3b(b3b, Utc::now())
        }

        #[cfg(test)]
        pub(super) fn bind_session_calendar_sequence_from_b3b_at(
            b3b: Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
            now: DateTime<Utc>,
        ) -> Result<
            Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            Box<Stage5eSessionCalendarSequenceBlocked>,
        > {
            validate_session_calendar_sequence_from_b3b(b3b, now)
        }

        fn validate_session_calendar_sequence_from_b3b(
            b3b: Stage5eBoundScheduleWindowSequenceForObservedLiveBar,
            now: DateTime<Utc>,
        ) -> Result<
            Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            Box<Stage5eSessionCalendarSequenceBlocked>,
        > {
            let schedule = &b3b.payload.schedule_projection.schedule_window;
            let effective_observed_at = schedule
                .effective_observed_at
                .0
                .max(b3b.payload.sequence_observed_at);
            let effective_expires_at = schedule.expires_at.0.min(b3b.payload.sequence_expires_at);
            let block =
                |reason, b3b| Box::new(Stage5eSessionCalendarSequenceBlocked { reason, b3b });
            if now < effective_observed_at {
                return Err(block(
                    Stage5eB3cBindingBlockReason::ClockBeforeEffectiveObservation,
                    b3b,
                ));
            }
            if now > effective_expires_at {
                return Err(block(Stage5eB3cBindingBlockReason::EvidenceExpired, b3b));
            }
            if b3b.payload.bar_close_ts > now.timestamp() {
                return Err(block(
                    Stage5eB3cBindingBlockReason::BarObservedInFuture,
                    b3b,
                ));
            }
            if b3b.event_key_fingerprint == [0; 32]
                || schedule.normalized_snapshot_identity_fingerprint == [0; 32]
                || schedule.fingerprint.0 == [0; 32]
                || schedule.stage4_dynamic_session_fingerprint == [0; 32]
                || b3b.payload.sequence_identity_fingerprint == [0; 32]
            {
                return Err(block(
                    Stage5eB3cBindingBlockReason::MissingCanonicalIdentity,
                    b3b,
                ));
            }
            let mut encoder = CanonicalEncoder::new(b"stage5e-continuation-binding-v3");
            encoder.field(1, &b3b.event_key_fingerprint);
            encoder.field(2, &schedule.normalized_snapshot_identity_fingerprint);
            encoder.field(3, &schedule.fingerprint.0);
            encoder.field(4, &schedule.stage4_dynamic_session_fingerprint);
            encoder.field(5, &b3b.payload.sequence_identity_fingerprint);
            let continuation_binding_id = encoder.finish();
            Ok(Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
                b3b,
                continuation_binding_id,
                bound_at: now,
                effective_observed_at,
                effective_expires_at,
            })
        }
    }

    /// Linear b3b receipt. It deliberately owns both earlier receipts so a
    /// later reviewed continuation cannot substitute a raw bar, window,
    /// fingerprint, expiry, or instrument identity.
    struct Stage5eBoundScheduleWindowForObservedLiveBar {
        schedule_window: Stage5eScheduleWindowEvidence,
        observed_live_bar: super::Stage5eObservedLiveBarAfterHistory,
        bar_instrument: broker_core::InstrumentId,
        bar_close_ts: MarketBarCloseTime,
        schedule_fingerprint: ScheduleFingerprint,
        binding_fingerprint: ScheduleObservedBarBindingFingerprint,
    }

    /// Binding is fail-closed but linear: every blocker returns both consumed
    /// inputs so a caller may obtain fresh evidence and retry without silently
    /// dropping strategy/recovery state.
    struct Stage5eScheduleWindowObservedBarBlocked {
        reason: ScheduleWindowObservedBarBindingError,
        schedule_window: Stage5eScheduleWindowEvidence,
        observed_live_bar: super::Stage5eObservedLiveBarAfterHistory,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ScheduleObservedBarBindingFingerprint([u8; 32]);

    struct CanonicalEncoder {
        hasher: Sha256,
    }

    impl CanonicalEncoder {
        fn new(domain: &[u8]) -> Self {
            let mut hasher = Sha256::new();
            hasher.update(domain);
            Self { hasher }
        }

        fn field(&mut self, tag: u8, bytes: &[u8]) {
            self.hasher.update([tag]);
            self.hasher.update((bytes.len() as u64).to_be_bytes());
            self.hasher.update(bytes);
        }

        fn finish(self) -> [u8; 32] {
            self.hasher.finalize().into()
        }
    }

    fn string_field(encoder: &mut CanonicalEncoder, tag: u8, value: &str) {
        encoder.field(tag, value.as_bytes());
    }

    fn encode_instrument(encoder: &mut CanonicalEncoder, instrument: &broker_core::InstrumentId) {
        string_field(encoder, 1, &instrument.symbol);
        string_field(encoder, 2, instrument.venue_symbol.as_deref().unwrap_or(""));
        match &instrument.exchange {
            broker_core::Exchange::Moex => encoder.field(3, b"moex"),
            broker_core::Exchange::Other(value) => {
                encoder.field(3, b"other");
                string_field(encoder, 4, value);
            }
        }
        match &instrument.market {
            broker_core::Market::Futures => encoder.field(5, b"futures"),
            broker_core::Market::Options => encoder.field(5, b"options"),
            broker_core::Market::Stocks => encoder.field(5, b"stocks"),
            broker_core::Market::Currency => encoder.field(5, b"currency"),
            broker_core::Market::Funds => encoder.field(5, b"funds"),
            broker_core::Market::Other(value) => {
                encoder.field(5, b"other");
                string_field(encoder, 6, value);
            }
        }
    }

    fn session_type_code(value: NormalizedSessionType) -> u8 {
        match value {
            NormalizedSessionType::TradableOpen => 1,
            NormalizedSessionType::BreakOrClearing => 2,
            NormalizedSessionType::Maintenance => 3,
            NormalizedSessionType::Unknown => 255,
        }
    }

    fn canonical_sessions_fingerprint(sessions: &[NormalizedScheduleSession]) -> [u8; 32] {
        let mut ordered = sessions.to_vec();
        ordered.sort_by_key(|session| {
            (
                session.start.0,
                session.end.0,
                session_type_code(session.session_type),
            )
        });
        let mut encoder = CanonicalEncoder::new(b"stage5e-b3-sessions-v2");
        for session in ordered {
            encoder.field(1, &[session_type_code(session.session_type)]);
            encoder.field(2, &session.start.0.to_be_bytes());
            encoder.field(3, &session.end.0.to_be_bytes());
        }
        encoder.finish()
    }

    fn normalized_snapshot_payload_fingerprint(
        snapshot: &NormalizedInstrumentScheduleSnapshot,
    ) -> [u8; 32] {
        let mut encoder = CanonicalEncoder::new(b"stage5e-b3-normalized-snapshot-v2");
        encode_instrument(&mut encoder, &snapshot.instrument);
        string_field(&mut encoder, 10, &snapshot.broker_symbol);
        string_field(&mut encoder, 11, &snapshot.venue_mic);
        string_field(&mut encoder, 12, &snapshot.board);
        string_field(&mut encoder, 13, &snapshot.trading_day.0.to_string());
        encoder.field(14, &[1]); // BrokerReported, explicitly versioned.
        string_field(&mut encoder, 15, &snapshot.source_contract_version);
        encoder.field(
            16,
            &snapshot
                .source_observed_at
                .0
                .timestamp_millis()
                .to_be_bytes(),
        );
        encoder.field(
            17,
            &snapshot
                .source_expires_at
                .0
                .timestamp_millis()
                .to_be_bytes(),
        );
        encoder.field(18, &snapshot.raw_response_sha256);
        string_field(&mut encoder, 19, &snapshot.instrument_registry_version);
        encoder.field(20, &canonical_sessions_fingerprint(&snapshot.sessions));
        encoder.finish()
    }

    fn split_canonical_broker_symbol(value: &str) -> Option<(&str, &str)> {
        let (ticker, mic) = value.rsplit_once('@')?;
        if ticker.is_empty() || mic.is_empty() || ticker.contains('@') {
            return None;
        }
        Some((ticker, mic))
    }

    fn validate_normalized_schedule_snapshot(
        availability: NormalizedScheduleAvailability,
        lifecycle_now: LifecycleInstant,
    ) -> Result<ValidatedNormalizedInstrumentScheduleSnapshot, NormalizedScheduleValidationError>
    {
        let NormalizedScheduleAvailability::Available(mut snapshot) = availability else {
            return Err(NormalizedScheduleValidationError::SourceUnavailable);
        };
        if snapshot.broker_symbol.is_empty()
            || snapshot.venue_mic.is_empty()
            || snapshot.board.is_empty()
            || snapshot.source_contract_version.is_empty()
            || snapshot.instrument_registry_version.is_empty()
            || snapshot.instrument.venue_symbol.as_deref() != Some(snapshot.broker_symbol.as_str())
            || snapshot.raw_response_sha256 == [0; 32]
            || snapshot.normalized_payload_sha256 == [0; 32]
        {
            return Err(NormalizedScheduleValidationError::MissingIdentityMetadata);
        }
        let Some((ticker, mic)) = split_canonical_broker_symbol(&snapshot.broker_symbol) else {
            return Err(NormalizedScheduleValidationError::CanonicalBrokerSymbolMismatch);
        };
        if ticker != snapshot.instrument.symbol || mic != snapshot.venue_mic {
            return Err(NormalizedScheduleValidationError::CanonicalBrokerSymbolMismatch);
        }
        if snapshot.sessions.is_empty() {
            return Err(NormalizedScheduleValidationError::EmptySessions);
        }
        if snapshot.source_observed_at.0 > snapshot.source_expires_at.0 {
            return Err(NormalizedScheduleValidationError::ObservedAfterExpiry);
        }
        if snapshot.source_observed_at.0 > lifecycle_now.0 {
            return Err(NormalizedScheduleValidationError::ObservedInFuture);
        }
        if lifecycle_now.0 > snapshot.source_expires_at.0 {
            return Err(NormalizedScheduleValidationError::Expired);
        }
        snapshot.sessions.sort_by_key(|session| {
            (
                session.start.0,
                session.end.0,
                session_type_code(session.session_type),
            )
        });
        let mut previous_end = None;
        let mut has_tradable_open = false;
        for session in &snapshot.sessions {
            if session.session_type == NormalizedSessionType::Unknown {
                return Err(NormalizedScheduleValidationError::UnknownSessionType);
            }
            if session.start.0 >= session.end.0 {
                return Err(NormalizedScheduleValidationError::InvalidInterval);
            }
            if previous_end.is_some_and(|end| session.start.0 <= end) {
                return Err(NormalizedScheduleValidationError::OverlappingIntervals);
            }
            has_tradable_open |= session.session_type == NormalizedSessionType::TradableOpen;
            previous_end = Some(session.end.0);
        }
        if !has_tradable_open {
            return Err(NormalizedScheduleValidationError::NoTradableOpen);
        }
        let payload_fingerprint = normalized_snapshot_payload_fingerprint(&snapshot);
        if payload_fingerprint != snapshot.normalized_payload_sha256 {
            return Err(NormalizedScheduleValidationError::PayloadFingerprintMismatch);
        }
        let sessions_fingerprint = canonical_sessions_fingerprint(&snapshot.sessions);
        let identity_fingerprint = payload_fingerprint;
        Ok(ValidatedNormalizedInstrumentScheduleSnapshot {
            snapshot: *snapshot,
            sessions_fingerprint,
            identity_fingerprint,
        })
    }

    fn accept_instrument_registry_evidence(
        validated: &ValidatedNormalizedInstrumentScheduleSnapshot,
        candidate: SealedInstrumentRegistryBridgeInput,
    ) -> Result<AcceptedInstrumentRegistryEvidence, RegistryBindingError> {
        let snapshot = &validated.snapshot;
        if candidate.instrument != snapshot.instrument
            || candidate.broker_symbol != snapshot.broker_symbol
            || candidate.venue_mic != snapshot.venue_mic
            || candidate.board != snapshot.board
            || candidate.registry_version != snapshot.instrument_registry_version
        {
            return Err(RegistryBindingError::IdentityMismatch);
        }
        Ok(AcceptedInstrumentRegistryEvidence {
            identity: candidate,
        })
    }

    fn project_accepted_stage4_schedule(
        evidence: &broker_core::Stage4AcceptedPaperHostEvidence,
        lifecycle_now: LifecycleInstant,
    ) -> Result<AcceptedStage4ScheduleEvidence, Stage4ScheduleProjectionError> {
        let report = evidence.report();
        if report.status != broker_core::Stage4BootstrapEvidenceReportStatus::Accepted {
            return Err(Stage4ScheduleProjectionError::NotAccepted);
        }
        let session_state = evidence.schedule_state();
        if session_state != broker_core::BrokerMarketSessionState::Open {
            return Err(Stage4ScheduleProjectionError::SessionNotOpen);
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
        let observed_at = report.checked_ts - chrono::Duration::milliseconds(age_ms);
        validate_stage4_projection_times(report.checked_ts, observed_at, lifecycle_now)?;
        if lifecycle_now.0 > evidence.required_source_expires_at() {
            return Err(Stage4ScheduleProjectionError::Expired);
        }
        let expires_at = evidence.required_source_expires_at();
        let mut source_encoder = CanonicalEncoder::new(b"stage5e-b3c-stage4-schedule-source-v1");
        source_encoder.field(1, &[6]); // Schedule
        source_encoder.field(2, &[stage4_source_status_code(schedule.source_status)]);
        source_encoder.field(
            3,
            &[stage4_freshness_status_code(schedule.freshness_status)],
        );
        source_encoder.field(4, &[u8::from(schedule.required_for_bootstrap)]);
        source_encoder.field(5, &[u8::from(schedule.blocks_bootstrap)]);
        match schedule.age_ms {
            Some(value) => {
                source_encoder.field(6, &[1]);
                source_encoder.field(7, &value.to_be_bytes());
            }
            None => source_encoder.field(6, &[0]),
        }
        source_encoder.field(8, &schedule.max_age_ms.to_be_bytes());
        source_encoder.field(9, &report.schema_version.to_be_bytes());
        source_encoder.field(10, &report.checked_ts.timestamp_millis().to_be_bytes());
        encode_instrument(&mut source_encoder, &report.target_instrument);
        let schedule_source_identity = source_encoder.finish();

        let mut encoder = CanonicalEncoder::new(b"stage5e-b3c-stage4-dynamic-session-v1");
        encode_instrument(&mut encoder, &report.target_instrument);
        encoder.field(10, &[stage4_session_state_code(session_state)]);
        encoder.field(11, &report.schema_version.to_be_bytes());
        encoder.field(12, &report.checked_ts.timestamp_millis().to_be_bytes());
        encoder.field(13, &observed_at.timestamp_millis().to_be_bytes());
        encoder.field(14, &expires_at.timestamp_millis().to_be_bytes());
        encoder.field(15, &schedule_source_identity);
        Ok(AcceptedStage4ScheduleEvidence {
            instrument: report.target_instrument.clone(),
            session_state,
            observed_at: LifecycleInstant(observed_at),
            expires_at: LifecycleInstant(expires_at),
            identity: ScheduleFingerprint(encoder.finish()),
        })
    }

    fn stage4_session_state_code(value: broker_core::BrokerMarketSessionState) -> u8 {
        match value {
            broker_core::BrokerMarketSessionState::Open => 1,
            broker_core::BrokerMarketSessionState::Closed => 2,
            broker_core::BrokerMarketSessionState::Break => 3,
            broker_core::BrokerMarketSessionState::Maintenance => 4,
            broker_core::BrokerMarketSessionState::Unknown => 255,
        }
    }

    fn stage4_source_status_code(value: broker_core::Stage4BrokerTruthSourceStatus) -> u8 {
        match value {
            broker_core::Stage4BrokerTruthSourceStatus::Present => 1,
            broker_core::Stage4BrokerTruthSourceStatus::Missing => 2,
            broker_core::Stage4BrokerTruthSourceStatus::Unavailable => 3,
            broker_core::Stage4BrokerTruthSourceStatus::DecodeFailed => 4,
            broker_core::Stage4BrokerTruthSourceStatus::Incomplete => 5,
        }
    }

    fn stage4_freshness_status_code(value: broker_core::Stage4BrokerTruthFreshnessStatus) -> u8 {
        match value {
            broker_core::Stage4BrokerTruthFreshnessStatus::Fresh => 1,
            broker_core::Stage4BrokerTruthFreshnessStatus::Stale => 2,
            broker_core::Stage4BrokerTruthFreshnessStatus::Unknown => 3,
            broker_core::Stage4BrokerTruthFreshnessStatus::Unavailable => 4,
        }
    }

    fn validate_stage4_projection_times(
        report_checked_ts: DateTime<Utc>,
        observed_at: DateTime<Utc>,
        lifecycle_now: LifecycleInstant,
    ) -> Result<(), Stage4ScheduleProjectionError> {
        if report_checked_ts > lifecycle_now.0 {
            return Err(Stage4ScheduleProjectionError::ReportCheckedInFuture);
        }
        if observed_at > lifecycle_now.0 {
            return Err(Stage4ScheduleProjectionError::ObservedInFuture);
        }
        Ok(())
    }

    fn map_trusted_schedule_window_internal(
        validated: ValidatedNormalizedInstrumentScheduleSnapshot,
        registry: AcceptedInstrumentRegistryEvidence,
        stage4: AcceptedStage4ScheduleEvidence,
        requested_bar_close: MarketBarCloseTime,
        lifecycle_now: LifecycleInstant,
    ) -> Result<Stage5eScheduleWindowEvidence, ScheduleWindowMappingError> {
        if lifecycle_now.0 > stage4.expires_at.0 {
            return Err(ScheduleWindowMappingError::Stage4Expired);
        }
        if stage4.session_state != broker_core::BrokerMarketSessionState::Open {
            return Err(ScheduleWindowMappingError::Stage4NotOpen);
        }
        if lifecycle_now.0 > validated.snapshot.source_expires_at.0 {
            return Err(ScheduleWindowMappingError::SnapshotExpired);
        }
        if stage4.instrument != validated.snapshot.instrument {
            return Err(ScheduleWindowMappingError::InstrumentMismatch);
        }
        if registry.identity.instrument != validated.snapshot.instrument
            || registry.identity.broker_symbol != validated.snapshot.broker_symbol
            || registry.identity.venue_mic != validated.snapshot.venue_mic
            || registry.identity.board != validated.snapshot.board
            || registry.identity.registry_version != validated.snapshot.instrument_registry_version
        {
            return Err(ScheduleWindowMappingError::RegistryMismatch);
        }
        let selected = validated
            .snapshot
            .sessions
            .iter()
            .find(|session| {
                session.session_type == NormalizedSessionType::TradableOpen
                    && session.start.0 <= requested_bar_close.0
                    && requested_bar_close.0 <= session.end.0
            })
            .ok_or(ScheduleWindowMappingError::NoTradableOpenForRequestedBar)?
            .clone();
        let fingerprint = deterministic_fingerprint(&validated, &registry, &stage4, &selected);
        Ok(Stage5eScheduleWindowEvidence {
            instrument: validated.snapshot.instrument,
            broker_symbol: validated.snapshot.broker_symbol,
            venue_mic: validated.snapshot.venue_mic,
            board: validated.snapshot.board,
            trading_day: validated.snapshot.trading_day,
            source_contract_version: validated.snapshot.source_contract_version,
            selected_session_type: selected.session_type,
            open_from: selected.start,
            open_until: selected.end,
            normalized_observed_at: validated.snapshot.source_observed_at,
            stage4_observed_at: stage4.observed_at,
            effective_observed_at: if validated.snapshot.source_observed_at.0 > stage4.observed_at.0
            {
                validated.snapshot.source_observed_at
            } else {
                stage4.observed_at
            },
            expires_at: if validated.snapshot.source_expires_at.0 < stage4.expires_at.0 {
                validated.snapshot.source_expires_at
            } else {
                stage4.expires_at
            },
            fingerprint,
            normalized_sessions: validated.snapshot.sessions,
            normalized_sessions_fingerprint: validated.sessions_fingerprint,
            normalized_snapshot_identity_fingerprint: validated.identity_fingerprint,
            stage4_dynamic_session_fingerprint: stage4.identity.0,
        })
    }

    fn map_trusted_schedule_projection(
        validated: ValidatedNormalizedInstrumentScheduleSnapshot,
        registry: AcceptedInstrumentRegistryEvidence,
        stage4: AcceptedStage4ScheduleEvidence,
        requested_bar_close: MarketBarCloseTime,
        lifecycle_now: LifecycleInstant,
    ) -> Result<Stage5eScheduleProjectionBridgeInput, ScheduleWindowMappingError> {
        map_trusted_schedule_window_internal(
            validated,
            registry,
            stage4,
            requested_bar_close,
            lifecycle_now,
        )
        .map(issue_schedule_projection_bridge)
    }

    /// Sole constructor for the opaque cross-module projection.
    fn issue_schedule_projection_bridge(
        schedule_window: Stage5eScheduleWindowEvidence,
    ) -> Stage5eScheduleProjectionBridgeInput {
        Stage5eScheduleProjectionBridgeInput { schedule_window }
    }

    /// Sole constructor for the linear classifier capability.
    pub(crate) fn into_stage5e_schedule_candidate_classifier(
        projection: Stage5eScheduleProjectionBridgeInput,
    ) -> Stage5eScheduleCandidateClassifier {
        Stage5eScheduleCandidateClassifier { projection }
    }

    fn classify_expected_close_grid(
        schedule_window: &Stage5eScheduleWindowEvidence,
        predecessor_close_ts: i64,
        current_close_ts: i64,
        timeframe_sec: std::num::NonZeroU32,
    ) -> Result<Stage5eScheduleSequenceClassification, Stage5eScheduleClassificationBlockReason>
    {
        let timeframe = i64::from(timeframe_sec.get());
        if timeframe <= 0 {
            return Err(Stage5eScheduleClassificationBlockReason::InvalidTimeframe);
        }
        if current_close_ts <= predecessor_close_ts {
            return Err(Stage5eScheduleClassificationBlockReason::NonMonotonicSequence);
        }
        if predecessor_close_ts.rem_euclid(timeframe) != 0
            || current_close_ts.rem_euclid(timeframe) != 0
        {
            return Err(Stage5eScheduleClassificationBlockReason::UnalignedEndpoint);
        }
        let predecessor_day = DateTime::<Utc>::from_timestamp(predecessor_close_ts, 0)
            .map(|value| value.date_naive());
        let current_day =
            DateTime::<Utc>::from_timestamp(current_close_ts, 0).map(|value| value.date_naive());
        if predecessor_day != Some(schedule_window.trading_day.0)
            || current_day != Some(schedule_window.trading_day.0)
        {
            return Err(Stage5eScheduleClassificationBlockReason::CrossTradingDay);
        }
        let steps = current_close_ts
            .checked_sub(predecessor_close_ts)
            .and_then(|delta| delta.checked_div(timeframe))
            .and_then(|count| usize::try_from(count).ok())
            .ok_or(Stage5eScheduleClassificationBlockReason::CandidateCountOverflow)?;
        if steps == 0 {
            return Err(Stage5eScheduleClassificationBlockReason::NonMonotonicSequence);
        }

        let mut classified = Vec::with_capacity(
            steps
                .checked_add(1)
                .ok_or(Stage5eScheduleClassificationBlockReason::CandidateCountOverflow)?,
        );
        for step in 0..=steps {
            let offset = i64::try_from(step)
                .ok()
                .and_then(|value| value.checked_mul(timeframe))
                .ok_or(Stage5eScheduleClassificationBlockReason::CandidateCountOverflow)?;
            let close_ts = predecessor_close_ts
                .checked_add(offset)
                .ok_or(Stage5eScheduleClassificationBlockReason::CandidateCountOverflow)?;
            let matches: Vec<_> = schedule_window
                .normalized_sessions
                .iter()
                .filter(|session| session.start.0 <= close_ts && close_ts <= session.end.0)
                .collect();
            let session = match matches.as_slice() {
                [] => {
                    return Err(
                        Stage5eScheduleClassificationBlockReason::EndpointOrCandidateUncovered,
                    );
                }
                [session] => *session,
                _ => {
                    return Err(
                        Stage5eScheduleClassificationBlockReason::EndpointOrCandidateAmbiguous,
                    );
                }
            };
            classified.push((close_ts, session));
        }

        let Some((_, predecessor_session)) = classified.first() else {
            return Err(Stage5eScheduleClassificationBlockReason::NonMonotonicSequence);
        };
        if predecessor_session.session_type != NormalizedSessionType::TradableOpen {
            return Err(Stage5eScheduleClassificationBlockReason::PredecessorCloseNotTradableOpen);
        }
        let Some((_, current_session)) = classified.last() else {
            return Err(Stage5eScheduleClassificationBlockReason::NonMonotonicSequence);
        };
        if current_session.session_type != NormalizedSessionType::TradableOpen {
            return Err(Stage5eScheduleClassificationBlockReason::CurrentCloseNotTradableOpen);
        }
        for (_, session) in classified
            .iter()
            .skip(1)
            .take(classified.len().saturating_sub(2))
        {
            match session.session_type {
                NormalizedSessionType::BreakOrClearing | NormalizedSessionType::Maintenance => {}
                NormalizedSessionType::TradableOpen => {
                    return Err(Stage5eScheduleClassificationBlockReason::InteriorTradableOpen);
                }
                NormalizedSessionType::Unknown => {
                    return Err(Stage5eScheduleClassificationBlockReason::InteriorUnknown);
                }
            }
        }

        if steps == 1 {
            return Ok(Stage5eScheduleSequenceClassification::Contiguous);
        }
        Ok(
            Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary(
                non_tradable_boundary_fingerprint(
                    schedule_window,
                    predecessor_close_ts,
                    current_close_ts,
                    timeframe_sec,
                    &classified,
                ),
            ),
        )
    }

    fn non_tradable_boundary_fingerprint(
        schedule_window: &Stage5eScheduleWindowEvidence,
        predecessor_close_ts: i64,
        current_close_ts: i64,
        timeframe_sec: std::num::NonZeroU32,
        classified: &[(i64, &NormalizedScheduleSession)],
    ) -> [u8; 32] {
        let mut encoder = CanonicalEncoder::new(b"stage5e-b3c-non-tradable-boundary-v1");
        encoder.field(1, &schedule_window.normalized_snapshot_identity_fingerprint);
        encoder.field(2, &schedule_window.normalized_sessions_fingerprint);
        string_field(&mut encoder, 3, &schedule_window.trading_day.0.to_string());
        encoder.field(4, &timeframe_sec.get().to_be_bytes());
        encoder.field(5, &predecessor_close_ts.to_be_bytes());
        encoder.field(6, &current_close_ts.to_be_bytes());
        for (close_ts, _) in classified
            .iter()
            .skip(1)
            .take(classified.len().saturating_sub(2))
        {
            encoder.field(7, &close_ts.to_be_bytes());
        }
        for (_, session) in classified {
            encoder.field(8, &[session_type_code(session.session_type)]);
            encoder.field(9, &session.start.0.to_be_bytes());
            encoder.field(10, &session.end.0.to_be_bytes());
        }
        encoder.finish()
    }

    fn validate_schedule_window_for_observed_bar(
        schedule_window: &Stage5eScheduleWindowEvidence,
        observed_bar_instrument: &broker_core::InstrumentId,
        observed_bar_close_ts: MarketBarCloseTime,
        lifecycle_now: LifecycleInstant,
    ) -> Result<(), ScheduleWindowObservedBarBindingError> {
        if schedule_window.instrument != *observed_bar_instrument {
            return Err(ScheduleWindowObservedBarBindingError::InstrumentMismatch);
        }
        if lifecycle_now.0 < schedule_window.effective_observed_at.0 {
            return Err(
                ScheduleWindowObservedBarBindingError::ClockBeforeEffectiveEvidenceObservation,
            );
        }
        if observed_bar_close_ts.0 > lifecycle_now.0.timestamp() {
            return Err(ScheduleWindowObservedBarBindingError::ObservedBarInFuture);
        }
        if lifecycle_now.0 > schedule_window.expires_at.0 {
            return Err(ScheduleWindowObservedBarBindingError::WindowExpired);
        }
        if observed_bar_close_ts.0 < schedule_window.open_from.0
            || observed_bar_close_ts.0 > schedule_window.open_until.0
        {
            return Err(ScheduleWindowObservedBarBindingError::BarOutsideInclusiveWindow);
        }
        Ok(())
    }

    fn bind_schedule_window_to_observed_live_bar(
        schedule_window: Stage5eScheduleWindowEvidence,
        observed_live_bar: super::Stage5eObservedLiveBarAfterHistory,
    ) -> Result<
        Stage5eBoundScheduleWindowForObservedLiveBar,
        Box<Stage5eScheduleWindowObservedBarBlocked>,
    > {
        bind_schedule_window_to_observed_live_bar_with_now(
            schedule_window,
            observed_live_bar,
            LifecycleInstant(Utc::now()),
        )
    }

    fn bind_schedule_window_to_observed_live_bar_with_now(
        schedule_window: Stage5eScheduleWindowEvidence,
        observed_live_bar: super::Stage5eObservedLiveBarAfterHistory,
        lifecycle_now: LifecycleInstant,
    ) -> Result<
        Stage5eBoundScheduleWindowForObservedLiveBar,
        Box<Stage5eScheduleWindowObservedBarBlocked>,
    > {
        let bar_instrument = observed_live_bar.bar.instrument.clone();
        let bar_close_ts = MarketBarCloseTime(observed_live_bar.bar_close_ts());
        if let Err(reason) = validate_schedule_window_for_observed_bar(
            &schedule_window,
            &bar_instrument,
            bar_close_ts,
            lifecycle_now,
        ) {
            return Err(Box::new(Stage5eScheduleWindowObservedBarBlocked {
                reason,
                schedule_window,
                observed_live_bar,
            }));
        }
        let binding_fingerprint = schedule_observed_bar_binding_fingerprint(
            &schedule_window,
            &bar_instrument,
            bar_close_ts,
        );
        Ok(Stage5eBoundScheduleWindowForObservedLiveBar {
            schedule_fingerprint: schedule_window.fingerprint,
            schedule_window,
            observed_live_bar,
            bar_instrument,
            bar_close_ts,
            binding_fingerprint,
        })
    }

    #[cfg(test)]
    fn bind_schedule_window_to_observed_live_bar_at(
        schedule_window: Stage5eScheduleWindowEvidence,
        observed_live_bar: super::Stage5eObservedLiveBarAfterHistory,
        lifecycle_now: LifecycleInstant,
    ) -> Result<
        Stage5eBoundScheduleWindowForObservedLiveBar,
        Box<Stage5eScheduleWindowObservedBarBlocked>,
    > {
        bind_schedule_window_to_observed_live_bar_with_now(
            schedule_window,
            observed_live_bar,
            lifecycle_now,
        )
    }

    fn schedule_observed_bar_binding_fingerprint(
        schedule_window: &Stage5eScheduleWindowEvidence,
        bar_instrument: &broker_core::InstrumentId,
        bar_close_ts: MarketBarCloseTime,
    ) -> ScheduleObservedBarBindingFingerprint {
        let mut encoder = CanonicalEncoder::new(b"stage5e-b3b-schedule-observed-bar-binding-v1");
        encoder.field(1, &schedule_window.fingerprint.0);
        encode_instrument(&mut encoder, bar_instrument);
        encoder.field(2, &bar_close_ts.0.to_be_bytes());
        encoder.field(3, b"stage5e-b3b-binding-v1");
        ScheduleObservedBarBindingFingerprint(encoder.finish())
    }

    impl Stage5eBoundScheduleWindowForObservedLiveBar {
        fn callback_count(&self) -> usize {
            self.observed_live_bar.callback_count()
        }

        fn intent_count(&self) -> usize {
            self.observed_live_bar.intent_count()
        }

        fn strategy_was_called(&self) -> bool {
            self.observed_live_bar.strategy_was_called()
        }

        fn executable_intent_created(&self) -> bool {
            self.observed_live_bar.executable_intent_created()
        }
    }

    impl Stage5eScheduleWindowObservedBarBlocked {
        fn reason(&self) -> ScheduleWindowObservedBarBindingError {
            self.reason
        }

        fn into_inputs(
            self,
        ) -> (
            Stage5eScheduleWindowEvidence,
            super::Stage5eObservedLiveBarAfterHistory,
        ) {
            (self.schedule_window, self.observed_live_bar)
        }
    }

    fn deterministic_fingerprint(
        validated: &ValidatedNormalizedInstrumentScheduleSnapshot,
        registry: &AcceptedInstrumentRegistryEvidence,
        stage4: &AcceptedStage4ScheduleEvidence,
        selected: &NormalizedScheduleSession,
    ) -> ScheduleFingerprint {
        let snapshot = &validated.snapshot;
        let mut encoder = CanonicalEncoder::new(b"stage5e-schedule-window-evidence-v2");
        encode_instrument(&mut encoder, &snapshot.instrument);
        string_field(&mut encoder, 10, &registry.identity.broker_symbol);
        string_field(&mut encoder, 11, &registry.identity.venue_mic);
        string_field(&mut encoder, 12, &registry.identity.board);
        string_field(&mut encoder, 13, &registry.identity.registry_version);
        string_field(&mut encoder, 14, &snapshot.trading_day.0.to_string());
        encoder.field(15, &[session_type_code(selected.session_type)]);
        encoder.field(16, &selected.start.0.to_be_bytes());
        encoder.field(17, &selected.end.0.to_be_bytes());
        encoder.field(18, &[1]); // BrokerReported
        string_field(&mut encoder, 19, &snapshot.source_contract_version);
        encoder.field(20, &snapshot.raw_response_sha256);
        encoder.field(21, &snapshot.normalized_payload_sha256);
        encoder.field(
            22,
            &snapshot
                .source_observed_at
                .0
                .timestamp_millis()
                .to_be_bytes(),
        );
        encoder.field(
            23,
            &snapshot
                .source_expires_at
                .0
                .timestamp_millis()
                .to_be_bytes(),
        );
        encoder.field(24, &validated.sessions_fingerprint);
        encoder.field(25, &validated.identity_fingerprint);
        encoder.field(26, &stage4.identity.0);
        encoder.field(27, &stage4.observed_at.0.timestamp_millis().to_be_bytes());
        encoder.field(28, &stage4.expires_at.0.timestamp_millis().to_be_bytes());
        ScheduleFingerprint(encoder.finish())
    }

    // STAGE5F-TEST-B3C-FACTORY-BEGIN
    #[cfg(test)]
    pub(crate) mod stage5f_test_seams {
        use super::*;

        /// Carries fixture-owned Stage 5C inputs through the existing
        /// schedule, B3B and B3C ownership transitions.
        pub(crate) fn b3c_from_sequence_inputs(
            recovered: crate::stage5c_paper_host::Stage5cPendingRecoveredPaperStrategy,
            accepted: crate::stage5c_paper_host::Stage5cAcceptedSemanticBar,
            target: broker_core::InstrumentId,
            bar_close_ts: i64,
            lifecycle_now: DateTime<Utc>,
        ) -> b3c_evidence::Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            let broker_symbol = target
                .venue_symbol
                .clone()
                .expect("Stage 5F target requires canonical venue symbol");
            let venue_mic = broker_symbol
                .rsplit_once('@')
                .map(|(_, value)| value.to_string())
                .expect("Stage 5F venue symbol requires MIC suffix");
            let mut snapshot = NormalizedInstrumentScheduleSnapshot {
                instrument: target.clone(),
                broker_symbol,
                venue_mic: venue_mic.clone(),
                board: venue_mic,
                trading_day: TradingDay(lifecycle_now.date_naive()),
                sessions: vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(bar_close_ts - 3_600),
                    end: MarketBarCloseTime(bar_close_ts + 3_600),
                }],
                source: ScheduleSourceIdentity::BrokerReported,
                source_contract_version: "stage5f-fixture-v1".to_string(),
                source_observed_at: LifecycleInstant(lifecycle_now),
                source_expires_at: LifecycleInstant(lifecycle_now + chrono::Duration::seconds(10)),
                raw_response_sha256: [0x5f; 32],
                normalized_payload_sha256: [0; 32],
                instrument_registry_version: "stage5f-fixture-registry-v1".to_string(),
            };
            snapshot.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&snapshot);
            let validated = validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(snapshot)),
                LifecycleInstant(lifecycle_now),
            )
            .expect("Stage 5F normalized schedule fixture must validate");
            let registry = SealedInstrumentRegistryBridgeInput {
                instrument: validated.snapshot.instrument.clone(),
                broker_symbol: validated.snapshot.broker_symbol.clone(),
                venue_mic: validated.snapshot.venue_mic.clone(),
                board: validated.snapshot.board.clone(),
                registry_version: validated.snapshot.instrument_registry_version.clone(),
            };
            let accepted_registry = accept_instrument_registry_evidence(&validated, registry)
                .expect("Stage 5F registry fixture must bind");
            let stage4 = AcceptedStage4ScheduleEvidence {
                instrument: target,
                session_state: broker_core::BrokerMarketSessionState::Open,
                observed_at: LifecycleInstant(lifecycle_now),
                expires_at: LifecycleInstant(lifecycle_now + chrono::Duration::seconds(10)),
                identity: ScheduleFingerprint([0x4f; 32]),
            };
            let window = map_trusted_schedule_window_internal(
                validated,
                accepted_registry,
                stage4,
                MarketBarCloseTime(bar_close_ts),
                LifecycleInstant(lifecycle_now),
            )
            .expect("Stage 5F schedule window must map");
            let projection = issue_schedule_projection_bridge(window);
            let observed =
                crate::stage5c_paper_host::stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered,
                    accepted,
                    projection,
                    lifecycle_now,
                )
                .unwrap_or_else(|blocked| {
                    panic!(
                        "Stage 5F live bar sequence must be observed: {:?}",
                        blocked.reason()
                    )
                });
            let b3b =
                bind_schedule_window_sequence_to_observed_live_bar_at(observed, lifecycle_now)
                    .unwrap_or_else(|_| panic!("Stage 5F sequence must bind to B3B"));
            b3c_evidence::bind_session_calendar_sequence_from_b3b_at(b3b, lifecycle_now)
                .unwrap_or_else(|_| panic!("Stage 5F B3B must bind to B3C"))
        }
    }
    // STAGE5F-TEST-B3C-FACTORY-END

    #[cfg(test)]
    mod tests {
        use super::*;
        use broker_core::{
            BrokerAccountId, BrokerInstrumentSpec, BrokerKind, BrokerMarketSessionState,
            BrokerSymbol, BrokerTruthSnapshot, Exchange, InstrumentId, InstrumentMapEntry,
            InternalSymbol, Market, Money, Stage4AdoptionDisposition,
            Stage4BootstrapEvidenceSourceStatusSection, Stage4BrokerTruthBootstrapInput,
            Stage4BrokerTruthFreshnessInput, Stage4BrokerTruthSafetyBoundary,
            Stage4BrokerTruthSourceStatus,
        };
        use chrono::TimeZone;
        use rust_decimal::Decimal;

        fn instrument() -> InstrumentId {
            InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            }
        }

        fn snapshot(now: DateTime<Utc>) -> NormalizedInstrumentScheduleSnapshot {
            let mut snapshot = NormalizedInstrumentScheduleSnapshot {
                instrument: instrument(),
                broker_symbol: "IMOEXF@RTSX".to_string(),
                venue_mic: "RTSX".to_string(),
                board: "RTSX".to_string(),
                trading_day: TradingDay(NaiveDate::from_ymd_opt(2026, 7, 24).unwrap()),
                sessions: vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(100),
                    end: MarketBarCloseTime(200),
                }],
                source: ScheduleSourceIdentity::BrokerReported,
                source_contract_version: "fixture-v1".to_string(),
                source_observed_at: LifecycleInstant(now),
                source_expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                raw_response_sha256: [1; 32],
                normalized_payload_sha256: [0; 32],
                instrument_registry_version: "fixture-registry-v1".to_string(),
            };
            snapshot.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&snapshot);
            snapshot
        }

        fn validated(now: DateTime<Utc>) -> ValidatedNormalizedInstrumentScheduleSnapshot {
            validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(snapshot(now))),
                LifecycleInstant(now),
            )
            .unwrap()
        }

        fn registry(
            validated: &ValidatedNormalizedInstrumentScheduleSnapshot,
        ) -> SealedInstrumentRegistryBridgeInput {
            SealedInstrumentRegistryBridgeInput {
                instrument: validated.snapshot.instrument.clone(),
                broker_symbol: validated.snapshot.broker_symbol.clone(),
                venue_mic: validated.snapshot.venue_mic.clone(),
                board: validated.snapshot.board.clone(),
                registry_version: validated.snapshot.instrument_registry_version.clone(),
            }
        }

        fn stage4(now: DateTime<Utc>, instrument: InstrumentId) -> AcceptedStage4ScheduleEvidence {
            AcceptedStage4ScheduleEvidence {
                instrument,
                session_state: broker_core::BrokerMarketSessionState::Open,
                observed_at: LifecycleInstant(now),
                expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                identity: ScheduleFingerprint([7; 32]),
            }
        }

        fn canonical_stage4_paper_host_evidence(
            now: DateTime<Utc>,
            schedule_state: BrokerMarketSessionState,
        ) -> Result<
            broker_core::Stage4AcceptedPaperHostEvidence,
            broker_core::Stage4AcceptedPaperHostEvidenceError,
        > {
            let target = instrument();
            let truth = BrokerTruthSnapshot {
                account_id: BrokerAccountId::new("ACC_TEST_0001"),
                orders: Vec::new(),
                positions: Vec::new(),
                cash: None,
                trades: Vec::new(),
                instruments: vec![BrokerInstrumentSpec {
                    instrument: InstrumentMapEntry {
                        internal_symbol: InternalSymbol("IMOEXF".to_string()),
                        broker: BrokerKind::Finam,
                        broker_symbol: BrokerSymbol("IMOEXF@RTSX".to_string()),
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
                    broker_asset_id: Some("ASSET_TEST_1".to_string()),
                    board: Some("RTSX".to_string()),
                    long_initial_margin: Some(Money::new(5000, 0)),
                    short_initial_margin: Some(Money::new(5000, 0)),
                }],
                received_ts: now,
            };
            let validated = broker_core::stage4_bootstrap::validate_stage4_broker_truth_bootstrap(
                Stage4BrokerTruthBootstrapInput {
                    broker_truth: &truth,
                    broker_truth_source_status: Stage4BrokerTruthSourceStatus::Present,
                    target_instrument: target,
                    restored_runtime_state: None,
                    freshness:
                        Stage4BrokerTruthFreshnessInput::synthetic_all_sections_fresh_for_tests(
                            now, 60_000,
                        ),
                    schedule_state,
                    adoption: Stage4AdoptionDisposition::default(),
                    external_issues: Vec::new(),
                    safety_boundary: Stage4BrokerTruthSafetyBoundary::closed(),
                    checked_ts: now,
                },
            );
            let source_sections = validated
                .freshness
                .sections
                .iter()
                .map(|section| Stage4BootstrapEvidenceSourceStatusSection {
                    section: section.section,
                    source_status: Stage4BrokerTruthSourceStatus::Present,
                    required_for_bootstrap: section.required_for_bootstrap,
                })
                .collect::<Vec<_>>();
            broker_core::stage4_bootstrap::build_stage4_accepted_paper_host_evidence(
                &validated,
                &source_sections,
            )
        }

        fn window_for_bar(
            now: DateTime<Utc>,
            target: InstrumentId,
            open_from: i64,
            open_until: i64,
        ) -> Stage5eScheduleWindowEvidence {
            let mut snapshot = snapshot(now);
            snapshot.instrument = target.clone();
            snapshot.broker_symbol = target
                .venue_symbol
                .clone()
                .expect("test instrument must have canonical venue symbol");
            snapshot.sessions = vec![NormalizedScheduleSession {
                session_type: NormalizedSessionType::TradableOpen,
                start: MarketBarCloseTime(open_from),
                end: MarketBarCloseTime(open_until),
            }];
            snapshot.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&snapshot);
            let validated = validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(snapshot)),
                LifecycleInstant(now),
            )
            .unwrap();
            let accepted_registry =
                accept_instrument_registry_evidence(&validated, registry(&validated)).unwrap();
            map_trusted_schedule_window_internal(
                validated,
                accepted_registry,
                stage4(now, target),
                MarketBarCloseTime(open_from),
                LifecycleInstant(now),
            )
            .unwrap()
        }

        fn window_for_sessions(
            now: DateTime<Utc>,
            sessions: Vec<NormalizedScheduleSession>,
            requested_bar_close: i64,
        ) -> Stage5eScheduleWindowEvidence {
            let target = instrument();
            let mut snapshot = snapshot(now);
            snapshot.sessions = sessions;
            snapshot.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&snapshot);
            let validated = validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(snapshot)),
                LifecycleInstant(now),
            )
            .unwrap();
            let accepted_registry =
                accept_instrument_registry_evidence(&validated, registry(&validated)).unwrap();
            map_trusted_schedule_window_internal(
                validated,
                accepted_registry,
                stage4(now, target),
                MarketBarCloseTime(requested_bar_close),
                LifecycleInstant(now),
            )
            .unwrap()
        }

        fn observed_live_bar(
            lifecycle_now: DateTime<Utc>,
            bar_close_ts: i64,
        ) -> super::super::Stage5eObservedLiveBarAfterHistory {
            crate::stage5c_paper_host::stage5e_test_observed_live_bar_after_history_at(
                lifecycle_now,
                bar_close_ts,
            )
        }

        fn canonical_b3c_receipt(
            now: DateTime<Utc>,
        ) -> b3c_evidence::Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            let current = now.timestamp();
            let predecessor = current - 600;
            let projection = issue_schedule_projection_bridge(window_for_sessions(
                now,
                vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(current - 3_600),
                    end: MarketBarCloseTime(current + 3_600),
                }],
                current,
            ));
            let (recovered, accepted) =
                crate::stage5c_paper_host::stage5e_test_sequence_inputs(now, predecessor, current);
            let observed =
                crate::stage5c_paper_host::stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered, accepted, projection, now,
                )
                .unwrap_or_else(|_| panic!("canonical sequence must be accepted"));
            let b3b = bind_schedule_window_sequence_to_observed_live_bar_at(observed, now)
                .unwrap_or_else(|_| panic!("canonical sequence must bind to B3B"));
            b3c_evidence::bind_session_calendar_sequence_from_b3b_at(b3b, now)
                .unwrap_or_else(|_| panic!("canonical B3B receipt must bind to B3C"))
        }

        fn canonical_nonempty_intent_b3c_receipt(
            now: DateTime<Utc>,
        ) -> b3c_evidence::Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            let current = now.timestamp();
            let predecessor = current - 600;
            let projection = issue_schedule_projection_bridge(window_for_sessions(
                now,
                vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(current - 3_600),
                    end: MarketBarCloseTime(current + 3_600),
                }],
                current,
            ));
            let (recovered, accepted) =
                crate::stage5c_paper_host::stage5e_test_nonempty_intent_sequence_inputs(
                    now,
                    predecessor,
                    current,
                );
            let observed =
                crate::stage5c_paper_host::stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered, accepted, projection, now,
                )
                .unwrap_or_else(|_| panic!("source-produced signal sequence must be accepted"));
            let b3b = bind_schedule_window_sequence_to_observed_live_bar_at(observed, now)
                .unwrap_or_else(|_| panic!("source-produced signal sequence must bind to B3B"));
            b3c_evidence::bind_session_calendar_sequence_from_b3b_at(b3b, now)
                .unwrap_or_else(|_| panic!("source-produced signal B3B must bind to B3C"))
        }

        fn canonical_zero_intent_escrow(
            now: DateTime<Utc>,
        ) -> crate::stage5e_no_io_lifecycle::callback_authority::Stage5ePaperCallbackResultEscrow
        {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let authority = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("canonical B3C receipt must issue authority"));
            invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("canonical callback must reach escrow"))
        }

        #[test]
        fn validation_is_fail_closed_and_canonicalizes_unsorted_sessions() {
            let now = Utc::now();
            assert!(matches!(
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::ScheduleSourceUnavailable,
                    LifecycleInstant(now),
                ),
                Err(NormalizedScheduleValidationError::SourceUnavailable)
            ));
            let mut unsorted = snapshot(now);
            unsorted.sessions.push(NormalizedScheduleSession {
                session_type: NormalizedSessionType::Maintenance,
                start: MarketBarCloseTime(300),
                end: MarketBarCloseTime(400),
            });
            unsorted.sessions.reverse();
            unsorted.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&unsorted);
            assert!(validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(unsorted)),
                LifecycleInstant(now),
            )
            .is_ok());
            let mut no_open = snapshot(now);
            no_open.sessions[0].session_type = NormalizedSessionType::Maintenance;
            no_open.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&no_open);
            assert!(matches!(
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::Available(Box::new(no_open)),
                    LifecycleInstant(now),
                ),
                Err(NormalizedScheduleValidationError::NoTradableOpen)
            ));
            let mut tampered = snapshot(now);
            tampered.board = "OTHER".to_string();
            assert!(matches!(
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::Available(Box::new(tampered)),
                    LifecycleInstant(now),
                ),
                Err(NormalizedScheduleValidationError::PayloadFingerprintMismatch)
            ));
            let mut non_canonical_symbol = snapshot(now);
            non_canonical_symbol.venue_mic = "MOEX".to_string();
            non_canonical_symbol.normalized_payload_sha256 =
                normalized_snapshot_payload_fingerprint(&non_canonical_symbol);
            assert!(matches!(
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::Available(Box::new(non_canonical_symbol)),
                    LifecycleInstant(now),
                ),
                Err(NormalizedScheduleValidationError::CanonicalBrokerSymbolMismatch)
            ));
            let mut shared_inclusive_endpoint = snapshot(now);
            shared_inclusive_endpoint
                .sessions
                .push(NormalizedScheduleSession {
                    session_type: NormalizedSessionType::BreakOrClearing,
                    start: MarketBarCloseTime(200),
                    end: MarketBarCloseTime(300),
                });
            shared_inclusive_endpoint.normalized_payload_sha256 =
                normalized_snapshot_payload_fingerprint(&shared_inclusive_endpoint);
            assert!(matches!(
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::Available(Box::new(shared_inclusive_endpoint,)),
                    LifecycleInstant(now),
                ),
                Err(NormalizedScheduleValidationError::OverlappingIntervals)
            ));
        }

        #[test]
        fn mapper_consumes_validated_snapshot_registry_and_stage4_identity() {
            let now = Utc::now();
            let validated_snapshot = validated(now);
            let accepted_registry = accept_instrument_registry_evidence(
                &validated_snapshot,
                registry(&validated_snapshot),
            )
            .unwrap();
            let evidence = map_trusted_schedule_window_internal(
                validated_snapshot,
                accepted_registry,
                stage4(now, instrument()),
                MarketBarCloseTime(150),
                LifecycleInstant(now),
            )
            .unwrap();
            assert_eq!(
                evidence.selected_session_type,
                NormalizedSessionType::TradableOpen
            );
            assert_eq!(evidence.open_from, MarketBarCloseTime(100));
            assert_eq!(evidence.open_until, MarketBarCloseTime(200));
            assert_eq!(evidence.normalized_observed_at, LifecycleInstant(now));
            assert_eq!(evidence.stage4_observed_at, LifecycleInstant(now));
            assert_eq!(evidence.effective_observed_at, LifecycleInstant(now));

            let bridge_snapshot = validated(now);
            let accepted_registry =
                accept_instrument_registry_evidence(&bridge_snapshot, registry(&bridge_snapshot))
                    .unwrap();
            let bridge = map_trusted_schedule_projection(
                bridge_snapshot,
                accepted_registry,
                stage4(now, instrument()),
                MarketBarCloseTime(150),
                LifecycleInstant(now),
            )
            .unwrap();
            assert_eq!(
                bridge.schedule_window.instrument,
                instrument(),
                "opaque bridge must retain the exact accepted projection"
            );
        }

        #[test]
        fn discrete_grid_classifier_accepts_contiguous_and_non_tradable_boundary_only() {
            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let current = now.timestamp();
            let timeframe = std::num::NonZeroU32::new(600).unwrap();
            let contiguous_window = window_for_sessions(
                now,
                vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(current - 3_600),
                    end: MarketBarCloseTime(current + 3_600),
                }],
                current,
            );
            let contiguous =
                classify_expected_close_grid(&contiguous_window, current - 600, current, timeframe)
                    .expect("contiguous grid must classify");
            assert!(matches!(
                contiguous,
                Stage5eScheduleSequenceClassification::Contiguous
            ));

            let boundary_window = window_for_sessions(
                now,
                vec![
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current - 5_400),
                        end: MarketBarCloseTime(current - 1_800),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::BreakOrClearing,
                        start: MarketBarCloseTime(current - 1_200),
                        end: MarketBarCloseTime(current - 600),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current),
                        end: MarketBarCloseTime(current + 3_600),
                    },
                ],
                current,
            );
            let boundary =
                classify_expected_close_grid(&boundary_window, current - 1_800, current, timeframe)
                    .expect("non-tradable interior grid must classify");
            assert!(matches!(
                boundary,
                Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary(value)
                    if value != [0; 32]
            ));

            let blocked_window = window_for_sessions(
                now,
                vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(current - 5_400),
                    end: MarketBarCloseTime(current + 3_600),
                }],
                current,
            );
            let blocked =
                classify_expected_close_grid(&blocked_window, current - 1_800, current, timeframe)
                    .expect_err("tradable interior candidate must block");
            assert_eq!(
                blocked,
                Stage5eScheduleClassificationBlockReason::InteriorTradableOpen
            );

            let uncovered_window = window_for_sessions(
                now,
                vec![
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current - 5_400),
                        end: MarketBarCloseTime(current - 1_800),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current),
                        end: MarketBarCloseTime(current + 3_600),
                    },
                ],
                current,
            );
            assert_eq!(
                classify_expected_close_grid(
                    &uncovered_window,
                    current - 1_800,
                    current,
                    timeframe,
                ),
                Err(Stage5eScheduleClassificationBlockReason::EndpointOrCandidateUncovered)
            );
            let non_open_predecessor = window_for_sessions(
                now,
                vec![
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::Maintenance,
                        start: MarketBarCloseTime(current - 1_800),
                        end: MarketBarCloseTime(current - 600),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current),
                        end: MarketBarCloseTime(current + 3_600),
                    },
                ],
                current,
            );
            assert_eq!(
                classify_expected_close_grid(
                    &non_open_predecessor,
                    current - 1_800,
                    current,
                    timeframe,
                ),
                Err(Stage5eScheduleClassificationBlockReason::PredecessorCloseNotTradableOpen)
            );
            assert_eq!(
                classify_expected_close_grid(
                    &contiguous_window,
                    current - 86_400,
                    current,
                    timeframe,
                ),
                Err(Stage5eScheduleClassificationBlockReason::CrossTradingDay)
            );
        }

        #[test]
        fn sealed_stage5c_b3b_b3c_path_revalidates_exact_min_expiry_without_io() {
            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let current = now.timestamp();
            let predecessor = current - 1_800;
            let projection = issue_schedule_projection_bridge(window_for_sessions(
                now,
                vec![
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current - 5_400),
                        end: MarketBarCloseTime(predecessor),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::Maintenance,
                        start: MarketBarCloseTime(current - 1_200),
                        end: MarketBarCloseTime(current - 600),
                    },
                    NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(current),
                        end: MarketBarCloseTime(current + 3_600),
                    },
                ],
                current,
            ));
            let (recovered, accepted) =
                crate::stage5c_paper_host::stage5e_test_sequence_inputs(now, predecessor, current);
            let observed = match
                crate::stage5c_paper_host::stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered, accepted, projection, now,
                )
            {
                Ok(observed) => observed,
                Err(_) => panic!("canonical sequence must be accepted"),
            };
            let b3b = match bind_schedule_window_sequence_to_observed_live_bar_at(observed, now) {
                Ok(b3b) => b3b,
                Err(_) => panic!("fresh canonical sequence must bind to B3B"),
            };
            assert_eq!(
                b3b.effective_expires_at,
                now + chrono::Duration::seconds(10)
            );
            assert_ne!(b3b.event_key_fingerprint, [0; 32]);
            let b3c = match b3c_evidence::bind_session_calendar_sequence_from_b3b_at(b3b, now) {
                Ok(b3c) => b3c,
                Err(_) => panic!("fresh B3B receipt must bind to B3C"),
            };
            assert_eq!(
                b3c.effective_expires_at,
                now + chrono::Duration::seconds(10)
            );
            assert_eq!(b3c.effective_observed_at, now);
            assert_eq!(b3c.bound_at, now);
            assert_ne!(b3c.continuation_binding_id, [0; 32]);
            assert_eq!(b3c.b3b.payload.bar_close_ts, current);
        }

        #[test]
        fn b3b_preflight_block_returns_original_receipt_and_retry_preserves_state() {
            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let current = now.timestamp();
            let projection = issue_schedule_projection_bridge(window_for_sessions(
                now,
                vec![NormalizedScheduleSession {
                    session_type: NormalizedSessionType::TradableOpen,
                    start: MarketBarCloseTime(current - 3_600),
                    end: MarketBarCloseTime(current + 3_600),
                }],
                current,
            ));
            let (recovered, accepted) = crate::stage5c_paper_host::stage5e_test_sequence_inputs(
                now,
                current - 600,
                current,
            );
            let expected_state =
                crate::stage5c_paper_host::stage5e_test_pending_recovered_state_fingerprint(
                    &recovered,
                );
            let observed = match crate::stage5c_paper_host::
                stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered, accepted, projection, now,
                ) {
                Ok(observed) => observed,
                Err(_) => panic!("contiguous sequence must be accepted"),
            };
            let blocked = match bind_schedule_window_sequence_to_observed_live_bar_at(
                observed,
                now - chrono::Duration::milliseconds(1),
            ) {
                Ok(_) => panic!("pre-observation B3B clock must block before ownership transfer"),
                Err(blocked) => blocked,
            };
            assert_eq!(
                blocked.reason(),
                Stage5eB3bBindingBlockReason::ClockBeforeEffectiveObservation
            );
            assert_eq!(
                blocked.disposition(),
                Stage5eB3bBlockDisposition::RetrySameReceipt
            );
            let observed = blocked.into_retry();
            let bound = match bind_schedule_window_sequence_to_observed_live_bar_at(observed, now) {
                Ok(bound) => bound,
                Err(_) => panic!("the exact returned Stage 5C receipt must retry successfully"),
            };
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(
                    &bound.payload.strategy,
                ),
                expected_state,
                "B3B block/retry must not mutate or replace strategy state"
            );
            assert_eq!(
                Stage5eB3bBindingBlockReason::EvidenceExpired.disposition(),
                Stage5eB3bBlockDisposition::RefreshScheduleRequired
            );
            assert_eq!(
                Stage5eB3bBindingBlockReason::SequenceIdentityMissing.disposition(),
                Stage5eB3bBlockDisposition::TerminalIntegrityBlock
            );
        }

        #[test]
        fn canonical_stage4_to_b3c_chain_uses_real_accepted_evidence_without_io() {
            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let current = now.timestamp();
            let predecessor = current - 600;

            for non_open in [
                BrokerMarketSessionState::Closed,
                BrokerMarketSessionState::Break,
                BrokerMarketSessionState::Maintenance,
            ] {
                let evidence = canonical_stage4_paper_host_evidence(now, non_open)
                    .expect("canonical non-open Stage 4 evidence must still be constructible");
                assert!(matches!(
                    project_accepted_stage4_schedule(&evidence, LifecycleInstant(now)),
                    Err(Stage4ScheduleProjectionError::SessionNotOpen)
                ));
            }

            let stage4_evidence =
                canonical_stage4_paper_host_evidence(now, BrokerMarketSessionState::Open)
                    .expect("the canonical no-live Stage 4 chain must issue accepted evidence");
            let accepted_stage4 =
                project_accepted_stage4_schedule(&stage4_evidence, LifecycleInstant(now))
                    .expect("accepted dynamic Open evidence must project");

            let mut normalized = snapshot(now);
            normalized.sessions = vec![NormalizedScheduleSession {
                session_type: NormalizedSessionType::TradableOpen,
                start: MarketBarCloseTime(current - 3_600),
                end: MarketBarCloseTime(current + 3_600),
            }];
            normalized.normalized_payload_sha256 =
                normalized_snapshot_payload_fingerprint(&normalized);
            let validated = validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(normalized)),
                LifecycleInstant(now),
            )
            .expect("broker-reported normalized schedule must validate");
            let accepted_registry =
                accept_instrument_registry_evidence(&validated, registry(&validated))
                    .expect("instrument registry identity must bind exactly");
            let projection = map_trusted_schedule_projection(
                validated,
                accepted_registry,
                accepted_stage4,
                MarketBarCloseTime(current),
                LifecycleInstant(now),
            )
            .expect("real Stage 4 and normalized schedule evidence must map");

            let (recovered, accepted_bar) =
                crate::stage5c_paper_host::stage5e_test_sequence_inputs(now, predecessor, current);
            let expected_state =
                crate::stage5c_paper_host::stage5e_test_pending_recovered_state_fingerprint(
                    &recovered,
                );
            let observed = match crate::stage5c_paper_host::
                stage5e_test_observe_live_bar_with_sequence_evidence_at(
                    recovered,
                    accepted_bar,
                    projection,
                    now,
                ) {
                Ok(observed) => observed,
                Err(_) => panic!("real trusted inputs must issue one sealed sequence receipt"),
            };
            let b3b = match bind_schedule_window_sequence_to_observed_live_bar_at(observed, now) {
                Ok(b3b) => b3b,
                Err(_) => panic!("borrowed preflight must pass before B3B ownership transfer"),
            };
            assert_eq!(
                b3b.effective_expires_at,
                now + chrono::Duration::seconds(10),
                "B3B expiry must be the exact minimum of the independent evidence"
            );
            let b3c = match b3c_evidence::bind_session_calendar_sequence_from_b3b_at(b3b, now) {
                Ok(b3c) => b3c,
                Err(_) => panic!("the canonical trusted chain must reach B3C"),
            };
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(
                    &b3c.b3b.payload.strategy,
                ),
                expected_state,
                "the no-callback chain must leave strategy state unchanged"
            );
            assert_eq!(
                b3c.effective_expires_at,
                now + chrono::Duration::seconds(10)
            );
            assert_ne!(b3c.continuation_binding_id, [0; 32]);

            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let authority =
                crate::stage5e_no_io_lifecycle::callback_authority::issue_stage5e_callback_authority_at(
                    b3c, now,
                )
                .unwrap_or_else(|_| panic!("canonical Stage 4 chain must issue callback authority"));
            let escrow =
                crate::stage5e_no_io_lifecycle::callback_authority::invoke_stage5e_authorized_paper_callback_at(
                    authority, now,
                )
                .unwrap_or_else(|_| panic!("canonical Stage 4 chain must reach opaque escrow"));
            assert_eq!(escrow.test_callback_count(), 1);
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                1
            );
            let (schedule_id, event_key, continuation_id, sequence_id, bound_at) =
                escrow.test_audit_proof_vector();
            for identity in [schedule_id, event_key, continuation_id, sequence_id] {
                assert_ne!(identity, [0; 32]);
            }
            assert!(bound_at <= now);
            let (strategy_id, account_id, target_instrument, bar_identity, close_ts) =
                escrow.test_attribution_binding_vector();
            assert_eq!(strategy_id, "stage5e_test");
            assert_eq!(
                account_id,
                broker_core::BrokerAccountId::new("ACC_TEST_0001")
            );
            assert_eq!(target_instrument, instrument());
            assert_ne!(bar_identity, [0; 32]);
            assert_eq!(close_ts, current);
        }

        #[test]
        fn b3d_authority_issue_is_linear_exact_and_callback_free() {
            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let b3c = canonical_b3c_receipt(now);
            let expected_ownership = b3c.test_ownership_fingerprint();
            let authority =
                crate::stage5e_no_io_lifecycle::callback_authority::issue_stage5e_callback_authority_at(
                    b3c, now,
                )
                .unwrap_or_else(|_| panic!("fresh canonical B3C receipt must issue authority"));
            assert_ne!(authority.test_authority_id(), [0; 32]);
            assert_eq!(authority.test_issued_at(), now);
            assert_eq!(authority.test_effective_observed_at(), now);
            assert_eq!(
                authority.test_authority_expires_at(),
                now + chrono::Duration::seconds(10)
            );
            assert_eq!(authority.test_ownership_fingerprint(), expected_ownership);
            assert_eq!(authority.test_callback_count(), 0);
            assert_eq!(authority.test_intent_count(), 0);
        }

        #[test]
        fn b3e_private_callback_invokes_exactly_once_and_retains_opaque_escrow() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let authority = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("canonical B3C receipt must issue authority"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("canonical callback chain must reach escrow"));
            assert_eq!(escrow.test_callback_count(), 1);
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                1
            );
            assert_eq!(escrow.test_callback_invoked_at(), now);
            assert!(!escrow.test_has_validation_error());
            assert!(!escrow.test_strategy_state_fingerprint().is_empty());
            let (recovered_ts, authority_id, bar_identity) = escrow.test_retained_ownership();
            assert!(recovered_ts <= now);
            assert_ne!(authority_id, [0; 32]);
            assert_ne!(bar_identity, [0; 32]);
            assert_eq!(escrow.test_attribution_ownership_shape(), (0, 0, false));
            let (close_ts, origin, execution_eligible, retained_bar_identity) =
                escrow.test_retained_bar_metadata();
            assert_eq!(close_ts, now.timestamp());
            assert_eq!(origin, broker_core::HybridRuntimeBarOrigin::Live);
            assert!(execution_eligible);
            assert_eq!(retained_bar_identity, bar_identity);
        }

        #[test]
        fn b3e_actual_authorized_callback_retains_nonempty_intents_only_in_opaque_escrow() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc.with_ymd_and_hms(2026, 7, 24, 7, 0, 0).single().unwrap();
            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let authority = issue_stage5e_callback_authority_at(
                canonical_nonempty_intent_b3c_receipt(now),
                now,
            )
            .unwrap_or_else(|_| panic!("source-produced signal must issue authority"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("source-produced signal callback must reach escrow"));
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                1
            );
            assert_eq!(escrow.test_callback_count(), 1);
            assert_eq!(
                escrow.test_intent_count(),
                1,
                "the actual strategy callback must produce and retain its intent in opaque escrow"
            );
            assert!(!escrow.test_has_validation_error());
        }

        #[test]
        fn b3e_callback_validation_error_is_retained_inside_escrow() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let mut b3c = canonical_b3c_receipt(now);
            b3c.b3b
                .payload
                .accepted_semantic_bar
                .stage5e_test_force_callback_validation_error();
            let authority = issue_stage5e_callback_authority_at(b3c, now)
                .unwrap_or_else(|_| panic!("outer authority evidence remains valid"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("callback validation error must remain in escrow"));
            assert_eq!(escrow.test_callback_count(), 1);
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                1
            );
            assert_eq!(escrow.test_intent_count(), 0);
            assert!(escrow.test_has_validation_error());
        }

        #[test]
        fn b3f_canonical_zero_intent_escrow_settles_once_with_one_entry_history() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::validate_and_settle_stage5e_paper_callback_escrow,
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let authority = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("canonical B3C receipt must issue authority"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("canonical callback must reach escrow"));
            let expected_state_fingerprint = escrow.test_strategy_state_fingerprint();
            let receipt = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .unwrap_or_else(|_| panic!("canonical zero-intent escrow must settle"));
            let (request_ids, intent_count, history_len, canonical_history, state_fingerprint) =
                receipt.test_identity_proof_shape();
            assert!(request_ids.is_empty());
            assert_eq!(intent_count, 0);
            assert_eq!(history_len, 1);
            assert!(canonical_history);
            assert_eq!(state_fingerprint, expected_state_fingerprint);
            assert_ne!(receipt.test_settlement_identity(), [0; 32]);
        }

        #[test]
        fn b3f_positive_settlement_preflight_accepts_canonical_escrow() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::validate_and_settle_stage5e_paper_callback_escrow;

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let receipt = validate_and_settle_stage5e_paper_callback_escrow(
                canonical_zero_intent_escrow(now),
            )
            .unwrap_or_else(|_| panic!("canonical settlement preflight must pass"));
            assert_ne!(receipt.test_settlement_identity(), [0; 32]);
        }

        #[test]
        fn b3f_stage5c_preflight_validator_produces_all_nine_exact_mismatches() {
            use crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch as Mismatch;
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::test_validate_stage5c_preflight_binding,
                Stage5eB3fPreflightTestMutation as Mutation,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            assert!(
                test_validate_stage5c_preflight_binding(&canonical_zero_intent_escrow(now)).is_ok()
            );
            let cases = [
                (Mutation::StrategyId, Mismatch::StrategyId),
                (Mutation::AccountId, Mismatch::AccountId),
                (Mutation::FullInstrumentId, Mismatch::FullInstrumentId),
                (Mutation::SemanticBarIdentity, Mismatch::SemanticBarIdentity),
                (Mutation::AcceptedBarClose, Mismatch::AcceptedBarClose),
                (Mutation::AuditEventKey, Mismatch::AuditEventKey),
                (Mutation::PaperMode, Mismatch::PaperMode),
                (Mutation::AcceptedBarOrigin, Mismatch::AcceptedBarOrigin),
                (
                    Mutation::ExecutionEligibility,
                    Mismatch::ExecutionEligibility,
                ),
            ];
            fn mismatch_tag(mismatch: Mismatch) -> &'static str {
                match mismatch {
                    Mismatch::StrategyId => "strategy_id",
                    Mismatch::AccountId => "account_id",
                    Mismatch::FullInstrumentId => "full_instrument_id",
                    Mismatch::SemanticBarIdentity => "semantic_bar_identity",
                    Mismatch::AcceptedBarClose => "accepted_bar_close",
                    Mismatch::AuditEventKey => "audit_event_key",
                    Mismatch::PaperMode => "paper_mode",
                    Mismatch::AcceptedBarOrigin => "accepted_bar_origin",
                    Mismatch::ExecutionEligibility => "execution_eligibility",
                }
            }
            for (mutation, expected) in cases {
                let mut escrow = canonical_zero_intent_escrow(now);
                escrow.test_corrupt_stage5c_preflight_binding(mutation);
                let actual = test_validate_stage5c_preflight_binding(&escrow)
                    .err()
                    .unwrap_or_else(|| panic!("validator must reject {mutation:?}"));
                assert_eq!(mismatch_tag(actual), mismatch_tag(expected));
            }
        }

        #[test]
        fn b3f_callback_before_retained_close_is_terminal_chronology_mismatch() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::{
                validate_and_settle_stage5e_paper_callback_escrow,
                Stage5ePaperSettlementTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut escrow = canonical_zero_intent_escrow(now);
            escrow.test_set_callback_before_retained_close();
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("callback before retained close must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::ChronologyMismatch
            );
            assert_eq!(terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_retained_close_after_authority_issue_is_terminal_chronology_mismatch() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::{
                validate_and_settle_stage5e_paper_callback_escrow,
                Stage5ePaperSettlementTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut escrow = canonical_zero_intent_escrow(now);
            escrow.test_force_retained_close_after_issue();
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("retained close after issue must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::ChronologyMismatch
            );
            assert_eq!(terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_b3c_outer_chronology_drift_is_terminal_chronology_mismatch() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::{
                validate_and_settle_stage5e_paper_callback_escrow,
                Stage5ePaperSettlementTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut escrow = canonical_zero_intent_escrow(now);
            escrow.test_corrupt_b3c_outer_chronology_equality();
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("B3C/outer chronology drift must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::ChronologyMismatch
            );
            assert_eq!(terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_same_wrong_stored_authority_ids_fail_canonical_recomputation() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::{
                validate_and_settle_stage5e_paper_callback_escrow,
                Stage5ePaperSettlementTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut escrow = canonical_zero_intent_escrow(now);
            escrow.test_set_both_authority_ids_same_wrong_nonzero();
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("same wrong stored authority IDs must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::IdentityMismatch
            );
            assert_eq!(terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_canonical_authority_input_drift_without_new_id_is_identity_mismatch() {
            use crate::stage5e_no_io_lifecycle::callback_authority::callback_settlement::{
                validate_and_settle_stage5e_paper_callback_escrow,
                Stage5ePaperSettlementTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut escrow = canonical_zero_intent_escrow(now);
            escrow.test_corrupt_canonical_authority_input_without_recomputing_id();
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("canonical authority input drift must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::IdentityMismatch
            );
            assert_eq!(terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_source_produced_intent_preserves_ordered_request_ids_and_exact_count() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::validate_and_settle_stage5e_paper_callback_escrow,
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc.with_ymd_and_hms(2026, 7, 24, 7, 0, 0).single().unwrap();
            let authority = issue_stage5e_callback_authority_at(
                canonical_nonempty_intent_b3c_receipt(now),
                now,
            )
            .unwrap_or_else(|_| panic!("source-produced signal must issue authority"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("source-produced callback must reach escrow"));
            let expected_state_fingerprint = escrow.test_strategy_state_fingerprint();
            let receipt = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .unwrap_or_else(|_| panic!("source-produced intent must pass Stage 5C settlement"));
            let (request_ids, intent_count, history_len, canonical_history, state_fingerprint) =
                receipt.test_identity_proof_shape();
            assert_eq!(request_ids.len(), 1);
            assert_eq!(intent_count, 1);
            assert_eq!(history_len, 1);
            assert!(canonical_history);
            assert_eq!(state_fingerprint, expected_state_fingerprint);
        }

        #[test]
        fn b3f_callback_validation_error_consumes_escrow_into_terminal_receipt() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::{
                    validate_and_settle_stage5e_paper_callback_escrow,
                    Stage5ePaperSettlementTerminalReason,
                },
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut b3c = canonical_b3c_receipt(now);
            b3c.b3b
                .payload
                .accepted_semantic_bar
                .stage5e_test_force_callback_validation_error();
            let authority = issue_stage5e_callback_authority_at(b3c, now)
                .unwrap_or_else(|_| panic!("outer authority remains valid"));
            let escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("callback error must reach escrow"));
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("callback validation error must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::CallbackValidationError
            );
            assert_eq!(terminal.test_original_intent_count(), 0);
            assert_eq!(
                terminal.test_ownership_variant(),
                "callback_validation_error"
            );
        }

        #[test]
        fn b3f_intent_capacity_boundary_is_exact_at_255_and_256() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::{
                    validate_and_settle_stage5e_paper_callback_escrow,
                    Stage5ePaperSettlementTerminalReason,
                },
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc.with_ymd_and_hms(2026, 7, 24, 7, 0, 0).single().unwrap();
            let make_escrow = || {
                let authority = issue_stage5e_callback_authority_at(
                    canonical_nonempty_intent_b3c_receipt(now),
                    now,
                )
                .unwrap_or_else(|_| panic!("source-produced signal must issue authority"));
                invoke_stage5e_authorized_paper_callback_at(authority, now)
                    .unwrap_or_else(|_| panic!("source-produced callback must reach escrow"))
            };

            let mut at_limit = make_escrow();
            at_limit.test_repeat_first_ok_intent(u8::MAX as usize);
            let at_limit_terminal = validate_and_settle_stage5e_paper_callback_escrow(at_limit)
                .err()
                .unwrap_or_else(|| panic!("duplicate source request IDs must fail in Stage 5C"));
            assert_ne!(
                at_limit_terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::IntentCapacityExceeded
            );
            assert_eq!(
                at_limit_terminal.test_original_intent_count(),
                u8::MAX as usize
            );
            assert_eq!(at_limit_terminal.test_ownership_variant(), "stage5c");

            let mut over_limit = make_escrow();
            over_limit.test_repeat_first_ok_intent(u8::MAX as usize + 1);
            let over_limit_terminal = validate_and_settle_stage5e_paper_callback_escrow(over_limit)
                .err()
                .unwrap_or_else(|| panic!("256 intents must be terminal before Stage 5C"));
            assert_eq!(
                over_limit_terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::IntentCapacityExceeded
            );
            assert_eq!(
                over_limit_terminal.test_original_intent_count(),
                u8::MAX as usize + 1
            );
            assert_eq!(over_limit_terminal.test_ownership_variant(), "preflight_ok");
        }

        #[test]
        fn b3f_early_attribution_error_disposes_exact_intent_vector() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                callback_settlement::{
                    validate_and_settle_stage5e_paper_callback_escrow,
                    Stage5ePaperSettlementTerminalReason,
                },
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
            };

            let now = Utc.with_ymd_and_hms(2026, 7, 24, 7, 0, 0).single().unwrap();
            let authority = issue_stage5e_callback_authority_at(
                canonical_nonempty_intent_b3c_receipt(now),
                now,
            )
            .unwrap_or_else(|_| panic!("source-produced signal must issue authority"));
            let mut escrow = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .unwrap_or_else(|_| panic!("source-produced callback must reach escrow"));
            escrow.test_repeat_first_ok_intent(2);
            let terminal = validate_and_settle_stage5e_paper_callback_escrow(escrow)
                .err()
                .unwrap_or_else(|| panic!("duplicate request IDs must be terminal"));
            assert_eq!(
                terminal.test_reason(),
                Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
            );
            assert_eq!(terminal.test_original_intent_count(), 2);
            assert_eq!(terminal.test_ownership_variant(), "stage5c");
        }

        #[test]
        fn b3e_expiry_and_identity_mismatch_block_before_callback() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
                Stage5eCallbackInvocationTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let expired = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("authority issue must succeed"));
            let expired = invoke_stage5e_authorized_paper_callback_at(
                expired,
                now + chrono::Duration::seconds(11),
            )
            .err()
            .unwrap_or_else(|| panic!("expired callback authority must block"));
            assert_eq!(
                expired.reason(),
                Stage5eCallbackInvocationTerminalReason::AuthorityExpired
            );
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                0
            );

            let mut identity = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("authority issue must succeed"));
            identity.test_corrupt_owned_sequence_identity();
            let identity = invoke_stage5e_authorized_paper_callback_at(identity, now)
                .err()
                .unwrap_or_else(|| panic!("owned identity mismatch must block"));
            assert_eq!(
                identity.reason(),
                Stage5eCallbackInvocationTerminalReason::OwnedIdentityMismatch
            );

            let mut schedule = issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                .unwrap_or_else(|_| panic!("authority issue must succeed"));
            schedule.test_zero_nested_schedule_identity();
            let schedule = invoke_stage5e_authorized_paper_callback_at(schedule, now)
                .err()
                .unwrap_or_else(|| panic!("missing nested schedule identity must block"));
            assert_eq!(
                schedule.reason(),
                Stage5eCallbackInvocationTerminalReason::OwnedIdentityMismatch
            );

            let mut chronology =
                issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                    .unwrap_or_else(|_| panic!("authority issue must succeed"));
            chronology.test_force_nested_bound_after_issue();
            let chronology = invoke_stage5e_authorized_paper_callback_at(chronology, now)
                .err()
                .unwrap_or_else(|| panic!("B3C bound after authority issue must block"));
            assert_eq!(
                chronology.reason(),
                Stage5eCallbackInvocationTerminalReason::InvalidAuthorityChronology
            );

            let mut authority_id =
                issue_stage5e_callback_authority_at(canonical_b3c_receipt(now), now)
                    .unwrap_or_else(|_| panic!("authority issue must succeed"));
            authority_id.test_corrupt_callback_authority_id();
            let authority_id = invoke_stage5e_authorized_paper_callback_at(authority_id, now)
                .err()
                .unwrap_or_else(|| panic!("authority ID mismatch must block"));
            assert_eq!(
                authority_id.reason(),
                Stage5eCallbackInvocationTerminalReason::CallbackAuthorityIdMismatch
            );
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                0
            );
        }

        #[test]
        fn b3e_materialization_mismatch_reaches_exact_top_level_terminal_without_callback() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                invoke_stage5e_authorized_paper_callback_at, issue_stage5e_callback_authority_at,
                Stage5eCallbackInvocationTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            crate::stage5c_paper_host::stage5e_test_reset_b3e_callback_count();
            let mut b3c = canonical_b3c_receipt(now);
            b3c.b3b
                .payload
                .accepted_semantic_bar
                .stage5e_test_force_instrument_mismatch();
            let authority = issue_stage5e_callback_authority_at(b3c, now)
                .unwrap_or_else(|_| panic!("outer authority evidence remains valid"));
            let blocked = invoke_stage5e_authorized_paper_callback_at(authority, now)
                .err()
                .unwrap_or_else(|| panic!("materialization mismatch must block"));
            assert_eq!(
                blocked.reason(),
                Stage5eCallbackInvocationTerminalReason::MaterializationIntegrityMismatch
            );
            assert_eq!(
                crate::stage5c_paper_host::stage5e_test_b3e_callback_count(),
                0
            );
        }

        #[test]
        fn b3d_retryable_issue_returns_the_exact_b3c_receipt() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                issue_stage5e_callback_authority_at, Stage5eCallbackAuthorityIssueBlocked,
                Stage5eCallbackAuthorityRetryableReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let b3c = canonical_b3c_receipt(now);
            let expected_ownership = b3c.test_ownership_fingerprint();
            let blocked = match issue_stage5e_callback_authority_at(
                b3c,
                now - chrono::Duration::milliseconds(1),
            ) {
                Ok(_) => panic!("clock before effective observation must block"),
                Err(Stage5eCallbackAuthorityIssueBlocked::Retryable(blocked)) => blocked,
                Err(Stage5eCallbackAuthorityIssueBlocked::Terminal(_)) => {
                    panic!("clock-before-observation must remain retryable")
                }
            };
            assert_eq!(
                blocked.reason(),
                Stage5eCallbackAuthorityRetryableReason::ClockBeforeEffectiveObservation
            );
            let returned = blocked.into_retry_same_receipt();
            assert_eq!(returned.test_ownership_fingerprint(), expected_ownership);
            let authority = issue_stage5e_callback_authority_at(returned, now)
                .unwrap_or_else(|_| panic!("the exact returned B3C receipt must retry"));
            assert_eq!(authority.test_ownership_fingerprint(), expected_ownership);
        }

        #[test]
        fn b3d_future_bar_is_retryable_but_expiry_and_missing_identity_are_terminal() {
            use crate::stage5e_no_io_lifecycle::callback_authority::{
                issue_stage5e_callback_authority_at, Stage5eCallbackAuthorityIssueBlocked,
                Stage5eCallbackAuthorityRetryableReason, Stage5eCallbackAuthorityTerminalReason,
            };

            let now = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let mut future_bar = canonical_b3c_receipt(now);
            future_bar.effective_observed_at = now - chrono::Duration::seconds(2);
            let blocked = match issue_stage5e_callback_authority_at(
                future_bar,
                now - chrono::Duration::seconds(1),
            ) {
                Ok(_) => panic!("future accepted bar must block"),
                Err(Stage5eCallbackAuthorityIssueBlocked::Retryable(blocked)) => blocked,
                Err(Stage5eCallbackAuthorityIssueBlocked::Terminal(_)) => {
                    panic!("future accepted bar must remain retryable")
                }
            };
            assert_eq!(
                blocked.reason(),
                Stage5eCallbackAuthorityRetryableReason::AcceptedBarObservedInFuture
            );

            let expired = match issue_stage5e_callback_authority_at(
                canonical_b3c_receipt(now),
                now + chrono::Duration::seconds(11),
            ) {
                Ok(_) => panic!("expired evidence must block"),
                Err(Stage5eCallbackAuthorityIssueBlocked::Terminal(blocked)) => blocked,
                Err(Stage5eCallbackAuthorityIssueBlocked::Retryable(_)) => {
                    panic!("expired evidence must be terminal")
                }
            };
            assert_eq!(
                expired.reason(),
                Stage5eCallbackAuthorityTerminalReason::EvidenceExpired
            );

            let mut missing_identity = canonical_b3c_receipt(now);
            missing_identity.b3b.payload.accepted_semantic_bar_identity = [0; 32];
            let missing = match issue_stage5e_callback_authority_at(missing_identity, now) {
                Ok(_) => panic!("missing semantic identity must block"),
                Err(Stage5eCallbackAuthorityIssueBlocked::Terminal(blocked)) => blocked,
                Err(Stage5eCallbackAuthorityIssueBlocked::Retryable(_)) => {
                    panic!("missing semantic identity must be terminal")
                }
            };
            assert_eq!(
                missing.reason(),
                Stage5eCallbackAuthorityTerminalReason::AcceptedSemanticBarIdentityMissing
            );
        }

        #[test]
        fn fingerprint_covers_full_identity_and_mapping_rechecks_expiry() {
            let now = Utc::now();
            let validated_a = validated(now);
            let registry_a =
                accept_instrument_registry_evidence(&validated_a, registry(&validated_a)).unwrap();
            let first = map_trusted_schedule_window_internal(
                validated_a,
                registry_a,
                stage4(now, instrument()),
                MarketBarCloseTime(150),
                LifecycleInstant(now),
            )
            .unwrap();
            let mut changed = snapshot(now);
            changed.instrument.exchange = Exchange::Other("other-exchange".to_string());
            changed.normalized_payload_sha256 = normalized_snapshot_payload_fingerprint(&changed);
            let validated_b = validate_normalized_schedule_snapshot(
                NormalizedScheduleAvailability::Available(Box::new(changed)),
                LifecycleInstant(now),
            )
            .unwrap();
            let registry_b =
                accept_instrument_registry_evidence(&validated_b, registry(&validated_b)).unwrap();
            let second = map_trusted_schedule_window_internal(
                validated_b,
                registry_b,
                stage4(
                    now,
                    InstrumentId {
                        symbol: "IMOEXF".to_string(),
                        venue_symbol: Some("IMOEXF@RTSX".to_string()),
                        exchange: Exchange::Other("other-exchange".to_string()),
                        market: Market::Futures,
                    },
                ),
                MarketBarCloseTime(150),
                LifecycleInstant(now),
            )
            .unwrap();
            assert_ne!(first.fingerprint, second.fingerprint);
            let stage4_expired_validated = validated(now);
            let accepted_registry = accept_instrument_registry_evidence(
                &stage4_expired_validated,
                registry(&stage4_expired_validated),
            )
            .unwrap();
            assert!(matches!(
                map_trusted_schedule_window_internal(
                    stage4_expired_validated,
                    accepted_registry,
                    AcceptedStage4ScheduleEvidence {
                        instrument: instrument(),
                        session_state: broker_core::BrokerMarketSessionState::Open,
                        observed_at: LifecycleInstant(now),
                        expires_at: LifecycleInstant(now - chrono::Duration::seconds(1)),
                        identity: ScheduleFingerprint([8; 32]),
                    },
                    MarketBarCloseTime(150),
                    LifecycleInstant(now),
                ),
                Err(ScheduleWindowMappingError::Stage4Expired)
            ));
            let snapshot_expired_validated = validated(now);
            let accepted_registry = accept_instrument_registry_evidence(
                &snapshot_expired_validated,
                registry(&snapshot_expired_validated),
            )
            .unwrap();
            assert!(matches!(
                map_trusted_schedule_window_internal(
                    snapshot_expired_validated,
                    accepted_registry,
                    AcceptedStage4ScheduleEvidence {
                        instrument: instrument(),
                        session_state: broker_core::BrokerMarketSessionState::Open,
                        observed_at: LifecycleInstant(now),
                        expires_at: LifecycleInstant(now + chrono::Duration::seconds(20)),
                        identity: ScheduleFingerprint([9; 32]),
                    },
                    MarketBarCloseTime(150),
                    LifecycleInstant(now + chrono::Duration::seconds(11)),
                ),
                Err(ScheduleWindowMappingError::SnapshotExpired)
            ));
        }

        #[test]
        fn stage4_non_open_blocks_schedule_projection() {
            let now = Utc::now();
            for state in [
                broker_core::BrokerMarketSessionState::Closed,
                broker_core::BrokerMarketSessionState::Break,
                broker_core::BrokerMarketSessionState::Maintenance,
                broker_core::BrokerMarketSessionState::Unknown,
            ] {
                let validated = validated(now);
                let accepted_registry =
                    accept_instrument_registry_evidence(&validated, registry(&validated)).unwrap();
                let blocked = map_trusted_schedule_window_internal(
                    validated,
                    accepted_registry,
                    AcceptedStage4ScheduleEvidence {
                        instrument: instrument(),
                        session_state: state,
                        observed_at: LifecycleInstant(now),
                        expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                        identity: ScheduleFingerprint([7; 32]),
                    },
                    MarketBarCloseTime(150),
                    LifecycleInstant(now),
                );
                assert!(matches!(
                    blocked,
                    Err(ScheduleWindowMappingError::Stage4NotOpen)
                ));
            }
        }

        #[test]
        fn sealed_registry_and_stage4_time_evidence_are_fail_closed() {
            let now = Utc::now();
            let validated = validated(now);
            let mut wrong_registry = registry(&validated);
            wrong_registry.board = "OTHER".to_string();
            assert!(matches!(
                accept_instrument_registry_evidence(&validated, wrong_registry),
                Err(RegistryBindingError::IdentityMismatch)
            ));
            assert!(matches!(
                validate_stage4_projection_times(
                    now + chrono::Duration::milliseconds(1),
                    now,
                    LifecycleInstant(now),
                ),
                Err(Stage4ScheduleProjectionError::ReportCheckedInFuture)
            ));
            assert!(matches!(
                validate_stage4_projection_times(
                    now,
                    now + chrono::Duration::milliseconds(1),
                    LifecycleInstant(now),
                ),
                Err(Stage4ScheduleProjectionError::ObservedInFuture)
            ));
        }

        #[test]
        fn observed_bar_binding_is_exact_inclusive_and_revalidates_time() {
            let now = Utc::now();
            let validated = validated(now);
            let accepted_registry =
                accept_instrument_registry_evidence(&validated, registry(&validated)).unwrap();
            let window = map_trusted_schedule_window_internal(
                validated,
                accepted_registry,
                stage4(now, instrument()),
                MarketBarCloseTime(100),
                LifecycleInstant(now),
            )
            .unwrap();
            assert!(validate_schedule_window_for_observed_bar(
                &window,
                &instrument(),
                MarketBarCloseTime(100),
                LifecycleInstant(now),
            )
            .is_ok());
            assert!(validate_schedule_window_for_observed_bar(
                &window,
                &instrument(),
                MarketBarCloseTime(200),
                LifecycleInstant(now),
            )
            .is_ok());
            assert!(matches!(
                validate_schedule_window_for_observed_bar(
                    &window,
                    &InstrumentId {
                        symbol: "OTHER".to_string(),
                        venue_symbol: Some("OTHER@RTSX".to_string()),
                        exchange: Exchange::Moex,
                        market: Market::Futures,
                    },
                    MarketBarCloseTime(150),
                    LifecycleInstant(now),
                ),
                Err(ScheduleWindowObservedBarBindingError::InstrumentMismatch)
            ));
            assert!(matches!(
                validate_schedule_window_for_observed_bar(
                    &window,
                    &instrument(),
                    MarketBarCloseTime(201),
                    LifecycleInstant(now),
                ),
                Err(ScheduleWindowObservedBarBindingError::BarOutsideInclusiveWindow)
            ));
            assert!(matches!(
                validate_schedule_window_for_observed_bar(
                    &window,
                    &instrument(),
                    MarketBarCloseTime(now.timestamp() + 1),
                    LifecycleInstant(now),
                ),
                Err(ScheduleWindowObservedBarBindingError::ObservedBarInFuture)
            ));
            assert!(matches!(
                validate_schedule_window_for_observed_bar(
                    &window,
                    &instrument(),
                    MarketBarCloseTime(150),
                    LifecycleInstant(now + chrono::Duration::seconds(11)),
                ),
                Err(ScheduleWindowObservedBarBindingError::WindowExpired)
            ));
            assert!(matches!(
                validate_schedule_window_for_observed_bar(
                    &window,
                    &instrument(),
                    MarketBarCloseTime(150),
                    LifecycleInstant(now - chrono::Duration::milliseconds(1)),
                ),
                Err(ScheduleWindowObservedBarBindingError::ClockBeforeEffectiveEvidenceObservation)
            ));
        }

        #[test]
        fn consuming_binding_owns_receipts_and_preserves_zero_side_effect_proof() {
            let now = Utc::now();
            let close = now.timestamp();
            let window = window_for_bar(now, instrument(), close, close + 1);
            let expected_schedule_fingerprint = window.fingerprint;
            let observed = observed_live_bar(now, close);
            let expected_ownership = observed.ownership_fingerprint();
            let bound = match bind_schedule_window_to_observed_live_bar_at(
                window,
                observed,
                LifecycleInstant(now),
            ) {
                Ok(bound) => bound,
                Err(_) => panic!("matching canonical receipts must bind"),
            };
            assert_eq!(bound.bar_instrument, instrument());
            assert_eq!(bound.bar_close_ts, MarketBarCloseTime(close));
            assert_eq!(bound.schedule_fingerprint, expected_schedule_fingerprint);
            assert_eq!(
                bound.binding_fingerprint,
                schedule_observed_bar_binding_fingerprint(
                    &bound.schedule_window,
                    &bound.bar_instrument,
                    bound.bar_close_ts,
                )
            );
            assert_eq!(bound.callback_count(), 0);
            assert_eq!(bound.intent_count(), 0);
            assert!(!bound.strategy_was_called());
            assert!(!bound.executable_intent_created());
            assert_eq!(bound.observed_live_bar.bar_close_ts(), close);
            assert_eq!(
                bound.observed_live_bar.ownership_fingerprint(),
                expected_ownership
            );
        }

        #[test]
        fn consuming_binding_returns_blocked_inputs_for_fresh_evidence_retry() {
            let now = Utc::now();
            let close = now.timestamp();
            let other = InstrumentId {
                symbol: "OTHER".to_string(),
                venue_symbol: Some("OTHER@RTSX".to_string()),
                exchange: Exchange::Moex,
                market: Market::Futures,
            };
            let observed = observed_live_bar(now, close);
            let expected_ownership = observed.ownership_fingerprint();
            let blocked = match bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, other, close, close + 1),
                observed,
                LifecycleInstant(now),
            ) {
                Ok(_) => panic!("wrong instrument must block"),
                Err(blocked) => blocked,
            };
            assert_eq!(
                blocked.reason(),
                ScheduleWindowObservedBarBindingError::InstrumentMismatch
            );
            let (_rejected_window, observed) = blocked.into_inputs();
            assert_eq!(observed.ownership_fingerprint(), expected_ownership);
            let bound = match bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, instrument(), close, close + 1),
                observed,
                LifecycleInstant(now),
            ) {
                Ok(bound) => bound,
                Err(_) => panic!("fresh matching evidence must retry successfully"),
            };
            assert_eq!(bound.callback_count(), 0);
            assert_eq!(bound.intent_count(), 0);
            assert_eq!(
                bound.observed_live_bar.ownership_fingerprint(),
                expected_ownership
            );

            let later = now + chrono::Duration::seconds(11);
            let expired = match bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, instrument(), close, close + 1),
                observed_live_bar(later, close),
                LifecycleInstant(later),
            ) {
                Ok(_) => panic!("expired window must block"),
                Err(blocked) => blocked,
            };
            assert_eq!(
                expired.reason(),
                ScheduleWindowObservedBarBindingError::WindowExpired
            );
            let (_expired_window, observed) = expired.into_inputs();
            assert!(bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(later, instrument(), close, close + 1),
                observed,
                LifecycleInstant(later),
            )
            .is_ok());
        }

        #[test]
        fn consuming_binding_blocks_future_and_preserves_inclusive_endpoints() {
            let now = Utc::now();
            let lower = now.timestamp();
            let upper = lower + 1;
            assert!(bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, instrument(), lower, upper),
                observed_live_bar(now, lower),
                LifecycleInstant(now),
            )
            .is_ok());
            let upper_now = now + chrono::Duration::seconds(1);
            assert!(bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, instrument(), lower, upper),
                observed_live_bar(upper_now, upper),
                LifecycleInstant(upper_now),
            )
            .is_ok());

            let future = now + chrono::Duration::seconds(1);
            let blocked = match bind_schedule_window_to_observed_live_bar_at(
                window_for_bar(now, instrument(), lower, future.timestamp()),
                observed_live_bar(future, future.timestamp()),
                LifecycleInstant(now),
            ) {
                Ok(_) => panic!("future observed bar must block"),
                Err(blocked) => blocked,
            };
            assert_eq!(
                blocked.reason(),
                ScheduleWindowObservedBarBindingError::ObservedBarInFuture
            );
        }
    }
    // STAGE5E-B3C-PRODUCTION-BRIDGE-END: trusted-no-io-v1

    // STAGE5E-B3C-EVIDENCE-BEGIN: private-no-io-v1
    // These receipts deliberately stay module-private.  They are evidence
    // producers only; a later reviewed bridge is required to consume b3b.
    mod legacy_b3c_evidence {
        use super::{
            AcceptedStage4ScheduleEvidence, NormalizedSessionType,
            ValidatedNormalizedInstrumentScheduleSnapshot,
        };
        use super::{DateTime, Stage5eBoundScheduleWindowForObservedLiveBar, Utc};

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum EvidenceError {
            Unavailable,
            Unknown,
            NotOpen,
            NonTrading,
            ObservedInFuture,
            Expired,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Stage5eEligibilityBlockReason {
            ClockBeforeEvidenceObservation,
            B3bScheduleExpired,
            B3bObservedBarInFuture,
            SessionExpired,
            CalendarExpired,
            SequenceExpired,
            SessionNotOpen,
            CalendarNonTrading,
            SequenceUnavailable,
            SequenceGap,
            InstrumentMismatch,
            CalendarTradingDayMismatch,
            ScheduleFingerprintMismatch,
            BarIdentityMismatch,
            ContinuationEpochMismatch,
        }

        struct Stage5eFreshOpenSessionEvidence {
            source: AcceptedStage4ScheduleEvidence,
            instrument: broker_core::InstrumentId,
            venue_mic: String,
            trading_day: chrono::NaiveDate,
            schedule_fingerprint: [u8; 32],
            event_key_fingerprint: [u8; 32],
            continuation_epoch: String,
            source_fingerprint: [u8; 32],
            observed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
        }
        struct Stage5eCalendarEligibilityEvidence {
            source: ValidatedNormalizedInstrumentScheduleSnapshot,
            instrument: broker_core::InstrumentId,
            venue_mic: String,
            trading_day: chrono::NaiveDate,
            schedule_fingerprint: [u8; 32],
            event_key_fingerprint: [u8; 32],
            version: String,
            continuation_epoch: String,
            fingerprint: [u8; 32],
            early_close_policy: String,
            observed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
        }
        struct UnverifiedMarketSequenceSource {
            instrument: broker_core::InstrumentId,
            venue_mic: String,
            trading_day: chrono::NaiveDate,
            schedule_fingerprint: [u8; 32],
            event_key_fingerprint: [u8; 32],
            timeframe_sec: u32,
            finality: bool,
            provenance: String,
            continuation_epoch: String,
            source_fingerprint: [u8; 32],
            previous_canonical_close: i64,
            gap_free: bool,
            observed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
        }
        struct Stage5eMarketSequenceEvidence {
            source: UnverifiedMarketSequenceSource,
            instrument: broker_core::InstrumentId,
            venue_mic: String,
            trading_day: chrono::NaiveDate,
            schedule_fingerprint: [u8; 32],
            event_key_fingerprint: [u8; 32],
            timeframe_sec: u32,
            finality: bool,
            provenance: String,
            continuation_epoch: String,
            source_fingerprint: [u8; 32],
            previous_canonical_close: i64,
            observed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
        }

        /// Producer failures retain their unique source.  A later operator or
        /// source-refresh step can therefore retry without reconstructing a
        /// supposedly accepted snapshot from detached fields.
        struct Stage5eOpenSessionEvidenceBlocked {
            reason: EvidenceError,
            source: AcceptedStage4ScheduleEvidence,
        }
        struct Stage5eCalendarEvidenceBlocked {
            reason: EvidenceError,
            source: ValidatedNormalizedInstrumentScheduleSnapshot,
        }
        struct Stage5eMarketSequenceEvidenceBlocked {
            reason: EvidenceError,
            source: UnverifiedMarketSequenceSource,
        }

        /// The combined receipt owns the b3b receipt and all newly accepted
        /// evidence.  It intentionally exposes no continuation, callback or
        /// executable-intent API.
        struct Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            b3b: Stage5eBoundScheduleWindowForObservedLiveBar,
            session: Stage5eFreshOpenSessionEvidence,
            calendar: Stage5eCalendarEligibilityEvidence,
            sequence: Stage5eMarketSequenceEvidence,
        }

        /// A blocked transition is recoverable only by returning every linear
        /// input unchanged.  This keeps retry ownership explicit and prevents a
        /// partial continuation from silently losing strategy/recovery state.
        struct Stage5eSessionCalendarSequenceBlocked {
            reason: Stage5eEligibilityBlockReason,
            b3b: Stage5eBoundScheduleWindowForObservedLiveBar,
            session: Stage5eFreshOpenSessionEvidence,
            calendar: Stage5eCalendarEligibilityEvidence,
            sequence: Stage5eMarketSequenceEvidence,
        }

        fn fresh(
            observed_at: DateTime<Utc>,
            expires_at: DateTime<Utc>,
            now: DateTime<Utc>,
        ) -> Result<(), EvidenceError> {
            if observed_at > now {
                return Err(EvidenceError::ObservedInFuture);
            }
            if now > expires_at {
                return Err(EvidenceError::Expired);
            }
            Ok(())
        }

        fn has_canonical_source_binding(
            instrument: &broker_core::InstrumentId,
            venue_mic: &str,
            schedule_fingerprint: [u8; 32],
            event_key_fingerprint: [u8; 32],
        ) -> bool {
            let Some(venue_symbol) = instrument.venue_symbol.as_deref() else {
                return false;
            };
            let Some((ticker, mic)) = super::split_canonical_broker_symbol(venue_symbol) else {
                return false;
            };
            ticker == instrument.symbol
                && mic == venue_mic
                && schedule_fingerprint != [0; 32]
                && event_key_fingerprint != [0; 32]
        }

        fn accept_open_session(
            source: AcceptedStage4ScheduleEvidence,
            b3b: &Stage5eBoundScheduleWindowForObservedLiveBar,
            now: DateTime<Utc>,
        ) -> Result<Stage5eFreshOpenSessionEvidence, Box<Stage5eOpenSessionEvidenceBlocked>>
        {
            if let Err(reason) = fresh(source.observed_at.0, source.expires_at.0, now) {
                return Err(Box::new(Stage5eOpenSessionEvidenceBlocked {
                    reason,
                    source,
                }));
            }
            let schedule = &b3b.schedule_window;
            if schedule.selected_session_type != NormalizedSessionType::TradableOpen {
                return Err(Box::new(Stage5eOpenSessionEvidenceBlocked {
                    reason: EvidenceError::NotOpen,
                    source,
                }));
            }
            if source.instrument != b3b.bar_instrument || source.identity.0 == [0; 32] {
                return Err(Box::new(Stage5eOpenSessionEvidenceBlocked {
                    reason: EvidenceError::Unavailable,
                    source,
                }));
            }
            Ok(Stage5eFreshOpenSessionEvidence {
                instrument: b3b.bar_instrument.clone(),
                venue_mic: schedule.venue_mic.clone(),
                trading_day: schedule.trading_day.0,
                schedule_fingerprint: b3b.schedule_fingerprint.0,
                event_key_fingerprint: b3b.binding_fingerprint.0,
                continuation_epoch: "epoch-1".to_owned(),
                source_fingerprint: source.identity.0,
                observed_at: source.observed_at.0,
                expires_at: source.expires_at.0,
                source,
            })
        }
        fn accept_calendar(
            source: ValidatedNormalizedInstrumentScheduleSnapshot,
            b3b: &Stage5eBoundScheduleWindowForObservedLiveBar,
            now: DateTime<Utc>,
        ) -> Result<Stage5eCalendarEligibilityEvidence, Box<Stage5eCalendarEvidenceBlocked>>
        {
            if let Err(reason) = fresh(
                source.snapshot.source_observed_at.0,
                source.snapshot.source_expires_at.0,
                now,
            ) {
                return Err(Box::new(Stage5eCalendarEvidenceBlocked { reason, source }));
            }
            let schedule = &b3b.schedule_window;
            if !source.snapshot.sessions.iter().any(|candidate| {
                candidate.session_type == NormalizedSessionType::TradableOpen
                    && candidate.start == schedule.open_from
                    && candidate.end == schedule.open_until
            }) {
                return Err(Box::new(Stage5eCalendarEvidenceBlocked {
                    reason: EvidenceError::NonTrading,
                    source,
                }));
            }
            if source.snapshot.instrument != b3b.bar_instrument
                || source.snapshot.venue_mic != schedule.venue_mic
                || source.snapshot.trading_day.0 != schedule.trading_day.0
                || source.identity_fingerprint == [0; 32]
            {
                return Err(Box::new(Stage5eCalendarEvidenceBlocked {
                    reason: EvidenceError::Unavailable,
                    source,
                }));
            }
            Ok(Stage5eCalendarEligibilityEvidence {
                instrument: b3b.bar_instrument.clone(),
                venue_mic: schedule.venue_mic.clone(),
                trading_day: schedule.trading_day.0,
                schedule_fingerprint: b3b.schedule_fingerprint.0,
                event_key_fingerprint: b3b.binding_fingerprint.0,
                version: source.snapshot.source_contract_version.clone(),
                continuation_epoch: "epoch-1".to_owned(),
                fingerprint: source.identity_fingerprint,
                early_close_policy: "broker-normalized-schedule".to_owned(),
                observed_at: source.snapshot.source_observed_at.0,
                expires_at: source.snapshot.source_expires_at.0,
                source,
            })
        }
        fn accept_sequence(
            source: UnverifiedMarketSequenceSource,
            _b3b: &Stage5eBoundScheduleWindowForObservedLiveBar,
            now: DateTime<Utc>,
        ) -> Result<Stage5eMarketSequenceEvidence, Box<Stage5eMarketSequenceEvidenceBlocked>>
        {
            if let Err(reason) = fresh(source.observed_at, source.expires_at, now) {
                return Err(Box::new(Stage5eMarketSequenceEvidenceBlocked {
                    reason,
                    source,
                }));
            }
            if !source.finality
                || !source.gap_free
                || source.timeframe_sec == 0
                || source.provenance.is_empty()
                || source.continuation_epoch.is_empty()
                || source.source_fingerprint == [0; 32]
            {
                return Err(Box::new(Stage5eMarketSequenceEvidenceBlocked {
                    reason: EvidenceError::Unknown,
                    source,
                }));
            }
            Ok(Stage5eMarketSequenceEvidence {
                instrument: source.instrument.clone(),
                venue_mic: source.venue_mic.clone(),
                trading_day: source.trading_day,
                schedule_fingerprint: source.schedule_fingerprint,
                event_key_fingerprint: source.event_key_fingerprint,
                timeframe_sec: source.timeframe_sec,
                finality: source.finality,
                provenance: source.provenance.clone(),
                continuation_epoch: source.continuation_epoch.clone(),
                source_fingerprint: source.source_fingerprint,
                previous_canonical_close: source.previous_canonical_close,
                observed_at: source.observed_at,
                expires_at: source.expires_at,
                source,
            })
        }

        fn validate_continuation(
            b3b: &Stage5eBoundScheduleWindowForObservedLiveBar,
            session: &Stage5eFreshOpenSessionEvidence,
            calendar: &Stage5eCalendarEligibilityEvidence,
            sequence: &Stage5eMarketSequenceEvidence,
            continuation_time: DateTime<Utc>,
        ) -> Result<(), Stage5eEligibilityBlockReason> {
            let schedule = &b3b.schedule_window;
            if continuation_time < schedule.effective_observed_at.0
                || continuation_time < session.observed_at
                || continuation_time < calendar.observed_at
                || continuation_time < sequence.observed_at
            {
                return Err(Stage5eEligibilityBlockReason::ClockBeforeEvidenceObservation);
            }
            if continuation_time > schedule.expires_at.0 {
                return Err(Stage5eEligibilityBlockReason::B3bScheduleExpired);
            }
            if b3b.bar_close_ts.0 > continuation_time.timestamp() {
                return Err(Stage5eEligibilityBlockReason::B3bObservedBarInFuture);
            }
            if continuation_time > session.expires_at {
                return Err(Stage5eEligibilityBlockReason::SessionExpired);
            }
            if continuation_time > calendar.expires_at {
                return Err(Stage5eEligibilityBlockReason::CalendarExpired);
            }
            if continuation_time > sequence.expires_at {
                return Err(Stage5eEligibilityBlockReason::SequenceExpired);
            }
            if session.source.instrument != b3b.bar_instrument {
                return Err(Stage5eEligibilityBlockReason::SessionNotOpen);
            }
            if calendar.source.snapshot.sessions.is_empty() {
                return Err(Stage5eEligibilityBlockReason::CalendarNonTrading);
            }
            if !sequence.finality || sequence.source_fingerprint == [0; 32] {
                return Err(Stage5eEligibilityBlockReason::SequenceUnavailable);
            }
            if session.instrument != b3b.bar_instrument
                || calendar.instrument != b3b.bar_instrument
                || sequence.instrument != b3b.bar_instrument
                || session.venue_mic != schedule.venue_mic
                || calendar.venue_mic != schedule.venue_mic
                || sequence.venue_mic != schedule.venue_mic
            {
                return Err(Stage5eEligibilityBlockReason::InstrumentMismatch);
            }
            if session.trading_day != schedule.trading_day.0
                || calendar.trading_day != schedule.trading_day.0
                || sequence.trading_day != schedule.trading_day.0
            {
                return Err(Stage5eEligibilityBlockReason::CalendarTradingDayMismatch);
            }
            if session.schedule_fingerprint != b3b.schedule_fingerprint.0
                || calendar.schedule_fingerprint != b3b.schedule_fingerprint.0
                || sequence.schedule_fingerprint != b3b.schedule_fingerprint.0
            {
                return Err(Stage5eEligibilityBlockReason::ScheduleFingerprintMismatch);
            }
            if session.event_key_fingerprint != b3b.binding_fingerprint.0
                || calendar.event_key_fingerprint != b3b.binding_fingerprint.0
                || sequence.event_key_fingerprint != b3b.binding_fingerprint.0
            {
                return Err(Stage5eEligibilityBlockReason::BarIdentityMismatch);
            }
            if session.continuation_epoch != calendar.continuation_epoch
                || session.continuation_epoch != sequence.continuation_epoch
            {
                return Err(Stage5eEligibilityBlockReason::ContinuationEpochMismatch);
            }
            Ok(())
        }

        fn bind_session_calendar_sequence(
            b3b: Stage5eBoundScheduleWindowForObservedLiveBar,
            session: Stage5eFreshOpenSessionEvidence,
            calendar: Stage5eCalendarEligibilityEvidence,
            sequence: Stage5eMarketSequenceEvidence,
        ) -> Result<
            Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            Box<Stage5eSessionCalendarSequenceBlocked>,
        > {
            bind_session_calendar_sequence_at(b3b, session, calendar, sequence, Utc::now())
        }

        fn bind_session_calendar_sequence_at(
            b3b: Stage5eBoundScheduleWindowForObservedLiveBar,
            session: Stage5eFreshOpenSessionEvidence,
            calendar: Stage5eCalendarEligibilityEvidence,
            sequence: Stage5eMarketSequenceEvidence,
            continuation_time: DateTime<Utc>,
        ) -> Result<
            Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            Box<Stage5eSessionCalendarSequenceBlocked>,
        > {
            if let Err(reason) =
                validate_continuation(&b3b, &session, &calendar, &sequence, continuation_time)
            {
                return Err(Box::new(Stage5eSessionCalendarSequenceBlocked {
                    reason,
                    b3b,
                    session,
                    calendar,
                    sequence,
                }));
            }
            Ok(Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
                b3b,
                session,
                calendar,
                sequence,
            })
        }

        impl Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            fn callback_count(&self) -> usize {
                self.b3b.callback_count()
            }

            fn intent_count(&self) -> usize {
                self.b3b.intent_count()
            }

            fn strategy_was_called(&self) -> bool {
                self.b3b.strategy_was_called()
            }

            fn executable_intent_created(&self) -> bool {
                self.b3b.executable_intent_created()
            }
        }

        impl Stage5eSessionCalendarSequenceBlocked {
            fn reason(&self) -> Stage5eEligibilityBlockReason {
                self.reason
            }

            fn into_inputs(
                self,
            ) -> (
                Stage5eBoundScheduleWindowForObservedLiveBar,
                Stage5eFreshOpenSessionEvidence,
                Stage5eCalendarEligibilityEvidence,
                Stage5eMarketSequenceEvidence,
            ) {
                (self.b3b, self.session, self.calendar, self.sequence)
            }
        }

        impl Stage5eOpenSessionEvidenceBlocked {
            fn reason(&self) -> EvidenceError {
                self.reason
            }

            fn into_source(self) -> AcceptedStage4ScheduleEvidence {
                self.source
            }
        }

        impl std::fmt::Debug for Stage5eOpenSessionEvidenceBlocked {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct("Stage5eOpenSessionEvidenceBlocked")
                    .field("reason", &self.reason)
                    .finish_non_exhaustive()
            }
        }

        impl Stage5eCalendarEvidenceBlocked {
            fn reason(&self) -> EvidenceError {
                self.reason
            }

            fn into_source(self) -> ValidatedNormalizedInstrumentScheduleSnapshot {
                self.source
            }
        }

        impl std::fmt::Debug for Stage5eCalendarEvidenceBlocked {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct("Stage5eCalendarEvidenceBlocked")
                    .field("reason", &self.reason)
                    .finish_non_exhaustive()
            }
        }

        impl Stage5eMarketSequenceEvidenceBlocked {
            fn reason(&self) -> EvidenceError {
                self.reason
            }

            fn into_source(self) -> UnverifiedMarketSequenceSource {
                self.source
            }
        }

        impl std::fmt::Debug for Stage5eMarketSequenceEvidenceBlocked {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct("Stage5eMarketSequenceEvidenceBlocked")
                    .field("reason", &self.reason)
                    .finish_non_exhaustive()
            }
        }

        #[cfg(test)]
        mod tests {
            use super::super::{
                normalized_snapshot_payload_fingerprint, validate_normalized_schedule_snapshot,
                LifecycleInstant, MarketBarCloseTime, NormalizedInstrumentScheduleSnapshot,
                NormalizedScheduleAvailability, NormalizedScheduleSession, NormalizedSessionType,
                ScheduleFingerprint, ScheduleObservedBarBindingFingerprint, ScheduleSourceIdentity,
                Stage5eScheduleWindowEvidence, TradingDay,
            };
            use super::*;
            use chrono::TimeZone;

            fn instrument() -> broker_core::InstrumentId {
                broker_core::InstrumentId {
                    symbol: "IMOEXF".to_owned(),
                    venue_symbol: Some("IMOEXF@RTSX".to_owned()),
                    exchange: broker_core::Exchange::Moex,
                    market: broker_core::Market::Futures,
                }
            }

            fn now() -> DateTime<Utc> {
                Utc.timestamp_opt(1_800, 0).single().unwrap()
            }

            fn trading_day() -> chrono::NaiveDate {
                chrono::NaiveDate::from_ymd_opt(2026, 7, 25).unwrap()
            }

            fn session_source(now: DateTime<Utc>) -> AcceptedStage4ScheduleEvidence {
                AcceptedStage4ScheduleEvidence {
                    instrument: instrument(),
                    session_state: broker_core::BrokerMarketSessionState::Open,
                    observed_at: LifecycleInstant(now),
                    expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                    identity: ScheduleFingerprint([1; 32]),
                }
            }

            fn calendar_source(
                now: DateTime<Utc>,
            ) -> ValidatedNormalizedInstrumentScheduleSnapshot {
                let mut snapshot = NormalizedInstrumentScheduleSnapshot {
                    instrument: instrument(),
                    broker_symbol: "IMOEXF@RTSX".to_owned(),
                    venue_mic: "RTSX".to_owned(),
                    trading_day: TradingDay(trading_day()),
                    board: "RTSX".to_owned(),
                    sessions: vec![NormalizedScheduleSession {
                        session_type: NormalizedSessionType::TradableOpen,
                        start: MarketBarCloseTime(1_700),
                        end: MarketBarCloseTime(1_900),
                    }],
                    source: ScheduleSourceIdentity::BrokerReported,
                    source_contract_version: "fixture-v1".to_owned(),
                    source_observed_at: LifecycleInstant(now),
                    source_expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                    raw_response_sha256: [2; 32],
                    normalized_payload_sha256: [0; 32],
                    instrument_registry_version: "fixture-registry-v1".to_owned(),
                };
                snapshot.normalized_payload_sha256 =
                    normalized_snapshot_payload_fingerprint(&snapshot);
                validate_normalized_schedule_snapshot(
                    NormalizedScheduleAvailability::Available(Box::new(snapshot)),
                    LifecycleInstant(now),
                )
                .expect("canonical broker schedule fixture must validate")
            }

            fn sequence_source(now: DateTime<Utc>) -> UnverifiedMarketSequenceSource {
                UnverifiedMarketSequenceSource {
                    instrument: instrument(),
                    venue_mic: "RTSX".to_owned(),
                    trading_day: trading_day(),
                    schedule_fingerprint: [7; 32],
                    event_key_fingerprint: [8; 32],
                    timeframe_sec: 60,
                    finality: true,
                    provenance: "unverified-test-source".to_owned(),
                    continuation_epoch: "epoch-1".to_owned(),
                    source_fingerprint: [3; 32],
                    previous_canonical_close: 1_740,
                    gap_free: true,
                    observed_at: now,
                    expires_at: now + chrono::Duration::seconds(10),
                }
            }

            fn accepted_inputs(
                now: DateTime<Utc>,
                b3b: &Stage5eBoundScheduleWindowForObservedLiveBar,
            ) -> (
                Stage5eFreshOpenSessionEvidence,
                Stage5eCalendarEligibilityEvidence,
                Stage5eMarketSequenceEvidence,
            ) {
                (
                    accept_open_session(session_source(now), b3b, now).unwrap(),
                    accept_calendar(calendar_source(now), b3b, now).unwrap(),
                    accept_sequence(sequence_source(now), b3b, now).unwrap(),
                )
            }

            fn bound_b3b(now: DateTime<Utc>) -> Stage5eBoundScheduleWindowForObservedLiveBar {
                let instrument = instrument();
                let schedule_fingerprint = ScheduleFingerprint([7; 32]);
                Stage5eBoundScheduleWindowForObservedLiveBar {
                    schedule_window: Stage5eScheduleWindowEvidence {
                        instrument: instrument.clone(),
                        broker_symbol: "IMOEXF@RTSX".to_owned(),
                        venue_mic: "RTSX".to_owned(),
                        board: "RTSX".to_owned(),
                        trading_day: TradingDay(trading_day()),
                        source_contract_version: "test-v1".to_owned(),
                        selected_session_type: NormalizedSessionType::TradableOpen,
                        open_from: MarketBarCloseTime(1_700),
                        open_until: MarketBarCloseTime(1_900),
                        normalized_observed_at: LifecycleInstant(
                            now - chrono::Duration::seconds(1),
                        ),
                        stage4_observed_at: LifecycleInstant(now - chrono::Duration::seconds(1)),
                        effective_observed_at: LifecycleInstant(now - chrono::Duration::seconds(1)),
                        expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                        fingerprint: schedule_fingerprint,
                        normalized_sessions: vec![NormalizedScheduleSession {
                            session_type: NormalizedSessionType::TradableOpen,
                            start: MarketBarCloseTime(1_700),
                            end: MarketBarCloseTime(1_900),
                        }],
                        normalized_sessions_fingerprint: [9; 32],
                        normalized_snapshot_identity_fingerprint: [10; 32],
                        stage4_dynamic_session_fingerprint: [11; 32],
                    },
                    observed_live_bar:
                        crate::stage5c_paper_host::stage5e_test_observed_live_bar_after_history_at(
                            now,
                            now.timestamp(),
                        ),
                    bar_instrument: instrument,
                    bar_close_ts: MarketBarCloseTime(now.timestamp()),
                    schedule_fingerprint,
                    binding_fingerprint: ScheduleObservedBarBindingFingerprint([8; 32]),
                }
            }

            #[test]
            fn accepts_only_fresh_open_trading_and_gap_free_evidence() {
                let now = now();
                let b3b = bound_b3b(now);
                assert!(accept_open_session(session_source(now), &b3b, now).is_ok());
                assert!(accept_calendar(calendar_source(now), &b3b, now).is_ok());
                assert!(accept_sequence(sequence_source(now), &b3b, now).is_ok());
            }

            #[test]
            fn blocks_non_open_non_trading_and_stale_evidence() {
                let now = now();
                let mut closed_b3b = bound_b3b(now);
                closed_b3b.schedule_window.selected_session_type =
                    NormalizedSessionType::BreakOrClearing;
                assert!(matches!(
                    accept_open_session(session_source(now), &closed_b3b, now),
                    Err(blocked) if blocked.reason() == EvidenceError::NotOpen
                ));
                let mut calendar = calendar_source(now);
                calendar.snapshot.sessions[0].session_type = NormalizedSessionType::Maintenance;
                assert!(matches!(
                    accept_calendar(calendar, &bound_b3b(now), now),
                    Err(blocked) if blocked.reason() == EvidenceError::NonTrading
                ));
                assert!(matches!(
                    accept_sequence(
                        sequence_source(now),
                        &bound_b3b(now),
                        now + chrono::Duration::seconds(601),
                    ),
                    Err(blocked) if blocked.reason() == EvidenceError::Expired
                ));
            }

            #[test]
            fn consumes_all_four_receipts_only_after_conjunctive_revalidation() {
                let now = now();
                let b3b = bound_b3b(now);
                let (session, calendar, sequence) = accepted_inputs(now, &b3b);
                let combined = match bind_session_calendar_sequence_at(
                    b3b, session, calendar, sequence, now,
                ) {
                    Ok(combined) => combined,
                    Err(_) => panic!("conjunctively valid evidence must bind"),
                };
                assert_eq!(combined.callback_count(), 0);
                assert_eq!(combined.intent_count(), 0);
                assert!(!combined.strategy_was_called());
                assert!(!combined.executable_intent_created());
                assert_eq!(combined.session.continuation_epoch, "epoch-1");
                assert_eq!(combined.calendar.continuation_epoch, "epoch-1");
                assert_eq!(combined.sequence.continuation_epoch, "epoch-1");
            }

            #[test]
            fn returns_every_input_unchanged_on_binding_or_freshness_block() {
                let now = now();
                let b3b = bound_b3b(now);
                let mut sequence_source = sequence_source(now);
                sequence_source.event_key_fingerprint = [9; 32];
                let blocked = match bind_session_calendar_sequence_at(
                    b3b,
                    accept_open_session(session_source(now), &bound_b3b(now), now).unwrap(),
                    accept_calendar(calendar_source(now), &bound_b3b(now), now).unwrap(),
                    accept_sequence(sequence_source, &bound_b3b(now), now).unwrap(),
                    now,
                ) {
                    Ok(_) => panic!("mismatched event-key must block"),
                    Err(blocked) => blocked,
                };
                assert_eq!(
                    blocked.reason(),
                    Stage5eEligibilityBlockReason::BarIdentityMismatch
                );
                let (b3b, session, calendar, sequence) = blocked.into_inputs();
                assert_eq!(b3b.callback_count(), 0);
                assert_eq!(session.event_key_fingerprint, [8; 32]);
                assert_eq!(calendar.event_key_fingerprint, [8; 32]);
                assert_eq!(sequence.event_key_fingerprint, [9; 32]);

                let b3b = bound_b3b(now);
                let (session, calendar, sequence) = accepted_inputs(now, &b3b);
                let stale = match bind_session_calendar_sequence_at(
                    b3b,
                    session,
                    calendar,
                    sequence,
                    now + chrono::Duration::seconds(11),
                ) {
                    Ok(_) => panic!("expired evidence must block"),
                    Err(blocked) => blocked,
                };
                assert_eq!(
                    stale.reason(),
                    Stage5eEligibilityBlockReason::B3bScheduleExpired
                );
                let (_b3b, session, _calendar, _sequence) = stale.into_inputs();
                assert_eq!(session.continuation_epoch, "epoch-1");
            }
        }
    }
    // STAGE5E-B3C-EVIDENCE-END: private-no-io-v1
}
// STAGE5E-B3-SCHEDULE-WINDOW-END: sealed-contract-v5

// STAGE5E-B3D-CALLBACK-AUTHORITY-BEGIN: private-no-io-issue-v1
#[allow(dead_code)]
pub(crate) mod callback_authority {
    use super::{DateTime, Digest, Sha256, Utc};
    use crate::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence::Stage5eBoundSessionCalendarSequenceForObservedLiveBar;

    const AUTHORITY_DOMAIN: &[u8] = b"stage5e-callback-authority-v1";

    pub(crate) struct Stage5eCallbackAuthorityId([u8; 32]);

    pub(crate) struct Stage5eCallbackAuthorityIssueSeal(());

    pub(crate) struct Stage5eCallbackAuthorityPreflight<'a> {
        _b3c_receipt: &'a Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
        full_instrument_id: &'a broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        event_key_fingerprint: [u8; 32],
        continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
        schedule_identity_fingerprint: [u8; 32],
        accepted_bar_close_ts: i64,
        effective_observed_at: DateTime<Utc>,
        effective_expires_at: DateTime<Utc>,
    }

    impl<'a> Stage5eCallbackAuthorityPreflight<'a> {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn from_b3c_receipt(
            _seal: Stage5eCallbackAuthorityIssueSeal,
            b3c_receipt: &'a Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            full_instrument_id: &'a broker_core::InstrumentId,
            accepted_semantic_bar_identity: [u8; 32],
            event_key_fingerprint: [u8; 32],
            continuation_binding_id: [u8; 32],
            sequence_identity_fingerprint: [u8; 32],
            schedule_identity_fingerprint: [u8; 32],
            accepted_bar_close_ts: i64,
            effective_observed_at: DateTime<Utc>,
            effective_expires_at: DateTime<Utc>,
        ) -> Self {
            Self {
                _b3c_receipt: b3c_receipt,
                full_instrument_id,
                accepted_semantic_bar_identity,
                event_key_fingerprint,
                continuation_binding_id,
                sequence_identity_fingerprint,
                schedule_identity_fingerprint,
                accepted_bar_close_ts,
                effective_observed_at,
                effective_expires_at,
            }
        }
    }

    pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {
        b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
        callback_authority_id: Stage5eCallbackAuthorityId,
        issued_at: DateTime<Utc>,
        effective_observed_at: DateTime<Utc>,
        authority_expires_at: DateTime<Utc>,
        accepted_bar_close_ts: i64,
        full_instrument_id: broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        event_key_fingerprint: [u8; 32],
        continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
    }

    impl Stage5eCallbackAuthorityReadyPaperStrategy {
        fn from_approved(
            b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
            approved: Stage5eCallbackAuthorityApproved,
        ) -> Self {
            Self {
                b3c_receipt,
                callback_authority_id: approved.callback_authority_id,
                issued_at: approved.issued_at,
                effective_observed_at: approved.effective_observed_at,
                authority_expires_at: approved.authority_expires_at,
                accepted_bar_close_ts: approved.accepted_bar_close_ts,
                full_instrument_id: approved.full_instrument_id,
                accepted_semantic_bar_identity: approved.accepted_semantic_bar_identity,
                event_key_fingerprint: approved.event_key_fingerprint,
                continuation_binding_id: approved.continuation_binding_id,
                sequence_identity_fingerprint: approved.sequence_identity_fingerprint,
            }
        }

        #[cfg(test)]
        pub(crate) fn test_authority_id(&self) -> [u8; 32] {
            self.callback_authority_id.0
        }

        #[cfg(test)]
        pub(crate) fn test_issued_at(&self) -> DateTime<Utc> {
            self.issued_at
        }

        #[cfg(test)]
        pub(crate) fn test_authority_expires_at(&self) -> DateTime<Utc> {
            self.authority_expires_at
        }

        #[cfg(test)]
        pub(crate) fn test_effective_observed_at(&self) -> DateTime<Utc> {
            self.effective_observed_at
        }

        #[cfg(test)]
        pub(crate) fn test_ownership_fingerprint(
            &self,
        ) -> (String, DateTime<Utc>, i64, usize, usize) {
            self.b3c_receipt.test_ownership_fingerprint()
        }

        #[cfg(test)]
        pub(crate) fn test_callback_count(&self) -> usize {
            0
        }

        #[cfg(test)]
        pub(crate) fn test_intent_count(&self) -> usize {
            0
        }

        #[cfg(test)]
        pub(crate) fn test_corrupt_callback_authority_id(&mut self) {
            self.callback_authority_id.0[0] ^= 1;
        }

        #[cfg(test)]
        pub(crate) fn test_corrupt_owned_sequence_identity(&mut self) {
            self.sequence_identity_fingerprint[0] ^= 1;
        }

        #[cfg(test)]
        pub(crate) fn test_zero_nested_schedule_identity(&mut self) {
            self.b3c_receipt.test_zero_owned_schedule_identity();
        }

        #[cfg(test)]
        pub(crate) fn test_force_nested_bound_after_issue(&mut self) {
            self.b3c_receipt
                .test_force_bound_at(self.issued_at + chrono::Duration::milliseconds(1));
        }
    }

    // STAGE5E-B3E-CALLBACK-IMPLEMENTATION-BEGIN: private-authority-v1
    pub(crate) struct Stage5eCallbackInvocationSeal(());
    pub(crate) struct Stage5eB3eNestedPreflightSeal(());
    pub(crate) struct Stage5eB3eNestedConsumeSeal(());
    pub(crate) struct Stage5cB3eCallbackExecutionSeal(());
    pub(crate) struct Stage5eEscrowConstructionSeal(());

    pub(crate) struct Stage5eB3eInvocationConsumeContext {
        callback_now: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        issued_at: DateTime<Utc>,
        effective_observed_at: DateTime<Utc>,
        authority_expires_at: DateTime<Utc>,
        full_instrument_id: broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        b3b_event_key_fingerprint: [u8; 32],
        b3c_continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
    }

    impl Stage5eB3eInvocationConsumeContext {
        pub(crate) fn consume_for_nested_b3c(
            self,
            nested_consume_capability: &Stage5eB3eNestedConsumeSeal,
        ) -> crate::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence::Stage5eB3eNestedInvocationMaterial
        {
            let Self {
                callback_now,
                callback_authority_id,
                issued_at,
                effective_observed_at,
                authority_expires_at,
                full_instrument_id,
                accepted_semantic_bar_identity,
                b3b_event_key_fingerprint,
                b3c_continuation_binding_id,
                sequence_identity_fingerprint,
            } = self;
            crate::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence::construct_nested_invocation_material(
                callback_now,
                callback_authority_id,
                issued_at,
                effective_observed_at,
                authority_expires_at,
                full_instrument_id,
                accepted_semantic_bar_identity,
                b3b_event_key_fingerprint,
                b3c_continuation_binding_id,
                sequence_identity_fingerprint,
                nested_consume_capability,
            )
        }
    }

    pub(crate) struct Stage5eB3eNestedPreflight<'a> {
        full_instrument_id: &'a broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        b3b_event_key_fingerprint: [u8; 32],
        b3c_continuation_binding_id: [u8; 32],
        schedule_window_identity_fingerprint: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
        accepted_bar_close_ts: i64,
        b3c_bound_at: DateTime<Utc>,
        effective_observed_at: DateTime<Utc>,
        effective_expires_at: DateTime<Utc>,
    }

    impl<'a> Stage5eB3eNestedPreflight<'a> {
        #[allow(clippy::too_many_arguments)]
        pub(crate) fn from_b3c_receipt(
            _seal: Stage5eB3eNestedPreflightSeal,
            full_instrument_id: &'a broker_core::InstrumentId,
            accepted_semantic_bar_identity: [u8; 32],
            b3b_event_key_fingerprint: [u8; 32],
            b3c_continuation_binding_id: [u8; 32],
            schedule_window_identity_fingerprint: [u8; 32],
            sequence_identity_fingerprint: [u8; 32],
            accepted_bar_close_ts: i64,
            b3c_bound_at: DateTime<Utc>,
            effective_observed_at: DateTime<Utc>,
            effective_expires_at: DateTime<Utc>,
        ) -> Self {
            Self {
                full_instrument_id,
                accepted_semantic_bar_identity,
                b3b_event_key_fingerprint,
                b3c_continuation_binding_id,
                schedule_window_identity_fingerprint,
                sequence_identity_fingerprint,
                accepted_bar_close_ts,
                b3c_bound_at,
                effective_observed_at,
                effective_expires_at,
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eCallbackInvocationTerminalReason {
        ClockBeforeAuthorityIssue,
        AuthorityExpired,
        AcceptedBarObservedInFuture,
        InvalidAuthorityChronology,
        InstrumentIdentityMissing,
        OwnedIdentityMismatch,
        CallbackAuthorityIdMismatch,
        MaterializationIntegrityMismatch,
    }

    pub(crate) struct Stage5eCallbackInvocationTerminalBlock {
        reason: Stage5eCallbackInvocationTerminalReason,
    }

    impl Stage5eCallbackInvocationTerminalBlock {
        pub(crate) fn reason(&self) -> Stage5eCallbackInvocationTerminalReason {
            self.reason
        }
    }

    pub(crate) struct Stage5eAuthorizedCallbackAuditLineage {
        _schedule_identity_fingerprint: [u8; 32],
        _sequence_classification:
            crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification,
        _optional_boundary_fingerprint: Option<[u8; 32]>,
        _owned_sequence_identity: [u8; 32],
        _sequence_observed_at: DateTime<Utc>,
        _sequence_expires_at: DateTime<Utc>,
        _event_key_fingerprint: [u8; 32],
        _b3b_effective_observed_at: DateTime<Utc>,
        _b3b_effective_expires_at: DateTime<Utc>,
        _continuation_binding_id: [u8; 32],
        _bound_at: DateTime<Utc>,
        _b3c_effective_observed_at: DateTime<Utc>,
        _b3c_effective_expires_at: DateTime<Utc>,
        _callback_authority_id: [u8; 32],
        _issued_at: DateTime<Utc>,
        _effective_observed_at: DateTime<Utc>,
        _authority_expires_at: DateTime<Utc>,
        _full_instrument_id: broker_core::InstrumentId,
        _accepted_semantic_bar_identity: [u8; 32],
        _b3b_event_key_fingerprint: [u8; 32],
        _b3c_continuation_binding_id: [u8; 32],
        _sequence_identity_fingerprint: [u8; 32],
        _owned_instrument: broker_core::InstrumentId,
        _owned_bar_identity: [u8; 32],
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn construct_stage5e_authorized_callback_audit_lineage(
        schedule_identity_fingerprint: [u8; 32],
        sequence_classification: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification,
        optional_boundary_fingerprint: Option<[u8; 32]>,
        owned_sequence_identity: [u8; 32],
        sequence_observed_at: DateTime<Utc>,
        sequence_expires_at: DateTime<Utc>,
        event_key_fingerprint: [u8; 32],
        b3b_effective_observed_at: DateTime<Utc>,
        b3b_effective_expires_at: DateTime<Utc>,
        continuation_binding_id: [u8; 32],
        bound_at: DateTime<Utc>,
        b3c_effective_observed_at: DateTime<Utc>,
        b3c_effective_expires_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        issued_at: DateTime<Utc>,
        effective_observed_at: DateTime<Utc>,
        authority_expires_at: DateTime<Utc>,
        full_instrument_id: broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        b3b_event_key_fingerprint: [u8; 32],
        b3c_continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
        owned_instrument: broker_core::InstrumentId,
        owned_bar_identity: [u8; 32],
        _nested_consume_capability: &Stage5eB3eNestedConsumeSeal,
    ) -> Stage5eAuthorizedCallbackAuditLineage {
        Stage5eAuthorizedCallbackAuditLineage {
            _schedule_identity_fingerprint: schedule_identity_fingerprint,
            _sequence_classification: sequence_classification,
            _optional_boundary_fingerprint: optional_boundary_fingerprint,
            _owned_sequence_identity: owned_sequence_identity,
            _sequence_observed_at: sequence_observed_at,
            _sequence_expires_at: sequence_expires_at,
            _event_key_fingerprint: event_key_fingerprint,
            _b3b_effective_observed_at: b3b_effective_observed_at,
            _b3b_effective_expires_at: b3b_effective_expires_at,
            _continuation_binding_id: continuation_binding_id,
            _bound_at: bound_at,
            _b3c_effective_observed_at: b3c_effective_observed_at,
            _b3c_effective_expires_at: b3c_effective_expires_at,
            _callback_authority_id: callback_authority_id,
            _issued_at: issued_at,
            _effective_observed_at: effective_observed_at,
            _authority_expires_at: authority_expires_at,
            _full_instrument_id: full_instrument_id,
            _accepted_semantic_bar_identity: accepted_semantic_bar_identity,
            _b3b_event_key_fingerprint: b3b_event_key_fingerprint,
            _b3c_continuation_binding_id: b3c_continuation_binding_id,
            _sequence_identity_fingerprint: sequence_identity_fingerprint,
            _owned_instrument: owned_instrument,
            _owned_bar_identity: owned_bar_identity,
        }
    }

    pub(crate) struct Stage5eAuthorizedPaperCallbackPayload {
        stage5c_authorized_callback_material:
            crate::stage5c_paper_host::Stage5eStage5cAuthorizedCallbackMaterial,
        authorized_callback_audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
    }

    pub(crate) fn construct_stage5e_authorized_paper_callback_payload(
        material: crate::stage5c_paper_host::Stage5eStage5cAuthorizedCallbackMaterial,
        audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        _nested_consume_capability: &Stage5eB3eNestedConsumeSeal,
    ) -> Stage5eAuthorizedPaperCallbackPayload {
        Stage5eAuthorizedPaperCallbackPayload {
            stage5c_authorized_callback_material: material,
            authorized_callback_audit_lineage: audit_lineage,
            callback_invoked_at,
            callback_authority_id,
        }
    }

    struct Stage5eAuthorizedPostCallbackPayload {
        post_callback_material: crate::stage5c_paper_host::Stage5eStage5cPostCallbackMaterial,
        audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
    }

    impl Stage5eAuthorizedPaperCallbackPayload {
        fn invoke_callback_once_in_authority(
            self,
            execution_seal: Stage5cB3eCallbackExecutionSeal,
        ) -> Stage5eAuthorizedPostCallbackPayload {
            let Self {
                stage5c_authorized_callback_material,
                authorized_callback_audit_lineage,
                callback_invoked_at,
                callback_authority_id,
            } = self;
            Stage5eAuthorizedPostCallbackPayload {
                post_callback_material: stage5c_authorized_callback_material
                    .invoke_authorized_callback_once(execution_seal),
                audit_lineage: authorized_callback_audit_lineage,
                callback_invoked_at,
                callback_authority_id,
            }
        }
    }

    enum PrivateStage5ePaperCallbackOutcome {
        Ok(Vec<crate::BrokerNeutralHybridIntent>),
        ValidationError(crate::HybridRuntimeCallbackValidationError),
    }

    pub(crate) struct Stage5ePaperCallbackOutcome {
        inner: PrivateStage5ePaperCallbackOutcome,
    }

    pub(crate) fn move_stage5e_paper_callback_outcome(
        exact_result: crate::BrokerNeutralHybridCallbackResult,
        _execution_capability: &Stage5cB3eCallbackExecutionSeal,
    ) -> Stage5ePaperCallbackOutcome {
        Stage5ePaperCallbackOutcome {
            inner: match exact_result {
                Ok(intents) => PrivateStage5ePaperCallbackOutcome::Ok(intents),
                Err(error) => PrivateStage5ePaperCallbackOutcome::ValidationError(error),
            },
        }
    }

    pub(crate) struct Stage5ePaperCallbackResultEscrow {
        mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
        audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
        attribution_snapshot: crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
        retained_bar_metadata: crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        callback_outcome: Stage5ePaperCallbackOutcome,
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn construct_stage5e_paper_callback_result_escrow(
        mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
        recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
        audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
        attribution_snapshot: crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
        retained_bar_metadata: crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
        callback_invoked_at: DateTime<Utc>,
        callback_authority_id: [u8; 32],
        callback_outcome: Stage5ePaperCallbackOutcome,
        _seal: Stage5eEscrowConstructionSeal,
    ) -> Stage5ePaperCallbackResultEscrow {
        Stage5ePaperCallbackResultEscrow {
            mutated_strategy,
            recovery_receipt,
            audit_lineage,
            attribution_snapshot,
            retained_bar_metadata,
            callback_invoked_at,
            callback_authority_id,
            callback_outcome,
        }
    }

    pub(crate) fn map_stage5c_materialization_terminal_to_callback_terminal(
        _block: crate::stage5c_paper_host::Stage5eStage5cMaterializationTerminalBlock,
        _nested_consume_capability: &Stage5eB3eNestedConsumeSeal,
    ) -> Stage5eCallbackInvocationTerminalBlock {
        Stage5eCallbackInvocationTerminalBlock {
            reason: Stage5eCallbackInvocationTerminalReason::MaterializationIntegrityMismatch,
        }
    }

    impl Stage5eCallbackAuthorityReadyPaperStrategy {
        fn consume_for_callback(
            self,
            _invocation_seal: Stage5eCallbackInvocationSeal,
            invocation_context: Stage5eB3eInvocationConsumeContext,
        ) -> Result<Stage5eAuthorizedPaperCallbackPayload, Stage5eCallbackInvocationTerminalBlock>
        {
            self.b3c_receipt
                .consume_for_authorized_callback_with_nested_seal_and_invocation_context(
                    Stage5eB3eNestedConsumeSeal(()),
                    invocation_context,
                )
        }
    }

    pub(crate) fn invoke_stage5e_authorized_paper_callback(
        authority: Stage5eCallbackAuthorityReadyPaperStrategy,
    ) -> Result<Stage5ePaperCallbackResultEscrow, Stage5eCallbackInvocationTerminalBlock> {
        invoke_stage5e_authorized_paper_callback_with_now(authority, Utc::now())
    }

    #[cfg(test)]
    pub(crate) fn invoke_stage5e_authorized_paper_callback_at(
        authority: Stage5eCallbackAuthorityReadyPaperStrategy,
        callback_now: DateTime<Utc>,
    ) -> Result<Stage5ePaperCallbackResultEscrow, Stage5eCallbackInvocationTerminalBlock> {
        invoke_stage5e_authorized_paper_callback_with_now(authority, callback_now)
    }

    fn invoke_stage5e_authorized_paper_callback_with_now(
        authority: Stage5eCallbackAuthorityReadyPaperStrategy,
        callback_now: DateTime<Utc>,
    ) -> Result<Stage5ePaperCallbackResultEscrow, Stage5eCallbackInvocationTerminalBlock> {
        {
            let nested_preflight = authority
                .b3c_receipt
                .borrow_for_authorized_callback_preflight(Stage5eB3eNestedPreflightSeal(()));
            validate_callback_invocation_preflight(&authority, nested_preflight, callback_now)?;
        }
        let callback_authority_id = authority.callback_authority_id.0;
        let invocation_context = Stage5eB3eInvocationConsumeContext {
            callback_now,
            callback_authority_id,
            issued_at: authority.issued_at,
            effective_observed_at: authority.effective_observed_at,
            authority_expires_at: authority.authority_expires_at,
            full_instrument_id: authority.full_instrument_id.clone(),
            accepted_semantic_bar_identity: authority.accepted_semantic_bar_identity,
            b3b_event_key_fingerprint: authority.event_key_fingerprint,
            b3c_continuation_binding_id: authority.continuation_binding_id,
            sequence_identity_fingerprint: authority.sequence_identity_fingerprint,
        };
        let payload = authority
            .consume_for_callback(Stage5eCallbackInvocationSeal(()), invocation_context)?;
        let post_callback =
            payload.invoke_callback_once_in_authority(Stage5cB3eCallbackExecutionSeal(()));
        let Stage5eAuthorizedPostCallbackPayload {
            post_callback_material,
            audit_lineage,
            callback_invoked_at,
            callback_authority_id,
        } = post_callback;
        Ok(post_callback_material.construct_result_escrow(
            audit_lineage,
            callback_invoked_at,
            callback_authority_id,
            Stage5eEscrowConstructionSeal(()),
        ))
    }

    fn validate_callback_invocation_preflight(
        authority: &Stage5eCallbackAuthorityReadyPaperStrategy,
        nested: Stage5eB3eNestedPreflight<'_>,
        callback_now: DateTime<Utc>,
    ) -> Result<(), Stage5eCallbackInvocationTerminalBlock> {
        let block = |reason| Stage5eCallbackInvocationTerminalBlock { reason };
        if callback_now < authority.issued_at {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::ClockBeforeAuthorityIssue,
            ));
        }
        if callback_now > authority.authority_expires_at {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::AuthorityExpired,
            ));
        }
        if authority.accepted_bar_close_ts > callback_now.timestamp() {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::AcceptedBarObservedInFuture,
            ));
        }
        if authority.effective_observed_at > authority.issued_at
            || nested.b3c_bound_at > authority.issued_at
            || authority.issued_at > callback_now
            || authority.effective_observed_at > authority.authority_expires_at
            || nested.effective_observed_at > nested.effective_expires_at
            || authority.accepted_bar_close_ts > authority.issued_at.timestamp()
        {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::InvalidAuthorityChronology,
            ));
        }
        if !instrument_identity_is_complete(&authority.full_instrument_id) {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::InstrumentIdentityMissing,
            ));
        }
        if [
            nested.accepted_semantic_bar_identity,
            nested.b3b_event_key_fingerprint,
            nested.b3c_continuation_binding_id,
            nested.schedule_window_identity_fingerprint,
            nested.sequence_identity_fingerprint,
        ]
        .contains(&[0; 32])
        {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::OwnedIdentityMismatch,
            ));
        }
        let recomputed_event_key = super::schedule_window_evidence::b3b_event_key_fingerprint(
            nested.schedule_window_identity_fingerprint,
            nested.full_instrument_id,
            nested.accepted_bar_close_ts,
            nested.sequence_identity_fingerprint,
        );
        if recomputed_event_key != nested.b3b_event_key_fingerprint {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::OwnedIdentityMismatch,
            ));
        }
        if authority.full_instrument_id != *nested.full_instrument_id
            || authority.accepted_semantic_bar_identity != nested.accepted_semantic_bar_identity
            || authority.event_key_fingerprint != nested.b3b_event_key_fingerprint
            || authority.continuation_binding_id != nested.b3c_continuation_binding_id
            || authority.sequence_identity_fingerprint != nested.sequence_identity_fingerprint
            || authority.accepted_bar_close_ts != nested.accepted_bar_close_ts
            || authority.effective_observed_at != nested.effective_observed_at
            || authority.authority_expires_at != nested.effective_expires_at
        {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::OwnedIdentityMismatch,
            ));
        }
        let recomputed = callback_authority_id(
            nested.full_instrument_id,
            nested.accepted_semantic_bar_identity,
            nested.b3b_event_key_fingerprint,
            nested.b3c_continuation_binding_id,
            nested.sequence_identity_fingerprint,
            authority.issued_at,
            authority.authority_expires_at,
        );
        if recomputed.0 != authority.callback_authority_id.0 {
            return Err(block(
                Stage5eCallbackInvocationTerminalReason::CallbackAuthorityIdMismatch,
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    type Stage5eTestAuditProofVector = ([u8; 32], [u8; 32], [u8; 32], [u8; 32], DateTime<Utc>);

    #[cfg(test)]
    #[derive(Debug, Clone, Copy)]
    pub(crate) enum Stage5eB3fPreflightTestMutation {
        StrategyId,
        AccountId,
        FullInstrumentId,
        SemanticBarIdentity,
        AcceptedBarClose,
        AuditEventKey,
        PaperMode,
        AcceptedBarOrigin,
        ExecutionEligibility,
    }

    #[cfg(test)]
    impl Stage5ePaperCallbackResultEscrow {
        pub(crate) fn test_callback_count(&self) -> usize {
            1
        }

        pub(crate) fn test_intent_count(&self) -> usize {
            match &self.callback_outcome.inner {
                PrivateStage5ePaperCallbackOutcome::Ok(intents) => intents.len(),
                PrivateStage5ePaperCallbackOutcome::ValidationError(_) => 0,
            }
        }

        pub(crate) fn test_has_validation_error(&self) -> bool {
            matches!(
                self.callback_outcome.inner,
                PrivateStage5ePaperCallbackOutcome::ValidationError(_)
            )
        }

        pub(crate) fn test_callback_invoked_at(&self) -> DateTime<Utc> {
            self.callback_invoked_at
        }

        pub(crate) fn test_strategy_state_fingerprint(&self) -> String {
            crate::stage5c_paper_host::stage5e_test_owned_strategy_state_fingerprint(
                &self.mutated_strategy,
            )
        }

        // STAGE5F-TEST-POST-CALLBACK-INSPECTION-BEGIN
        pub(crate) fn test_strategy_state_value(&self) -> serde_json::Value {
            serde_json::to_value(crate::runtime_compat::Strategy::state(
                &self.mutated_strategy,
            ))
            .expect("Stage 5F callback strategy state must serialize")
        }

        pub(crate) fn test_runtime_private_extension(
            &self,
        ) -> crate::stage5d_persistence::Stage5dRuntimePrivateExtension {
            self.mutated_strategy
                .stage5d_export_runtime_private_extension()
                .expect("Stage 5F callback private extension must export")
        }

        pub(crate) fn test_clear_public_pending_entry_request(&mut self) {
            use crate::runtime_compat::{Strategy, StrategyState};

            let mut state = Strategy::state(&self.mutated_strategy).clone();
            let StrategyState::HybridIntradayRuntime {
                pending_entry_request_id,
                ..
            } = &mut state
            else {
                panic!("Stage 5F pending-mismatch test requires hybrid runtime state");
            };
            assert!(
                pending_entry_request_id.take().is_some(),
                "Stage 5F pending-mismatch test requires a source pending request"
            );
            Strategy::set_state(&mut self.mutated_strategy, state);
        }
        // STAGE5F-TEST-POST-CALLBACK-INSPECTION-END

        pub(crate) fn test_retained_ownership(&self) -> (DateTime<Utc>, [u8; 32], [u8; 32]) {
            (
                self.recovery_receipt.recovered_ts(),
                self.callback_authority_id,
                self.audit_lineage._accepted_semantic_bar_identity,
            )
        }

        pub(crate) fn test_attribution_ownership_shape(&self) -> (usize, usize, bool) {
            self.attribution_snapshot.test_ownership_shape()
        }

        pub(crate) fn test_attribution_binding_vector(
            &self,
        ) -> (
            String,
            broker_core::BrokerAccountId,
            broker_core::InstrumentId,
            [u8; 32],
            i64,
        ) {
            self.attribution_snapshot.test_binding_vector()
        }

        pub(crate) fn test_audit_proof_vector(&self) -> Stage5eTestAuditProofVector {
            (
                self.audit_lineage._schedule_identity_fingerprint,
                self.audit_lineage._event_key_fingerprint,
                self.audit_lineage._continuation_binding_id,
                self.audit_lineage._owned_sequence_identity,
                self.audit_lineage._bound_at,
            )
        }

        pub(crate) fn test_retained_bar_metadata(
            &self,
        ) -> (i64, broker_core::HybridRuntimeBarOrigin, bool, [u8; 32]) {
            self.retained_bar_metadata.test_retained_bar_metadata()
        }

        pub(crate) fn test_repeat_first_ok_intent(&mut self, count: usize) {
            let PrivateStage5ePaperCallbackOutcome::Ok(intents) = &mut self.callback_outcome.inner
            else {
                panic!("test requires an Ok callback outcome");
            };
            let template = intents
                .first()
                .cloned()
                .expect("test requires a non-empty callback outcome");
            intents.clear();
            intents.resize(count, template);
        }

        pub(crate) fn test_set_callback_before_retained_close(&mut self) {
            self.callback_invoked_at = DateTime::from_timestamp(
                self.retained_bar_metadata.test_retained_bar_metadata().0 - 1,
                0,
            )
            .expect("canonical test close timestamp is representable");
        }

        pub(crate) fn test_force_retained_close_after_issue(&mut self) {
            let accepted_bar_close_ts = self.audit_lineage._issued_at.timestamp() + 1;
            self.attribution_snapshot
                .test_set_accepted_bar_close_ts(accepted_bar_close_ts);
            self.retained_bar_metadata
                .test_set_accepted_bar_close_ts(accepted_bar_close_ts);
            let event_key = super::schedule_window_evidence::b3b_event_key_fingerprint(
                self.audit_lineage._schedule_identity_fingerprint,
                &self.audit_lineage._full_instrument_id,
                accepted_bar_close_ts,
                self.audit_lineage._sequence_identity_fingerprint,
            );
            self.audit_lineage._event_key_fingerprint = event_key;
            self.audit_lineage._b3b_event_key_fingerprint = event_key;
        }

        pub(crate) fn test_corrupt_b3c_outer_chronology_equality(&mut self) {
            self.audit_lineage._b3c_effective_observed_at += chrono::Duration::nanoseconds(1);
        }

        pub(crate) fn test_set_both_authority_ids_same_wrong_nonzero(&mut self) {
            self.callback_authority_id[0] ^= 1;
            self.audit_lineage._callback_authority_id = self.callback_authority_id;
            assert_ne!(self.callback_authority_id, [0; 32]);
        }

        pub(crate) fn test_corrupt_canonical_authority_input_without_recomputing_id(&mut self) {
            self.audit_lineage._continuation_binding_id[0] ^= 1;
            self.audit_lineage._b3c_continuation_binding_id =
                self.audit_lineage._continuation_binding_id;
            assert_ne!(self.audit_lineage._continuation_binding_id, [0; 32]);
        }

        pub(crate) fn test_corrupt_stage5c_preflight_binding(
            &mut self,
            mutation: Stage5eB3fPreflightTestMutation,
        ) {
            match mutation {
                Stage5eB3fPreflightTestMutation::StrategyId => {
                    self.attribution_snapshot.test_corrupt_strategy_id();
                }
                Stage5eB3fPreflightTestMutation::AccountId => {
                    self.attribution_snapshot.test_corrupt_account_id();
                }
                Stage5eB3fPreflightTestMutation::FullInstrumentId => {
                    self.attribution_snapshot.test_corrupt_target_instrument();
                }
                Stage5eB3fPreflightTestMutation::SemanticBarIdentity => {
                    self.attribution_snapshot
                        .test_corrupt_semantic_bar_identity();
                }
                Stage5eB3fPreflightTestMutation::AcceptedBarClose => {
                    self.attribution_snapshot.test_set_accepted_bar_close_ts(
                        self.retained_bar_metadata.test_retained_bar_metadata().0 - 1,
                    );
                }
                Stage5eB3fPreflightTestMutation::AuditEventKey => {
                    self.audit_lineage._event_key_fingerprint[0] ^= 1;
                }
                Stage5eB3fPreflightTestMutation::PaperMode => {
                    self.recovery_receipt.test_disable_paper_mode();
                }
                Stage5eB3fPreflightTestMutation::AcceptedBarOrigin => {
                    self.retained_bar_metadata
                        .test_corrupt_accepted_bar_origin();
                }
                Stage5eB3fPreflightTestMutation::ExecutionEligibility => {
                    self.retained_bar_metadata
                        .test_disable_execution_eligibility();
                }
            }
        }
    }

    // STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION-BEGIN: private-process-local-v1
    impl Stage5ePaperCallbackResultEscrow {
        fn borrow_for_settlement_preflight(
            &self,
            seal: &callback_settlement::Stage5ePaperSettlementPreflightSeal,
        ) -> callback_settlement::Stage5ePaperSettlementPreflight<'_> {
            callback_settlement::Stage5ePaperSettlementPreflight::from_escrow(self, seal)
        }

        fn consume_for_settlement(
            self,
            seal: &callback_settlement::Stage5ePaperSettlementConsumeSeal,
        ) -> callback_settlement::Stage5ePaperSettlementPayload {
            callback_settlement::Stage5ePaperSettlementPayload::from_escrow(self, seal)
        }
    }

    pub(crate) mod callback_settlement {
        use super::{
            DateTime, Digest, PrivateStage5ePaperCallbackOutcome,
            Stage5eAuthorizedCallbackAuditLineage, Stage5ePaperCallbackOutcome,
            Stage5ePaperCallbackResultEscrow, Utc,
        };
        use sha2::Sha256;

        pub(crate) struct Stage5ePaperSettlementPreflightSeal(());
        pub(crate) struct Stage5ePaperSettlementConsumeSeal(());
        pub(crate) struct Stage5ePaperSettlementSuccessSeal(());
        pub(crate) struct Stage5ePaperSettlementTerminalSeal(());
        struct Stage5eB3fAuditCommitmentSeal(());

        pub(super) struct Stage5ePaperSettlementPreflight<'a> {
            escrow: &'a Stage5ePaperCallbackResultEscrow,
        }

        impl<'a> Stage5ePaperSettlementPreflight<'a> {
            pub(super) fn from_escrow(
                escrow: &'a Stage5ePaperCallbackResultEscrow,
                _seal: &Stage5ePaperSettlementPreflightSeal,
            ) -> Self {
                Self { escrow }
            }
        }

        #[cfg(doctest)]
        pub(crate) fn b3f_doctest_borrow_preflight(
            escrow: &Stage5ePaperCallbackResultEscrow,
            seal: &Stage5ePaperSettlementPreflightSeal,
        ) {
            let _actual_preflight = escrow.borrow_for_settlement_preflight(seal);
        }

        #[cfg(doctest)]
        pub(crate) fn b3f_doctest_consume_escrow(
            escrow: Stage5ePaperCallbackResultEscrow,
            seal: &Stage5ePaperSettlementConsumeSeal,
        ) {
            let _actual_payload = escrow.consume_for_settlement(seal);
        }

        pub(super) struct Stage5ePaperSettlementPayload {
            mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
            recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
            audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
            pre_callback_attribution_snapshot:
                crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
            retained_bar_metadata: crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
            callback_invoked_at: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            callback_outcome: Stage5ePaperCallbackOutcome,
        }

        impl Stage5ePaperSettlementPayload {
            pub(super) fn from_escrow(
                escrow: Stage5ePaperCallbackResultEscrow,
                _seal: &Stage5ePaperSettlementConsumeSeal,
            ) -> Self {
                let Stage5ePaperCallbackResultEscrow {
                    mutated_strategy,
                    recovery_receipt,
                    audit_lineage,
                    attribution_snapshot,
                    retained_bar_metadata,
                    callback_invoked_at,
                    callback_authority_id,
                    callback_outcome,
                } = escrow;
                Self {
                    mutated_strategy,
                    recovery_receipt,
                    audit_lineage,
                    pre_callback_attribution_snapshot: attribution_snapshot,
                    retained_bar_metadata,
                    callback_invoked_at,
                    callback_authority_id,
                    callback_outcome,
                }
            }
        }

        enum Stage5ePaperSettlementPreflightDecision {
            ProceedOk,
            Terminal(Stage5ePaperSettlementTerminalReason),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Stage5ePaperSettlementTerminalReason {
            CallbackValidationError,
            IntentCapacityExceeded,
            IdentityMismatch,
            ChronologyMismatch,
            PaperModeMismatch,
            Stage5cIntentValidationFailed,
            Stage5cPendingRequestMismatch,
        }

        enum Stage5ePaperSettlementTerminalOwnership {
            PreflightOk {
                _mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
                _recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
                _audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
                _pre_callback_attribution_snapshot:
                    crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
                _retained_bar_metadata:
                    crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
                _callback_invoked_at: DateTime<Utc>,
                _callback_authority_id: [u8; 32],
                _callback_outcome: Stage5ePaperCallbackOutcome,
            },
            CallbackValidationError {
                _mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
                _recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
                _audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
                _pre_callback_attribution_snapshot:
                    crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
                _retained_bar_metadata:
                    crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
                _callback_invoked_at: DateTime<Utc>,
                _callback_authority_id: [u8; 32],
                _callback_error: crate::HybridRuntimeCallbackValidationError,
            },
            Stage5c {
                _mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
                _recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
                _audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
                _pre_callback_attribution_snapshot:
                    crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
                _retained_bar_metadata:
                    crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
                _callback_invoked_at: DateTime<Utc>,
                _callback_authority_id: [u8; 32],
                _exact_stage5c_error: crate::stage5c_paper_host::Stage5cIntentSettlementError,
            },
        }

        pub(crate) struct Stage5ePaperSettlementTerminalReceipt {
            reason: Stage5ePaperSettlementTerminalReason,
            _ownership: Stage5ePaperSettlementTerminalOwnership,
            original_intent_count: usize,
            _audit_commitment: [u8; 32],
        }

        pub(crate) struct Stage5eValidatedPaperSettlementReceipt {
            _settlement_success: crate::stage5c_paper_host::Stage5eStage5cSettlementSuccess,
            _audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
            _callback_invoked_at: DateTime<Utc>,
            _callback_authority_id: [u8; 32],
            settlement_identity: [u8; 32],
        }

        #[allow(clippy::result_large_err)]
        pub(crate) fn validate_and_settle_stage5e_paper_callback_escrow(
            escrow: Stage5ePaperCallbackResultEscrow,
        ) -> Result<Stage5eValidatedPaperSettlementReceipt, Stage5ePaperSettlementTerminalReceipt>
        {
            let preflight_seal = Stage5ePaperSettlementPreflightSeal(());
            let decision = {
                let preflight = escrow.borrow_for_settlement_preflight(&preflight_seal);
                validate_preflight(preflight, &preflight_seal)
            };
            let consume_capability = Stage5ePaperSettlementConsumeSeal(());
            let payload = escrow.consume_for_settlement(&consume_capability);
            let audit_commitment = construct_stage5e_b3f_audit_commitment(
                &payload.audit_lineage,
                &Stage5eB3fAuditCommitmentSeal(()),
            );
            if let Stage5ePaperSettlementPreflightDecision::Terminal(reason) = decision {
                return Err(construct_preflight_terminal_receipt(
                    payload,
                    reason,
                    audit_commitment,
                ));
            }
            let Stage5ePaperSettlementPayload {
                mutated_strategy,
                recovery_receipt,
                audit_lineage,
                pre_callback_attribution_snapshot,
                retained_bar_metadata,
                callback_invoked_at,
                callback_authority_id,
                callback_outcome,
            } = payload;
            let exact_intent_vector = match callback_outcome.inner {
                PrivateStage5ePaperCallbackOutcome::Ok(intents) => intents,
                PrivateStage5ePaperCallbackOutcome::ValidationError(error) => {
                    return Err(Stage5ePaperSettlementTerminalReceipt {
                        reason: Stage5ePaperSettlementTerminalReason::CallbackValidationError,
                        _ownership:
                            Stage5ePaperSettlementTerminalOwnership::CallbackValidationError {
                                _mutated_strategy: mutated_strategy,
                                _recovery_receipt: recovery_receipt,
                                _audit_lineage: audit_lineage,
                                _pre_callback_attribution_snapshot:
                                    pre_callback_attribution_snapshot,
                                _retained_bar_metadata: retained_bar_metadata,
                                _callback_invoked_at: callback_invoked_at,
                                _callback_authority_id: callback_authority_id,
                                _callback_error: error,
                            },
                        original_intent_count: 0,
                        _audit_commitment: audit_commitment,
                    });
                }
            };
            let material_seal =
                crate::stage5c_paper_host::issue_stage5c_b3f_settlement_material_seal(
                    &consume_capability,
                );
            let material = crate::stage5c_paper_host::construct_stage5e_stage5c_settlement_material(
                mutated_strategy,
                recovery_receipt,
                pre_callback_attribution_snapshot,
                retained_bar_metadata,
                exact_intent_vector,
                material_seal,
            );
            let settlement_seal =
                crate::stage5c_paper_host::issue_stage5c_b3f_settlement_seal(&consume_capability);
            match crate::stage5c_paper_host::settle_stage5e_callback_escrow_material(
                material,
                settlement_seal,
            ) {
                Ok(success) => {
                    let accepted_semantic_bar_identity =
                        audit_lineage._accepted_semantic_bar_identity;
                    Ok(success.construct_stage5e_success_receipt(
                        audit_lineage,
                        callback_invoked_at,
                        callback_authority_id,
                        accepted_semantic_bar_identity,
                        audit_commitment,
                        Stage5ePaperSettlementSuccessSeal(()),
                    ))
                }
                Err(terminal) => Err(terminal.construct_stage5e_terminal_receipt(
                    audit_lineage,
                    callback_invoked_at,
                    callback_authority_id,
                    audit_commitment,
                    Stage5ePaperSettlementTerminalSeal(()),
                )),
            }
        }

        fn validate_preflight(
            preflight: Stage5ePaperSettlementPreflight<'_>,
            seal: &Stage5ePaperSettlementPreflightSeal,
        ) -> Stage5ePaperSettlementPreflightDecision {
            let escrow = preflight.escrow;
            let expected = construct_stage5c_expected_preflight_binding(escrow, seal);
            if let Err(mismatch) =
                crate::stage5c_paper_host::validate_stage5e_b3f_stage5c_preflight_binding(
                    &escrow.recovery_receipt,
                    &escrow.attribution_snapshot,
                    &escrow.retained_bar_metadata,
                    &expected,
                    seal,
                )
            {
                return Stage5ePaperSettlementPreflightDecision::Terminal(
                    map_stage5c_preflight_mismatch_exact(mismatch, seal),
                );
            }
            if crate::stage5c_paper_host::validate_stage5e_b3f_retained_close_chronology(
                &escrow.retained_bar_metadata,
                escrow.audit_lineage._issued_at,
                escrow.callback_invoked_at,
                seal,
            )
            .is_err()
            {
                return Stage5ePaperSettlementPreflightDecision::Terminal(
                    Stage5ePaperSettlementTerminalReason::ChronologyMismatch,
                );
            }
            if !stage5e_audit_chronology_matches(&escrow.audit_lineage, escrow.callback_invoked_at)
            {
                return Stage5ePaperSettlementPreflightDecision::Terminal(
                    Stage5ePaperSettlementTerminalReason::ChronologyMismatch,
                );
            }
            if !stage5e_authority_identity_matches(escrow) {
                return Stage5ePaperSettlementPreflightDecision::Terminal(
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch,
                );
            }
            match &escrow.callback_outcome.inner {
                PrivateStage5ePaperCallbackOutcome::ValidationError(_) => {
                    Stage5ePaperSettlementPreflightDecision::Terminal(
                        Stage5ePaperSettlementTerminalReason::CallbackValidationError,
                    )
                }
                PrivateStage5ePaperCallbackOutcome::Ok(intents)
                    if intents.len() > u8::MAX as usize =>
                {
                    Stage5ePaperSettlementPreflightDecision::Terminal(
                        Stage5ePaperSettlementTerminalReason::IntentCapacityExceeded,
                    )
                }
                PrivateStage5ePaperCallbackOutcome::Ok(_) => {
                    Stage5ePaperSettlementPreflightDecision::ProceedOk
                }
            }
        }

        fn construct_stage5c_expected_preflight_binding<'a>(
            escrow: &'a Stage5ePaperCallbackResultEscrow,
            seal: &Stage5ePaperSettlementPreflightSeal,
        ) -> crate::stage5c_paper_host::Stage5eB3fStage5cExpectedPreflightBinding<'a> {
            crate::stage5c_paper_host::construct_stage5e_b3f_stage5c_expected_preflight_binding(
                &escrow.audit_lineage._schedule_identity_fingerprint,
                &escrow.audit_lineage._sequence_identity_fingerprint,
                &escrow.audit_lineage._event_key_fingerprint,
                &escrow.audit_lineage._b3b_event_key_fingerprint,
                &escrow.audit_lineage._full_instrument_id,
                &escrow.audit_lineage._owned_instrument,
                &escrow.audit_lineage._accepted_semantic_bar_identity,
                &escrow.audit_lineage._owned_bar_identity,
                seal,
            )
        }

        #[cfg(test)]
        pub(crate) fn test_validate_stage5c_preflight_binding(
            escrow: &Stage5ePaperCallbackResultEscrow,
        ) -> Result<(), crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch> {
            let seal = Stage5ePaperSettlementPreflightSeal(());
            let expected = construct_stage5c_expected_preflight_binding(escrow, &seal);
            crate::stage5c_paper_host::validate_stage5e_b3f_stage5c_preflight_binding(
                &escrow.recovery_receipt,
                &escrow.attribution_snapshot,
                &escrow.retained_bar_metadata,
                &expected,
                &seal,
            )
            .map(|_| ())
        }

        fn stage5e_authority_identity_matches(escrow: &Stage5ePaperCallbackResultEscrow) -> bool {
            let audit = &escrow.audit_lineage;
            let nonzero = [
                audit._schedule_identity_fingerprint,
                audit._owned_sequence_identity,
                audit._event_key_fingerprint,
                audit._continuation_binding_id,
                audit._callback_authority_id,
                audit._accepted_semantic_bar_identity,
                audit._b3b_event_key_fingerprint,
                audit._b3c_continuation_binding_id,
                audit._sequence_identity_fingerprint,
                audit._owned_bar_identity,
            ]
            .iter()
            .all(|value| *value != [0; 32]);
            let recomputed = super::callback_authority_id(
                &audit._full_instrument_id,
                audit._accepted_semantic_bar_identity,
                audit._b3b_event_key_fingerprint,
                audit._b3c_continuation_binding_id,
                audit._sequence_identity_fingerprint,
                audit._issued_at,
                audit._authority_expires_at,
            );
            nonzero
                && escrow.callback_authority_id == audit._callback_authority_id
                && recomputed.0 == escrow.callback_authority_id
                && recomputed.0 == audit._callback_authority_id
                && audit._event_key_fingerprint == audit._b3b_event_key_fingerprint
                && audit._continuation_binding_id == audit._b3c_continuation_binding_id
                && audit._owned_sequence_identity == audit._sequence_identity_fingerprint
                && audit._full_instrument_id == audit._owned_instrument
                && audit._accepted_semantic_bar_identity == audit._owned_bar_identity
        }

        fn stage5e_audit_chronology_matches(
            audit: &Stage5eAuthorizedCallbackAuditLineage,
            callback_invoked_at: DateTime<Utc>,
        ) -> bool {
            audit._sequence_observed_at <= audit._sequence_expires_at
                && audit._b3b_effective_observed_at <= audit._b3b_effective_expires_at
                && audit._b3c_effective_observed_at <= audit._bound_at
                && audit._bound_at <= audit._b3c_effective_expires_at
                && audit._bound_at <= audit._issued_at
                && audit._b3c_effective_observed_at == audit._effective_observed_at
                && audit._b3c_effective_expires_at == audit._authority_expires_at
                && audit._effective_observed_at <= audit._issued_at
                && audit._issued_at <= callback_invoked_at
                && callback_invoked_at <= audit._authority_expires_at
        }

        fn map_stage5c_preflight_mismatch_exact(
            mismatch: crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch,
            _seal: &Stage5ePaperSettlementPreflightSeal,
        ) -> Stage5ePaperSettlementTerminalReason {
            match mismatch {
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::StrategyId => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::AccountId => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::FullInstrumentId => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::SemanticBarIdentity => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::AcceptedBarClose => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::AuditEventKey => {
                    Stage5ePaperSettlementTerminalReason::IdentityMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::PaperMode => {
                    Stage5ePaperSettlementTerminalReason::PaperModeMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::AcceptedBarOrigin => {
                    Stage5ePaperSettlementTerminalReason::PaperModeMismatch
                }
                crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch::ExecutionEligibility => {
                    Stage5ePaperSettlementTerminalReason::PaperModeMismatch
                }
            }
        }

        fn construct_preflight_terminal_receipt(
            payload: Stage5ePaperSettlementPayload,
            reason: Stage5ePaperSettlementTerminalReason,
            audit_commitment: [u8; 32],
        ) -> Stage5ePaperSettlementTerminalReceipt {
            let Stage5ePaperSettlementPayload {
                mutated_strategy,
                recovery_receipt,
                audit_lineage,
                pre_callback_attribution_snapshot,
                retained_bar_metadata,
                callback_invoked_at,
                callback_authority_id,
                callback_outcome,
            } = payload;
            match callback_outcome.inner {
                PrivateStage5ePaperCallbackOutcome::Ok(intents) => {
                    let original_intent_count = intents.len();
                    Stage5ePaperSettlementTerminalReceipt {
                        reason,
                        _ownership: Stage5ePaperSettlementTerminalOwnership::PreflightOk {
                            _mutated_strategy: mutated_strategy,
                            _recovery_receipt: recovery_receipt,
                            _audit_lineage: audit_lineage,
                            _pre_callback_attribution_snapshot: pre_callback_attribution_snapshot,
                            _retained_bar_metadata: retained_bar_metadata,
                            _callback_invoked_at: callback_invoked_at,
                            _callback_authority_id: callback_authority_id,
                            _callback_outcome: Stage5ePaperCallbackOutcome {
                                inner: PrivateStage5ePaperCallbackOutcome::Ok(intents),
                            },
                        },
                        original_intent_count,
                        _audit_commitment: audit_commitment,
                    }
                }
                PrivateStage5ePaperCallbackOutcome::ValidationError(error) => {
                    Stage5ePaperSettlementTerminalReceipt {
                        reason: Stage5ePaperSettlementTerminalReason::CallbackValidationError,
                        _ownership:
                            Stage5ePaperSettlementTerminalOwnership::CallbackValidationError {
                                _mutated_strategy: mutated_strategy,
                                _recovery_receipt: recovery_receipt,
                                _audit_lineage: audit_lineage,
                                _pre_callback_attribution_snapshot:
                                    pre_callback_attribution_snapshot,
                                _retained_bar_metadata: retained_bar_metadata,
                                _callback_invoked_at: callback_invoked_at,
                                _callback_authority_id: callback_authority_id,
                                _callback_error: error,
                            },
                        original_intent_count: 0,
                        _audit_commitment: audit_commitment,
                    }
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        pub(crate) fn construct_stage5e_paper_settlement_terminal_receipt(
            mutated_strategy: crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,
            recovery_receipt: crate::stage5c_paper_host::Stage5cPendingRecoveryReceipt,
            pre_callback_attribution_snapshot:
                crate::stage5c_paper_host::Stage5ePreCallbackAttributionSnapshot,
            retained_bar_metadata: crate::stage5c_paper_host::Stage5eAcceptedBarSettlementMetadata,
            audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
            callback_invoked_at: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            reason: Stage5ePaperSettlementTerminalReason,
            exact_stage5c_error: crate::stage5c_paper_host::Stage5cIntentSettlementError,
            original_intent_count: usize,
            audit_commitment: [u8; 32],
            _seal: Stage5ePaperSettlementTerminalSeal,
        ) -> Stage5ePaperSettlementTerminalReceipt {
            Stage5ePaperSettlementTerminalReceipt {
                reason,
                _ownership: Stage5ePaperSettlementTerminalOwnership::Stage5c {
                    _mutated_strategy: mutated_strategy,
                    _recovery_receipt: recovery_receipt,
                    _audit_lineage: audit_lineage,
                    _pre_callback_attribution_snapshot: pre_callback_attribution_snapshot,
                    _retained_bar_metadata: retained_bar_metadata,
                    _callback_invoked_at: callback_invoked_at,
                    _callback_authority_id: callback_authority_id,
                    _exact_stage5c_error: exact_stage5c_error,
                },
                original_intent_count,
                _audit_commitment: audit_commitment,
            }
        }

        pub(crate) fn construct_stage5e_validated_paper_settlement_receipt(
            settlement_success: crate::stage5c_paper_host::Stage5eStage5cSettlementSuccess,
            audit_lineage: Stage5eAuthorizedCallbackAuditLineage,
            callback_invoked_at: DateTime<Utc>,
            callback_authority_id: [u8; 32],
            settlement_identity: [u8; 32],
            _seal: Stage5ePaperSettlementSuccessSeal,
        ) -> Stage5eValidatedPaperSettlementReceipt {
            Stage5eValidatedPaperSettlementReceipt {
                _settlement_success: settlement_success,
                _audit_lineage: audit_lineage,
                _callback_invoked_at: callback_invoked_at,
                _callback_authority_id: callback_authority_id,
                settlement_identity,
            }
        }

        pub(crate) fn map_stage5c_settlement_error_exact(
            error: crate::stage5c_paper_host::Stage5cIntentSettlementError,
            _seal: &Stage5ePaperSettlementTerminalSeal,
        ) -> Stage5ePaperSettlementTerminalReason {
            match error {
                crate::stage5c_paper_host::Stage5cIntentSettlementError::TooManyIntents => {
                    Stage5ePaperSettlementTerminalReason::IntentCapacityExceeded
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::MissingIntentClass => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::InstrumentNamespaceMismatch => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::InvalidQuantity => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::InvalidPrice => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::PriceNotTickAligned => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::InvalidStopEnd => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::ReplayIntentNotExecutable => {
                    Stage5ePaperSettlementTerminalReason::PaperModeMismatch
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::MissingPendingRequest => {
                    Stage5ePaperSettlementTerminalReason::Stage5cPendingRequestMismatch
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::RequestIdMismatch => {
                    Stage5ePaperSettlementTerminalReason::Stage5cPendingRequestMismatch
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::DuplicateRequestId => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
                crate::stage5c_paper_host::Stage5cIntentSettlementError::UnsupportedIntentAction => {
                    Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed
                }
            }
        }

        fn construct_stage5e_b3f_audit_commitment(
            lineage: &Stage5eAuthorizedCallbackAuditLineage,
            _seal: &Stage5eB3fAuditCommitmentSeal,
        ) -> [u8; 32] {
            let mut encoder = Stage5eB3fCanonicalEncoder::new(b"stage5e-b3f-audit-commitment-v1\0");
            encoder.digest(&lineage._schedule_identity_fingerprint);
            encoder.schedule_classification(lineage._sequence_classification);
            encoder.optional_digest(lineage._optional_boundary_fingerprint.as_ref());
            encoder.digest(&lineage._owned_sequence_identity);
            encoder.datetime(lineage._sequence_observed_at);
            encoder.datetime(lineage._sequence_expires_at);
            encoder.digest(&lineage._event_key_fingerprint);
            encoder.datetime(lineage._b3b_effective_observed_at);
            encoder.datetime(lineage._b3b_effective_expires_at);
            encoder.digest(&lineage._continuation_binding_id);
            encoder.datetime(lineage._bound_at);
            encoder.datetime(lineage._b3c_effective_observed_at);
            encoder.datetime(lineage._b3c_effective_expires_at);
            encoder.digest(&lineage._callback_authority_id);
            encoder.datetime(lineage._issued_at);
            encoder.datetime(lineage._effective_observed_at);
            encoder.datetime(lineage._authority_expires_at);
            encoder.instrument(&lineage._full_instrument_id);
            encoder.digest(&lineage._accepted_semantic_bar_identity);
            encoder.digest(&lineage._b3b_event_key_fingerprint);
            encoder.digest(&lineage._b3c_continuation_binding_id);
            encoder.digest(&lineage._sequence_identity_fingerprint);
            encoder.instrument(&lineage._owned_instrument);
            encoder.digest(&lineage._owned_bar_identity);
            encoder.finish()
        }

        #[allow(clippy::too_many_arguments)]
        pub(crate) fn construct_stage5e_b3f_settlement_identity(
            callback_authority_id: [u8; 32],
            callback_invoked_at: DateTime<Utc>,
            accepted_semantic_bar_identity: [u8; 32],
            strategy_id: &str,
            account_id: &broker_core::BrokerAccountId,
            full_instrument_id: &broker_core::InstrumentId,
            accepted_bar_close_timestamp: i64,
            batch_state_fingerprint: &str,
            ordered_strategy_request_ids: &[broker_core::StrategyRequestId],
            intent_count_u8: u8,
            audit_commitment: [u8; 32],
            _seal: &Stage5ePaperSettlementSuccessSeal,
        ) -> [u8; 32] {
            let mut encoder =
                Stage5eB3fCanonicalEncoder::new(b"stage5e-b3f-settlement-identity-v1\0");
            encoder.digest(&callback_authority_id);
            encoder.datetime(callback_invoked_at);
            encoder.digest(&accepted_semantic_bar_identity);
            encoder.string(strategy_id);
            encoder.string(account_id.as_str());
            encoder.instrument(full_instrument_id);
            encoder.i64(accepted_bar_close_timestamp);
            encoder.string(batch_state_fingerprint);
            encoder.request_ids(ordered_strategy_request_ids);
            encoder.u8(intent_count_u8);
            encoder.digest(&audit_commitment);
            encoder.finish()
        }

        struct Stage5eB3fCanonicalEncoder {
            hasher: Sha256,
        }

        impl Stage5eB3fCanonicalEncoder {
            fn new(domain: &[u8]) -> Self {
                let mut hasher = Sha256::new();
                hasher.update(domain);
                Self { hasher }
            }

            fn bytes(&mut self, value: &[u8]) {
                self.hasher.update(value);
            }

            fn u8(&mut self, value: u8) {
                self.bytes(&[value]);
            }

            fn i64(&mut self, value: i64) {
                self.bytes(&value.to_be_bytes());
            }

            fn datetime(&mut self, value: DateTime<Utc>) {
                self.i64(value.timestamp());
                self.bytes(&value.timestamp_subsec_nanos().to_be_bytes());
            }

            fn digest(&mut self, value: &[u8; 32]) {
                self.bytes(value);
            }

            fn string(&mut self, value: &str) {
                let bytes = value.as_bytes();
                self.bytes(
                    &u32::try_from(bytes.len())
                        .expect("canonical B3F strings fit u32")
                        .to_be_bytes(),
                );
                self.bytes(bytes);
            }

            fn optional_string(&mut self, value: Option<&str>) {
                match value {
                    Some(value) => {
                        self.u8(1);
                        self.string(value);
                    }
                    None => self.u8(0),
                }
            }

            fn optional_digest(&mut self, value: Option<&[u8; 32]>) {
                match value {
                    Some(value) => {
                        self.u8(1);
                        self.digest(value);
                    }
                    None => self.u8(0),
                }
            }

            fn instrument(&mut self, instrument: &broker_core::InstrumentId) {
                self.string(&instrument.symbol);
                self.optional_string(instrument.venue_symbol.as_deref());
                match &instrument.exchange {
                    broker_core::Exchange::Moex => self.u8(1),
                    broker_core::Exchange::Other(value) => {
                        self.u8(0x7f);
                        self.string(value);
                    }
                }
                match &instrument.market {
                    broker_core::Market::Futures => self.u8(1),
                    broker_core::Market::Options => self.u8(2),
                    broker_core::Market::Stocks => self.u8(3),
                    broker_core::Market::Currency => self.u8(4),
                    broker_core::Market::Funds => self.u8(5),
                    broker_core::Market::Other(value) => {
                        self.u8(0x7f);
                        self.string(value);
                    }
                }
            }

            fn schedule_classification(
                &mut self,
                classification: crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification,
            ) {
                match classification {
                    crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification::Contiguous => {
                        self.u8(1);
                    }
                    crate::stage5e_no_io_lifecycle::schedule_window_evidence::Stage5eScheduleSequenceClassification::ApprovedNonTradableBoundary(fingerprint) => {
                        self.u8(2);
                        self.digest(&fingerprint);
                    }
                }
            }

            fn request_ids(&mut self, request_ids: &[broker_core::StrategyRequestId]) {
                self.bytes(
                    &u32::try_from(request_ids.len())
                        .expect("canonical B3F request-id vector fits u32")
                        .to_be_bytes(),
                );
                for request_id in request_ids {
                    self.bytes(request_id.as_uuid().as_bytes());
                }
            }

            fn finish(self) -> [u8; 32] {
                self.hasher.finalize().into()
            }
        }

        #[cfg(test)]
        impl Stage5ePaperSettlementTerminalReceipt {
            pub(crate) fn test_reason(&self) -> Stage5ePaperSettlementTerminalReason {
                self.reason
            }

            pub(crate) fn test_original_intent_count(&self) -> usize {
                self.original_intent_count
            }

            pub(crate) fn test_ownership_variant(&self) -> &'static str {
                match self._ownership {
                    Stage5ePaperSettlementTerminalOwnership::PreflightOk { .. } => "preflight_ok",
                    Stage5ePaperSettlementTerminalOwnership::CallbackValidationError { .. } => {
                        "callback_validation_error"
                    }
                    Stage5ePaperSettlementTerminalOwnership::Stage5c { .. } => "stage5c",
                }
            }
        }

        #[cfg(test)]
        impl Stage5eValidatedPaperSettlementReceipt {
            pub(crate) fn test_settlement_identity(&self) -> [u8; 32] {
                self.settlement_identity
            }

            pub(crate) fn test_identity_proof_shape(
                &self,
            ) -> (
                Vec<broker_core::StrategyRequestId>,
                usize,
                usize,
                bool,
                String,
            ) {
                self._settlement_success.test_identity_proof_shape()
            }
        }

        #[cfg(test)]
        mod tests {
            use super::*;
            use chrono::TimeZone;

            fn instrument() -> broker_core::InstrumentId {
                broker_core::InstrumentId {
                    symbol: "IMOEXF".to_string(),
                    venue_symbol: Some("IMOEXF@RTSX".to_string()),
                    exchange: broker_core::Exchange::Moex,
                    market: broker_core::Market::Futures,
                }
            }

            #[test]
            fn b3f_preflight_mismatch_mapper_is_exact_for_all_nine_variants() {
                use crate::stage5c_paper_host::Stage5eStage5cPreflightMismatch as Mismatch;
                let seal = Stage5ePaperSettlementPreflightSeal(());
                for mismatch in [
                    Mismatch::StrategyId,
                    Mismatch::AccountId,
                    Mismatch::FullInstrumentId,
                    Mismatch::SemanticBarIdentity,
                    Mismatch::AcceptedBarClose,
                    Mismatch::AuditEventKey,
                ] {
                    assert_eq!(
                        map_stage5c_preflight_mismatch_exact(mismatch, &seal),
                        Stage5ePaperSettlementTerminalReason::IdentityMismatch
                    );
                }
                for mismatch in [
                    Mismatch::PaperMode,
                    Mismatch::AcceptedBarOrigin,
                    Mismatch::ExecutionEligibility,
                ] {
                    assert_eq!(
                        map_stage5c_preflight_mismatch_exact(mismatch, &seal),
                        Stage5ePaperSettlementTerminalReason::PaperModeMismatch
                    );
                }
            }

            #[test]
            fn b3f_stage5c_error_mapper_is_exact_for_all_twelve_variants() {
                use crate::stage5c_paper_host::Stage5cIntentSettlementError as Error;
                let seal = Stage5ePaperSettlementTerminalSeal(());
                let cases = [
                    (
                        Error::TooManyIntents,
                        Stage5ePaperSettlementTerminalReason::IntentCapacityExceeded,
                    ),
                    (
                        Error::MissingIntentClass,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::InstrumentNamespaceMismatch,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::InvalidQuantity,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::InvalidPrice,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::PriceNotTickAligned,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::InvalidStopEnd,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::ReplayIntentNotExecutable,
                        Stage5ePaperSettlementTerminalReason::PaperModeMismatch,
                    ),
                    (
                        Error::MissingPendingRequest,
                        Stage5ePaperSettlementTerminalReason::Stage5cPendingRequestMismatch,
                    ),
                    (
                        Error::RequestIdMismatch,
                        Stage5ePaperSettlementTerminalReason::Stage5cPendingRequestMismatch,
                    ),
                    (
                        Error::DuplicateRequestId,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                    (
                        Error::UnsupportedIntentAction,
                        Stage5ePaperSettlementTerminalReason::Stage5cIntentValidationFailed,
                    ),
                ];
                for (error, expected) in cases {
                    assert_eq!(map_stage5c_settlement_error_exact(error, &seal), expected);
                }
            }

            #[test]
            fn b3f_event_key_validator_rejects_every_frozen_source_drift() {
                let seal = Stage5ePaperSettlementPreflightSeal(());
                let schedule = [1; 32];
                let sequence = [2; 32];
                let bar_close = 1_790_000_000;
                let instrument = instrument();
                let event =
                    crate::stage5e_no_io_lifecycle::schedule_window_evidence::
                        b3b_event_key_fingerprint(
                            schedule,
                            &instrument,
                            bar_close,
                            sequence,
                        );
                assert!(crate::stage5e_no_io_lifecycle::schedule_window_evidence::
                    validate_stage5e_b3f_b3b_event_key_binding(
                        &schedule,
                        &instrument,
                        bar_close,
                        &sequence,
                        &event,
                        &event,
                        &seal,
                    )
                    .is_ok());

                let mut changed_schedule = schedule;
                changed_schedule[0] ^= 1;
                let mut changed_sequence = sequence;
                changed_sequence[0] ^= 1;
                let mut changed_instrument = instrument.clone();
                changed_instrument.venue_symbol = Some("IMOEXF@OTHER".to_string());
                let mut changed_event = event;
                changed_event[0] ^= 1;
                let cases = [
                    (
                        changed_schedule,
                        instrument.clone(),
                        bar_close,
                        sequence,
                        event,
                        event,
                    ),
                    (
                        schedule,
                        changed_instrument,
                        bar_close,
                        sequence,
                        event,
                        event,
                    ),
                    (
                        schedule,
                        instrument.clone(),
                        bar_close + 1,
                        sequence,
                        event,
                        event,
                    ),
                    (
                        schedule,
                        instrument.clone(),
                        bar_close,
                        changed_sequence,
                        event,
                        event,
                    ),
                    (
                        schedule,
                        instrument.clone(),
                        bar_close,
                        sequence,
                        changed_event,
                        event,
                    ),
                    (
                        schedule,
                        instrument,
                        bar_close,
                        sequence,
                        event,
                        changed_event,
                    ),
                ];
                for (schedule, instrument, close, sequence, event, b3b_event) in cases {
                    assert!(crate::stage5e_no_io_lifecycle::schedule_window_evidence::
                        validate_stage5e_b3f_b3b_event_key_binding(
                            &schedule,
                            &instrument,
                            close,
                            &sequence,
                            &event,
                            &b3b_event,
                            &seal,
                        )
                        .is_err());
                }
            }

            #[test]
            fn b3f_settlement_identity_preserves_request_order_and_chronology() {
                let seal = Stage5ePaperSettlementSuccessSeal(());
                let now = Utc
                    .with_ymd_and_hms(2026, 7, 28, 10, 20, 0)
                    .single()
                    .unwrap();
                let first = broker_core::StrategyRequestId::new(uuid::Uuid::from_u128(1));
                let second = broker_core::StrategyRequestId::new(uuid::Uuid::from_u128(2));
                let account = broker_core::BrokerAccountId::new("ACC_TEST_0001");
                let canonical_instrument = instrument();
                let base = construct_stage5e_b3f_settlement_identity(
                    [1; 32],
                    now,
                    [2; 32],
                    "hybrid_imoexf",
                    &account,
                    &canonical_instrument,
                    now.timestamp(),
                    "state",
                    &[first, second],
                    2,
                    [3; 32],
                    &seal,
                );
                let reordered = construct_stage5e_b3f_settlement_identity(
                    [1; 32],
                    now,
                    [2; 32],
                    "hybrid_imoexf",
                    &account,
                    &canonical_instrument,
                    now.timestamp(),
                    "state",
                    &[second, first],
                    2,
                    [3; 32],
                    &seal,
                );
                assert_ne!(base, reordered);
                assert_ne!(base, [0; 32]);

                let mut changed_instrument = canonical_instrument.clone();
                changed_instrument.venue_symbol = Some("IMOEXF@OTHER".to_string());
                let changed = [
                    construct_stage5e_b3f_settlement_identity(
                        [9; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now + chrono::Duration::nanoseconds(1),
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [9; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_other",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &broker_core::BrokerAccountId::new("ACC_TEST_0002"),
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &changed_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp() - 1,
                        "state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "other-state",
                        &[first, second],
                        2,
                        [3; 32],
                        &seal,
                    ),
                    reordered,
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        1,
                        [3; 32],
                        &seal,
                    ),
                    construct_stage5e_b3f_settlement_identity(
                        [1; 32],
                        now,
                        [2; 32],
                        "hybrid_imoexf",
                        &account,
                        &canonical_instrument,
                        now.timestamp(),
                        "state",
                        &[first, second],
                        2,
                        [9; 32],
                        &seal,
                    ),
                ];
                for changed_identity in changed {
                    assert_ne!(base, changed_identity);
                }
            }
        }
    }
    // STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION-END: private-process-local-v1
    // STAGE5E-B3E-CALLBACK-IMPLEMENTATION-END: private-authority-v1

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eCallbackAuthorityRetryableReason {
        ClockBeforeEffectiveObservation,
        AcceptedBarObservedInFuture,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Stage5eCallbackAuthorityTerminalReason {
        EvidenceExpired,
        InvalidAuthorityChronology,
        AcceptedSemanticBarIdentityMissing,
        EventKeyMissing,
        ContinuationBindingMissing,
        ScheduleIdentityMissing,
        SequenceIdentityMissing,
        InstrumentIdentityMissing,
    }

    pub(crate) struct Stage5eCallbackAuthorityRetryableBlock {
        reason: Stage5eCallbackAuthorityRetryableReason,
        b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
    }

    impl Stage5eCallbackAuthorityRetryableBlock {
        pub(crate) fn reason(&self) -> Stage5eCallbackAuthorityRetryableReason {
            self.reason
        }

        pub(crate) fn into_retry_same_receipt(
            self,
        ) -> Stage5eBoundSessionCalendarSequenceForObservedLiveBar {
            self.b3c_receipt
        }
    }

    pub(crate) struct Stage5eCallbackAuthorityTerminalBlock {
        reason: Stage5eCallbackAuthorityTerminalReason,
    }

    impl Stage5eCallbackAuthorityTerminalBlock {
        pub(crate) fn reason(&self) -> Stage5eCallbackAuthorityTerminalReason {
            self.reason
        }
    }

    pub(crate) enum Stage5eCallbackAuthorityIssueBlocked {
        Retryable(Box<Stage5eCallbackAuthorityRetryableBlock>),
        Terminal(Stage5eCallbackAuthorityTerminalBlock),
    }

    struct Stage5eCallbackAuthorityApproved {
        callback_authority_id: Stage5eCallbackAuthorityId,
        issued_at: DateTime<Utc>,
        effective_observed_at: DateTime<Utc>,
        authority_expires_at: DateTime<Utc>,
        accepted_bar_close_ts: i64,
        full_instrument_id: broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        event_key_fingerprint: [u8; 32],
        continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
    }

    fn issue_callback_authority_seal_inside_issue_transition() -> Stage5eCallbackAuthorityIssueSeal
    {
        Stage5eCallbackAuthorityIssueSeal(())
    }

    pub(crate) fn issue_stage5e_callback_authority(
        b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
    ) -> Result<Stage5eCallbackAuthorityReadyPaperStrategy, Stage5eCallbackAuthorityIssueBlocked>
    {
        issue_stage5e_callback_authority_with_now(b3c_receipt, Utc::now())
    }

    #[cfg(test)]
    pub(crate) fn issue_stage5e_callback_authority_at(
        b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
        now: DateTime<Utc>,
    ) -> Result<Stage5eCallbackAuthorityReadyPaperStrategy, Stage5eCallbackAuthorityIssueBlocked>
    {
        issue_stage5e_callback_authority_with_now(b3c_receipt, now)
    }

    fn issue_stage5e_callback_authority_with_now(
        b3c_receipt: Stage5eBoundSessionCalendarSequenceForObservedLiveBar,
        now: DateTime<Utc>,
    ) -> Result<Stage5eCallbackAuthorityReadyPaperStrategy, Stage5eCallbackAuthorityIssueBlocked>
    {
        let validation = {
            let preflight = b3c_receipt.borrow_callback_authority_preflight(
                issue_callback_authority_seal_inside_issue_transition(),
            );
            validate_callback_authority_preflight(preflight, now)
        };
        match validation {
            Ok(approved) => Ok(Stage5eCallbackAuthorityReadyPaperStrategy::from_approved(
                b3c_receipt,
                approved,
            )),
            Err(Stage5eCallbackAuthorityValidationError::Retryable(reason)) => {
                Err(Stage5eCallbackAuthorityIssueBlocked::Retryable(Box::new(
                    Stage5eCallbackAuthorityRetryableBlock {
                        reason,
                        b3c_receipt,
                    },
                )))
            }
            Err(Stage5eCallbackAuthorityValidationError::Terminal(reason)) => {
                drop(b3c_receipt);
                Err(Stage5eCallbackAuthorityIssueBlocked::Terminal(
                    Stage5eCallbackAuthorityTerminalBlock { reason },
                ))
            }
        }
    }

    enum Stage5eCallbackAuthorityValidationError {
        Retryable(Stage5eCallbackAuthorityRetryableReason),
        Terminal(Stage5eCallbackAuthorityTerminalReason),
    }

    fn validate_callback_authority_preflight(
        preflight: Stage5eCallbackAuthorityPreflight<'_>,
        now: DateTime<Utc>,
    ) -> Result<Stage5eCallbackAuthorityApproved, Stage5eCallbackAuthorityValidationError> {
        if now < preflight.effective_observed_at {
            return Err(Stage5eCallbackAuthorityValidationError::Retryable(
                Stage5eCallbackAuthorityRetryableReason::ClockBeforeEffectiveObservation,
            ));
        }
        if preflight.accepted_bar_close_ts > now.timestamp() {
            return Err(Stage5eCallbackAuthorityValidationError::Retryable(
                Stage5eCallbackAuthorityRetryableReason::AcceptedBarObservedInFuture,
            ));
        }
        if now > preflight.effective_expires_at {
            return Err(Stage5eCallbackAuthorityValidationError::Terminal(
                Stage5eCallbackAuthorityTerminalReason::EvidenceExpired,
            ));
        }
        if preflight.effective_observed_at > preflight.effective_expires_at {
            return Err(Stage5eCallbackAuthorityValidationError::Terminal(
                Stage5eCallbackAuthorityTerminalReason::InvalidAuthorityChronology,
            ));
        }
        if !instrument_identity_is_complete(preflight.full_instrument_id) {
            return Err(Stage5eCallbackAuthorityValidationError::Terminal(
                Stage5eCallbackAuthorityTerminalReason::InstrumentIdentityMissing,
            ));
        }
        for (identity, reason) in [
            (
                preflight.accepted_semantic_bar_identity,
                Stage5eCallbackAuthorityTerminalReason::AcceptedSemanticBarIdentityMissing,
            ),
            (
                preflight.event_key_fingerprint,
                Stage5eCallbackAuthorityTerminalReason::EventKeyMissing,
            ),
            (
                preflight.continuation_binding_id,
                Stage5eCallbackAuthorityTerminalReason::ContinuationBindingMissing,
            ),
            (
                preflight.schedule_identity_fingerprint,
                Stage5eCallbackAuthorityTerminalReason::ScheduleIdentityMissing,
            ),
            (
                preflight.sequence_identity_fingerprint,
                Stage5eCallbackAuthorityTerminalReason::SequenceIdentityMissing,
            ),
        ] {
            if identity == [0; 32] {
                return Err(Stage5eCallbackAuthorityValidationError::Terminal(reason));
            }
        }
        let issued_at = now;
        let authority_expires_at = preflight.effective_expires_at;
        if issued_at > authority_expires_at {
            return Err(Stage5eCallbackAuthorityValidationError::Terminal(
                Stage5eCallbackAuthorityTerminalReason::InvalidAuthorityChronology,
            ));
        }
        let callback_authority_id = callback_authority_id(
            preflight.full_instrument_id,
            preflight.accepted_semantic_bar_identity,
            preflight.event_key_fingerprint,
            preflight.continuation_binding_id,
            preflight.sequence_identity_fingerprint,
            issued_at,
            authority_expires_at,
        );
        Ok(Stage5eCallbackAuthorityApproved {
            callback_authority_id,
            issued_at,
            effective_observed_at: preflight.effective_observed_at,
            authority_expires_at,
            accepted_bar_close_ts: preflight.accepted_bar_close_ts,
            full_instrument_id: preflight.full_instrument_id.clone(),
            accepted_semantic_bar_identity: preflight.accepted_semantic_bar_identity,
            event_key_fingerprint: preflight.event_key_fingerprint,
            continuation_binding_id: preflight.continuation_binding_id,
            sequence_identity_fingerprint: preflight.sequence_identity_fingerprint,
        })
    }

    fn instrument_identity_is_complete(instrument: &broker_core::InstrumentId) -> bool {
        if instrument.symbol.trim().is_empty()
            || instrument
                .venue_symbol
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return false;
        }
        if matches!(&instrument.exchange, broker_core::Exchange::Other(value) if value.trim().is_empty())
            || matches!(&instrument.market, broker_core::Market::Other(value) if value.trim().is_empty())
        {
            return false;
        }
        true
    }

    fn callback_authority_id(
        instrument: &broker_core::InstrumentId,
        accepted_semantic_bar_identity: [u8; 32],
        event_key_fingerprint: [u8; 32],
        continuation_binding_id: [u8; 32],
        sequence_identity_fingerprint: [u8; 32],
        issued_at: DateTime<Utc>,
        authority_expires_at: DateTime<Utc>,
    ) -> Stage5eCallbackAuthorityId {
        let mut encoder = CanonicalEncoder::new(AUTHORITY_DOMAIN);
        encoder.field(1, &canonical_instrument_bytes(instrument));
        encoder.field(2, &accepted_semantic_bar_identity);
        encoder.field(3, &event_key_fingerprint);
        encoder.field(4, &continuation_binding_id);
        encoder.field(5, &sequence_identity_fingerprint);
        encoder.field(6, &issued_at.timestamp_millis().to_be_bytes());
        encoder.field(7, &authority_expires_at.timestamp_millis().to_be_bytes());
        Stage5eCallbackAuthorityId(encoder.finish())
    }

    fn canonical_instrument_bytes(instrument: &broker_core::InstrumentId) -> Vec<u8> {
        let mut encoder = CanonicalEncoder::new(b"broker-neutral-instrument-id-v1");
        encoder.field(1, instrument.symbol.as_bytes());
        match instrument.venue_symbol.as_deref() {
            Some(value) => {
                encoder.field(2, &[1]);
                encoder.field(3, value.as_bytes());
            }
            None => encoder.field(2, &[0]),
        }
        match &instrument.exchange {
            broker_core::Exchange::Moex => encoder.field(4, b"moex"),
            broker_core::Exchange::Other(value) => {
                encoder.field(4, b"other");
                encoder.field(5, value.as_bytes());
            }
        }
        match &instrument.market {
            broker_core::Market::Futures => encoder.field(6, b"futures"),
            broker_core::Market::Options => encoder.field(6, b"options"),
            broker_core::Market::Stocks => encoder.field(6, b"stocks"),
            broker_core::Market::Currency => encoder.field(6, b"currency"),
            broker_core::Market::Funds => encoder.field(6, b"funds"),
            broker_core::Market::Other(value) => {
                encoder.field(6, b"other");
                encoder.field(7, value.as_bytes());
            }
        }
        encoder.finish().to_vec()
    }

    struct CanonicalEncoder {
        hasher: Sha256,
    }

    impl CanonicalEncoder {
        fn new(domain: &[u8]) -> Self {
            let mut hasher = Sha256::new();
            hasher.update(domain);
            Self { hasher }
        }

        fn field(&mut self, tag: u8, bytes: &[u8]) {
            self.hasher.update([tag]);
            self.hasher.update((bytes.len() as u64).to_be_bytes());
            self.hasher.update(bytes);
        }

        fn finish(self) -> [u8; 32] {
            self.hasher.finalize().into()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use chrono::TimeZone;

        fn instrument() -> broker_core::InstrumentId {
            broker_core::InstrumentId {
                symbol: "IMOEXF".to_string(),
                venue_symbol: Some("IMOEXF@RTSX".to_string()),
                exchange: broker_core::Exchange::Moex,
                market: broker_core::Market::Futures,
            }
        }

        #[test]
        fn callback_authority_id_is_sensitive_to_every_frozen_field() {
            let issued_at = Utc
                .with_ymd_and_hms(2026, 7, 24, 10, 20, 0)
                .single()
                .unwrap();
            let expires_at = issued_at + chrono::Duration::seconds(10);
            let base = callback_authority_id(
                &instrument(),
                [1; 32],
                [2; 32],
                [3; 32],
                [4; 32],
                issued_at,
                expires_at,
            )
            .0;
            let mut changed_instrument = instrument();
            changed_instrument.venue_symbol = Some("IMOEXF@OTHER".to_string());
            let variants = [
                callback_authority_id(
                    &changed_instrument,
                    [1; 32],
                    [2; 32],
                    [3; 32],
                    [4; 32],
                    issued_at,
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [9; 32],
                    [2; 32],
                    [3; 32],
                    [4; 32],
                    issued_at,
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [1; 32],
                    [9; 32],
                    [3; 32],
                    [4; 32],
                    issued_at,
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [1; 32],
                    [2; 32],
                    [9; 32],
                    [4; 32],
                    issued_at,
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [1; 32],
                    [2; 32],
                    [3; 32],
                    [9; 32],
                    issued_at,
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [1; 32],
                    [2; 32],
                    [3; 32],
                    [4; 32],
                    issued_at + chrono::Duration::milliseconds(1),
                    expires_at,
                )
                .0,
                callback_authority_id(
                    &instrument(),
                    [1; 32],
                    [2; 32],
                    [3; 32],
                    [4; 32],
                    issued_at,
                    expires_at + chrono::Duration::milliseconds(1),
                )
                .0,
            ];
            for variant in variants {
                assert_ne!(variant, base);
            }
        }

        #[test]
        fn callback_authority_instrument_identity_is_fail_closed() {
            assert!(instrument_identity_is_complete(&instrument()));
            let mut empty_symbol = instrument();
            empty_symbol.symbol.clear();
            assert!(!instrument_identity_is_complete(&empty_symbol));
            let mut empty_venue = instrument();
            empty_venue.venue_symbol = Some(String::new());
            assert!(!instrument_identity_is_complete(&empty_venue));
            let mut empty_exchange = instrument();
            empty_exchange.exchange = broker_core::Exchange::Other(String::new());
            assert!(!instrument_identity_is_complete(&empty_exchange));
            let mut empty_market = instrument();
            empty_market.market = broker_core::Market::Other(String::new());
            assert!(!instrument_identity_is_complete(&empty_market));
        }
    }
}
// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1

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
