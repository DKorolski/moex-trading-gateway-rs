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
        let mut encoder = CanonicalEncoder::new(b"stage5e-b3-stage4-schedule-v2");
        encoder.field(1, &report.schema_version.to_be_bytes());
        encode_instrument(&mut encoder, &report.target_instrument);
        encoder.field(2, &report.checked_ts.timestamp_millis().to_be_bytes());
        encoder.field(3, &observed_at.timestamp_millis().to_be_bytes());
        encoder.field(4, &expires_at.timestamp_millis().to_be_bytes());
        encoder.field(5, &age_ms.to_be_bytes());
        Ok(AcceptedStage4ScheduleEvidence {
            instrument: report.target_instrument.clone(),
            observed_at: LifecycleInstant(observed_at),
            expires_at: LifecycleInstant(expires_at),
            identity: ScheduleFingerprint(encoder.finish()),
        })
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

    fn map_trusted_schedule_window(
        validated: ValidatedNormalizedInstrumentScheduleSnapshot,
        registry: AcceptedInstrumentRegistryEvidence,
        stage4: AcceptedStage4ScheduleEvidence,
        requested_bar_close: MarketBarCloseTime,
        lifecycle_now: LifecycleInstant,
    ) -> Result<Stage5eScheduleWindowEvidence, ScheduleWindowMappingError> {
        if lifecycle_now.0 > stage4.expires_at.0 {
            return Err(ScheduleWindowMappingError::Stage4Expired);
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
            .ok_or(ScheduleWindowMappingError::NoTradableOpenForRequestedBar)?;
        let fingerprint = deterministic_fingerprint(&validated, &registry, &stage4, selected);
        Ok(Stage5eScheduleWindowEvidence {
            instrument: validated.snapshot.instrument,
            broker_symbol: validated.snapshot.broker_symbol,
            venue_mic: validated.snapshot.venue_mic,
            board: validated.snapshot.board,
            trading_day: validated.snapshot.trading_day,
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
        })
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use broker_core::{Exchange, InstrumentId, Market};

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
                observed_at: LifecycleInstant(now),
                expires_at: LifecycleInstant(now + chrono::Duration::seconds(10)),
                identity: ScheduleFingerprint([7; 32]),
            }
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
            map_trusted_schedule_window(
                validated,
                accepted_registry,
                stage4(now, target),
                MarketBarCloseTime(open_from),
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
            let validated = validated(now);
            let accepted_registry =
                accept_instrument_registry_evidence(&validated, registry(&validated)).unwrap();
            let evidence = map_trusted_schedule_window(
                validated,
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
        }

        #[test]
        fn fingerprint_covers_full_identity_and_mapping_rechecks_expiry() {
            let now = Utc::now();
            let validated_a = validated(now);
            let registry_a =
                accept_instrument_registry_evidence(&validated_a, registry(&validated_a)).unwrap();
            let first = map_trusted_schedule_window(
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
            let second = map_trusted_schedule_window(
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
                map_trusted_schedule_window(
                    stage4_expired_validated,
                    accepted_registry,
                    AcceptedStage4ScheduleEvidence {
                        instrument: instrument(),
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
                map_trusted_schedule_window(
                    snapshot_expired_validated,
                    accepted_registry,
                    AcceptedStage4ScheduleEvidence {
                        instrument: instrument(),
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
            let window = map_trusted_schedule_window(
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

    // STAGE5E-B3C-EVIDENCE-BEGIN: private-no-io-v1
    // These receipts deliberately stay module-private.  They are evidence
    // producers only; a later reviewed bridge is required to consume b3b.
    mod b3c_evidence {
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
