use super::{
    wire::{check_bar, same_values},
    PrimaryError, PrimaryPolicy, SnapshotPresence, WireFrame,
};
use broker_core::{
    canonical_m1::aggregate_m1_window, event::Bar, HybridRuntimeBarEvent, HybridRuntimeBarOrigin,
    InstrumentId,
};
use broker_finam::sparse_m10::AdmittedClosedM1Snapshot;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};
mod calendar;
mod proof;
mod recovery;
mod restart;
pub use recovery::{
    AdmittedRecovery, RecoveryCompletion, RecoveryInput, RecoveryPlan, RecoveryReceipt,
    ResolvedRecovery,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PipelineState {
    Bootstrap,
    LiveReady,
    Recovering,
    Degraded,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExceptionReason {
    StartupPartial,
    SparseUnproven,
    BoundaryUnproven,
    SessionEnd,
}

#[derive(Debug)]
pub enum CanonicalStatus {
    Waiting,
    PendingProof {
        label_ms: i64,
        reason: ExceptionReason,
        may_request: bool,
    },
    AwaitingCommit,
    Finished,
    Ready(Box<AdmittedCanonicalM10>),
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counters {
    pub rx_rows: u64,
    pub valid_updates: u64,
    pub duplicates: u64,
    pub discarded_history: u64,
    pub committed: u64,
    pub recovery_committed: u64,
    pub restart_committed: u64,
}

#[derive(Debug, Serialize)]
pub struct CanonicalReceipt {
    pub config_sha256: String,
    pub policy_sha256: String,
    pub origin: &'static str,
    pub reason: Option<ExceptionReason>,
    pub label_ms: i64,
    pub close_ms: i64,
    pub generation: u64,
    pub subscribed_at_ms: i64,
    pub last_component_received_at_ms: i64,
    pub boundary_received_at_ms: Option<i64>,
    pub proof_received_at_ms: Option<i64>,
    pub rest_snapshot_sha256: Option<String>,
    pub canonical_ready_at_ms: i64,
    pub minute_bitmap: u16,
    pub canonical_digest: String,
    pub actual_m1: Vec<Bar>,
    pub source_frame_sha256: Vec<String>,
}

/// A single-use *data* admission. It does not prove a model callback/feedback
/// commit, nor authorize anything on the real account. No Clone/Deserialize.
///
/// ```compile_fail
/// use finam_signal_shadow::primary::AdmittedCanonicalM10;
/// let forged: AdmittedCanonicalM10 = serde_json::from_str("{}").unwrap();
/// ```
///
/// ```compile_fail
/// use finam_signal_shadow::primary::AdmittedCanonicalM10;
/// fn duplicate(input: &AdmittedCanonicalM10) -> AdmittedCanonicalM10 {
///     (*input).clone()
/// }
/// ```
#[derive(Debug)]
pub struct AdmittedCanonicalM10 {
    owner: Arc<()>,
    receipt: CanonicalReceipt,
    model_bar: HybridRuntimeBarEvent,
    observed_bo: bool,
    pub(super) rest_evidence: Option<broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1>,
}

impl AdmittedCanonicalM10 {
    pub fn receipt(&self) -> &CanonicalReceipt {
        &self.receipt
    }
    pub fn model_bar(&self) -> &HybridRuntimeBarEvent {
        &self.model_bar
    }

    pub fn received_at_ms(&self) -> i64 {
        self.receipt
            .proof_received_at_ms
            .or(self.receipt.boundary_received_at_ms)
            .expect("private admission constructors always retain a network receipt")
    }

    pub fn provenance_sha256(&self) -> Result<String, PrimaryError> {
        let body = serde_json::to_vec(&self.receipt).map_err(|_| PrimaryError::Numeric)?;
        Ok(crate::seed::digest(&body))
    }

    pub fn rest_evidence(&self) -> Option<&broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1> {
        self.rest_evidence.as_ref()
    }

    pub fn compare_rest(&self, source: &AdmittedClosedM1Snapshot) -> Result<(), PrimaryError> {
        let actual: Vec<_> = source.bars().collect();
        if source.evidence().start.timestamp_millis() != self.receipt.label_ms
            || source.evidence().end.timestamp_millis() != self.receipt.close_ms
            || actual.len() != self.receipt.actual_m1.len()
            || self
                .receipt
                .actual_m1
                .iter()
                .zip(actual)
                .any(|(a, b)| !super::revisions::compatible(a, b, self.observed_bo))
        {
            return Err(PrimaryError::CanonicalConflict);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Version {
    bar: Bar,
    generation: u64,
    first_received_ms: i64,
    last_changed_ms: i64,
    frame_sha256: String,
}

/// One instrument/frontier, independent of number of future runtime consumers.
/// Until runner/session integration is complete this core is OFFLINE ONLY.
pub struct PrimaryProcessor {
    instrument: InstrumentId,
    pub(super) policy: PrimaryPolicy,
    pub(super) source_changes: Vec<super::revisions::SourceChange>,
    policy_sha256: String,
    config_sha256: String,
    clock_uncertainty_ms: i64,
    required: Vec<i64>,
    next: usize,
    generation: u64,
    subscribed_at_ms: i64,
    last_clock_ms: i64,
    /// Last readiness assessment, not the later time of Update/evidence IO.
    last_readiness_evaluation: Option<(i64, i64)>,
    state: PipelineState,
    fresh_seen: bool,
    bootstrap_rows: usize,
    bootstrap_bytes: usize,
    reference: BTreeMap<i64, Bar>,
    reference_hashes: Vec<String>,
    cache: BTreeMap<i64, Version>,
    committed: BTreeMap<i64, Bar>,
    in_flight: Option<Arc<()>>,
    gap_unresolved: bool,
    gap_owner: Option<Arc<()>>,
    last_resolution: Option<Arc<()>>,
    recovery_pending: Option<recovery::Pending>,
    run_deadline_ms: i64,
    counters: Counters,
    last_live_data: Option<super::LiveDataStamp>,
    physical_owner: Option<Arc<()>>,
    calendar: Option<super::calendar::AdmittedCalendar>,
    calendar_subscribed: bool,
    session_end_committed: bool,
    #[cfg(test)]
    fixture_unbound: bool,
}

impl PrimaryProcessor {
    pub(crate) fn rest_nonblocking(&self) -> bool {
        self.policy.rest_nonblocking()
    }

    pub(crate) fn assess_rest(
        &self,
        source: &AdmittedClosedM1Snapshot,
    ) -> super::rest_observation::Assessment {
        let label = source.evidence().start.timestamp_millis();
        let close = label + 600000;
        let committed = self.next_label_ms().map_or(true, |next| label < next);
        let known = if committed {
            self.committed
                .range(label..close)
                .map(|(_, bar)| bar.clone())
                .collect()
        } else {
            self.cache
                .range(label..close)
                .map(|(_, v)| v.bar.clone())
                .collect()
        };
        super::rest_observation::assess(
            label,
            if committed {
                "COMMITTED_MODEL_INPUT"
            } else {
                "PROVISIONAL_WS"
            },
            known,
            source.bars().cloned().collect(),
        )
    }

    pub(crate) fn validate_known_rest_inventory(
        &mut self,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<(), PrimaryError> {
        let assessment = self.assess_rest(source);
        if let Some(missing) = assessment.missing_known_m1_ms.first() {
            let old = assessment
                .expected_m1
                .iter()
                .find(|b| b.open_ts.timestamp_millis() == *missing)
                .unwrap();
            self.note_change(
                "REST_KNOWN_INVENTORY",
                Some(old),
                None,
                Some(source.sha256()),
            )?;
            return self.hold(PrimaryError::CanonicalConflict);
        }
        Ok(())
    }

    pub(crate) fn rest_precommit_ws_first(&self) -> bool {
        self.policy.rest_precommit_ws_first()
    }

    pub(super) fn rest_proof_ws_witness(
        &self,
        bar: &Bar,
    ) -> Option<super::revisions::WsVersionWitness> {
        let version = self.cache.get(&bar.open_ts.timestamp_millis())?;
        if !same_values(&version.bar, bar) {
            return None;
        }
        Some(super::revisions::WsVersionWitness {
            generation: version.generation,
            first_received_at_ms: version.first_received_ms,
            last_changed_at_ms: version.last_changed_ms,
            frame_sha256: version.frame_sha256.clone(),
        })
    }

    pub fn from_config(
        config: &super::WsPrimaryConfig,
        instrument: InstrumentId,
        required: Vec<i64>,
        generation: u64,
        subscribed_at_ms: i64,
    ) -> Result<Self, PrimaryError> {
        let config_sha256 = config.fingerprint()?;
        let mut processor = Self::new(
            instrument,
            config.primary.clone(),
            required,
            generation,
            subscribed_at_ms,
        )?;
        processor.config_sha256 = config_sha256;
        processor.clock_uncertainty_ms = config.base.clock_uncertainty_ms as i64;
        processor.run_deadline_ms = config.base.run_deadline_utc_ms;
        Ok(processor)
    }
    #[cfg(test)]
    pub(crate) fn from_fixture(
        config: &super::WsPrimaryConfig,
        instrument: InstrumentId,
        required: Vec<i64>,
        generation: u64,
        subscribed_at_ms: i64,
    ) -> Result<Self, PrimaryError> {
        let mut value =
            Self::from_config(config, instrument, required, generation, subscribed_at_ms)?;
        value.fixture_unbound = true;
        Ok(value)
    }
    fn physical_bound(&self) -> bool {
        #[cfg(test)]
        if self.fixture_unbound {
            return true;
        }
        self.physical_owner.is_some()
    }
    pub(crate) fn bind_subscription(
        &mut self,
        source: &crate::socket::primary::Subscription,
        now: i64,
    ) -> Result<(), PrimaryError> {
        if !self.calendar_subscribed {
            if let Some(calendar) = &self.calendar {
                calendar
                    .validate_entry(source.sent_at_ms())
                    .and_then(|_| calendar.validate_entry(now))
                    .map_err(|_| PrimaryError::EntryWindow)?;
            }
        }
        if source.generation() != self.generation
            || self.physical_owner.is_some()
            || self.fresh_seen
            || self.bootstrap_rows != 0
        {
            return Err(PrimaryError::Generation);
        }
        if source.sent_at_ms() > now {
            return self.hold(PrimaryError::Clock);
        }
        self.clock(now)?;
        self.subscribed_at_ms = source.sent_at_ms();
        self.physical_owner = Some(source.owner.clone());
        self.calendar_subscribed = true;
        Ok(())
    }

    /// required is independently fixed model-calendar inventory, never derived
    /// from WS presence. subscribed_at is a physical-subscription receipt time.
    fn new(
        instrument: InstrumentId,
        policy: PrimaryPolicy,
        required: Vec<i64>,
        generation: u64,
        subscribed_at_ms: i64,
    ) -> Result<Self, PrimaryError> {
        policy.validate()?;
        if generation == 0
            || subscribed_at_ms < 0
            || required.is_empty()
            || required.len() > 4096
            || required.iter().any(|t| *t < 0 || t % 600000 != 0)
            || required.iter().any(|t| {
                DateTime::<Utc>::from_timestamp_millis(*t)
                    .and_then(|v| v.checked_add_signed(chrono::Duration::minutes(10)))
                    .is_none()
            })
            || DateTime::<Utc>::from_timestamp_millis(subscribed_at_ms).is_none()
            || required.windows(2).any(|w| w[0] >= w[1])
            || instrument.venue_symbol.as_deref() != Some("IMOEXF@RTSX")
            || instrument.market != broker_core::Market::Futures
        {
            return Err(PrimaryError::Configuration);
        }
        let policy_sha256 = policy.fingerprint()?;
        Ok(Self {
            instrument,
            policy,
            source_changes: Vec::new(),
            policy_sha256,
            config_sha256: String::new(),
            clock_uncertainty_ms: 100,
            required,
            next: 0,
            generation,
            subscribed_at_ms,
            last_clock_ms: subscribed_at_ms,
            last_readiness_evaluation: None,
            state: PipelineState::Bootstrap,
            fresh_seen: false,
            bootstrap_rows: 0,
            bootstrap_bytes: 0,
            reference: BTreeMap::new(),
            reference_hashes: Vec::new(),
            cache: BTreeMap::new(),
            committed: BTreeMap::new(),
            in_flight: None,
            gap_unresolved: false,
            gap_owner: None,
            last_resolution: None,
            recovery_pending: None,
            run_deadline_ms: i64::MAX,
            counters: Counters::default(),
            last_live_data: None,
            physical_owner: None,
            calendar: None,
            calendar_subscribed: false,
            session_end_committed: false,
            #[cfg(test)]
            fixture_unbound: false,
        })
    }

    pub fn state(&self) -> PipelineState {
        self.state
    }
    pub fn config_sha256(&self) -> &str {
        &self.config_sha256
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn subscription_data_ready(&self) -> bool {
        self.fresh_seen
    }
    pub(crate) fn proof_deadline_ms(&self, label: i64, now: i64) -> i64 {
        (now + self.policy.proof_timeout_ms as i64)
            .min(if self.policy.observed_bo() {
                label + 602000 - self.clock_uncertainty_ms
            } else {
                i64::MAX
            })
            .min(label + 610000 - self.clock_uncertainty_ms)
            .min(self.run_deadline_ms)
            .min(
                self.calendar
                    .as_ref()
                    .map_or(i64::MAX, |c| c.eod_check_at_ms()),
            )
    }
    pub fn counters(&self) -> &Counters {
        &self.counters
    }
    pub fn next_label_ms(&self) -> Option<i64> {
        self.required.get(self.next).copied()
    }
    pub fn gap_unresolved(&self) -> bool {
        self.gap_unresolved
    }
    pub fn cached_m1(&self) -> usize {
        self.cache.len()
    }
    pub fn reference_hashes(&self) -> &[String] {
        &self.reference_hashes
    }
    pub fn last_live_data(&self) -> Option<super::LiveDataStamp> {
        self.last_live_data.clone()
    }

    fn clock(&mut self, now_ms: i64) -> Result<(), PrimaryError> {
        if self.state == PipelineState::Hold {
            return Err(PrimaryError::Held);
        }
        if now_ms < self.last_clock_ms {
            self.state = PipelineState::Hold;
            return Err(PrimaryError::Clock);
        }
        self.last_clock_ms = now_ms;
        Ok(())
    }

    /// This only accepts the existing source-admitted REST capability, never
    /// arbitrary M10 seed data or caller-supplied normalized rows.
    pub fn add_reference(&mut self, source: &AdmittedClosedM1Snapshot) -> Result<(), PrimaryError> {
        if self.state == PipelineState::Hold {
            return Err(PrimaryError::Held);
        }
        let mut candidate = self.reference.clone();
        for bar in source.bars() {
            if bar.instrument != self.instrument {
                return Err(PrimaryError::Identity);
            }
            let key = bar.open_ts.timestamp_millis();
            if let Some(old) = candidate.get(&key) {
                self.note_change(
                    "REST_REFERENCE",
                    Some(old),
                    Some(bar),
                    Some(source.sha256()),
                )?;
                if !super::revisions::compatible(old, bar, self.policy.observed_bo()) {
                    self.state = PipelineState::Hold;
                    return Err(PrimaryError::ReferenceConflict);
                }
            }
            candidate.insert(key, bar.clone());
        }
        if candidate.len() > self.policy.max_cache_m1 {
            return Err(PrimaryError::CacheBudget);
        }
        if !self
            .reference_hashes
            .iter()
            .any(|hash| hash == source.sha256())
        {
            if self.reference_hashes.len() >= self.policy.max_cache_m1 {
                return Err(PrimaryError::CacheBudget);
            }
            self.reference_hashes.push(source.sha256().to_owned());
        }
        self.reference = candidate;
        Ok(())
    }

    /// The complete mixed frame is checked against temporary state before any
    /// cache/readiness/counter effect. Missing proof is retryable only after an
    /// independently admitted bounded reference fetch; it is NOT live readiness.
    pub fn ingest(&mut self, frame: WireFrame, processed_at_ms: i64) -> Result<(), PrimaryError> {
        // A fenced old owner cannot change even the new generation's fault state.
        if frame.generation != self.generation {
            return Err(PrimaryError::Generation);
        }
        if !self.physical_bound() {
            return Err(PrimaryError::Generation);
        }
        if self.physical_owner.as_ref().is_some_and(|owner| {
            frame
                .owner
                .as_ref()
                .map_or(true, |candidate| !Arc::ptr_eq(owner, candidate))
        }) {
            return Err(PrimaryError::Generation);
        }
        self.clock(processed_at_ms)?;
        if processed_at_ms < frame.received_at_ms {
            return self.hold(PrimaryError::Clock);
        }
        if processed_at_ms - frame.received_at_ms > self.policy.max_ingress_age_ms as i64 {
            self.break_continuity();
            return Err(PrimaryError::Continuity);
        }
        let result = self.ingest_checked(frame);
        if result.is_err() && result != Err(PrimaryError::ReferenceUnproven) {
            self.state = PipelineState::Hold;
        }
        result
    }

    /// A frame already dequeued on time may wait for mandatory reference IO.
    /// After 2s ONLY an entirely verified history frame can be discarded. A
    /// mixed/live frame never gains a new receive time or freshness exemption.
    pub(crate) fn resume_bootstrap_frame(
        &mut self,
        frame: WireFrame,
        now: i64,
    ) -> Result<(), PrimaryError> {
        if frame.generation != self.generation {
            return Err(PrimaryError::Generation);
        }
        if !self.physical_bound() {
            return Err(PrimaryError::Generation);
        }
        if self.physical_owner.as_ref().is_some_and(|owner| {
            frame
                .owner
                .as_ref()
                .map_or(true, |candidate| !Arc::ptr_eq(owner, candidate))
        }) {
            return Err(PrimaryError::Generation);
        }
        if now < frame.received_at_ms {
            return self.hold(PrimaryError::Clock);
        }
        if now - frame.received_at_ms <= self.policy.max_ingress_age_ms as i64 {
            return self.ingest(frame, now);
        }
        self.clock(now)?;
        if self.fresh_seen {
            return self.hold(PrimaryError::UnexpectedHistory);
        }
        let history_only = frame.rows.iter().all(|row| match row.presence {
            SnapshotPresence::ExplicitTrue => true,
            SnapshotPresence::Missing => {
                row.bar.close_ts.timestamp_millis() <= frame.received_at_ms
                    && self
                        .reference
                        .get(&row.bar.open_ts.timestamp_millis())
                        .is_some_and(|old| {
                            super::revisions::compatible(old, &row.bar, self.policy.observed_bo())
                        })
            }
            SnapshotPresence::ExplicitFalse => false,
        });
        if !history_only {
            self.break_continuity();
            return Err(PrimaryError::Continuity);
        }
        let result = self.ingest_checked(frame);
        if result.is_err() {
            self.state = PipelineState::Hold;
        }
        result
    }

    pub(super) fn hold<T>(&mut self, error: PrimaryError) -> Result<T, PrimaryError> {
        self.state = PipelineState::Hold;
        Err(error)
    }

    fn ingest_checked(&mut self, frame: WireFrame) -> Result<(), PrimaryError> {
        let mut cache = self.cache.clone();
        let mut counters = self.counters.clone();
        let mut history_rows = 0;
        let mut has_fresh_update = false;
        for row in &frame.rows {
            let bar = &row.bar;
            check_bar(bar, frame.received_at_ms)?;
            if bar.instrument != self.instrument {
                return Err(PrimaryError::Identity);
            }
            let key = bar.open_ts.timestamp_millis();
            if let Some(old) = self.committed.get(&key).cloned() {
                if !same_values(&old, bar) {
                    self.note_change(
                        "WS_AFTER_COMMIT",
                        Some(&old),
                        Some(bar),
                        Some(&frame.sha256),
                    )?;
                }
                if !super::revisions::compatible(&old, bar, self.policy.observed_bo()) {
                    return Err(PrimaryError::CanonicalConflict);
                }
                if row.presence == SnapshotPresence::ExplicitTrue {
                    if self.fresh_seen {
                        return Err(PrimaryError::UnexpectedHistory);
                    }
                    history_rows += 1;
                }
                counters.duplicates += 1;
                continue;
            }
            let stale = frame.received_at_ms - bar.close_ts.timestamp_millis() > 60000;
            let reference = self.reference.get(&key).cloned();
            let history = match row.presence {
                SnapshotPresence::ExplicitTrue => {
                    if self.fresh_seen {
                        return Err(PrimaryError::UnexpectedHistory);
                    }
                    true
                }
                SnapshotPresence::Missing
                    if !self.fresh_seen
                        && bar.close_ts.timestamp_millis() <= frame.received_at_ms
                        && reference.is_some() =>
                {
                    true
                }
                SnapshotPresence::Missing
                    if !self.fresh_seen
                        && bar.close_ts.timestamp_millis() <= frame.received_at_ms =>
                {
                    return Err(PrimaryError::ReferenceUnproven)
                }
                _ if stale => return Err(PrimaryError::Timestamp),
                _ => false,
            };
            if history {
                if let Some(old) = reference {
                    self.note_change(
                        "WS_BOOTSTRAP_REFERENCE",
                        Some(&old),
                        Some(bar),
                        Some(&frame.sha256),
                    )?;
                    if !super::revisions::compatible(&old, bar, self.policy.observed_bo()) {
                        return Err(PrimaryError::ReferenceConflict);
                    }
                }
                history_rows += 1;
                continue;
            }
            if let Some(old) = cache.get(&key) {
                if same_values(&old.bar, bar) && old.generation == self.generation {
                    counters.duplicates += 1;
                    continue;
                }
                if self.in_flight.is_some()
                    && self
                        .next_label_ms()
                        .is_some_and(|t| key >= t && key < t + 600000)
                {
                    self.note_change(
                        "WS_IN_FLIGHT",
                        Some(&old.bar),
                        Some(bar),
                        Some(&frame.sha256),
                    )?;
                    if self.policy.observed_bo() && super::revisions::same_prices(&old.bar, bar) {
                        counters.duplicates += 1;
                        continue;
                    }
                    return Err(PrimaryError::CanonicalConflict);
                }
                if old.bar.open != bar.open
                    || bar.high < old.bar.high
                    || bar.low > old.bar.low
                    || (bar.volume < old.bar.volume
                        && !(self.policy.observed_bo()
                            && super::revisions::same_prices(&old.bar, bar)))
                {
                    self.note_change(
                        "WS_PROVISIONAL",
                        Some(&old.bar),
                        Some(bar),
                        Some(&frame.sha256),
                    )?;
                    return Err(PrimaryError::CanonicalConflict);
                }
                if bar.volume < old.bar.volume {
                    self.note_change(
                        "WS_PROVISIONAL",
                        Some(&old.bar),
                        Some(bar),
                        Some(&frame.sha256),
                    )?;
                }
            }
            let first_received_ms = cache
                .get(&key)
                .filter(|old| old.generation == self.generation)
                .map_or(frame.received_at_ms, |old| old.first_received_ms);
            cache.insert(
                key,
                Version {
                    bar: bar.clone(),
                    generation: self.generation,
                    first_received_ms,
                    last_changed_ms: frame.received_at_ms,
                    frame_sha256: frame.sha256.clone(),
                },
            );
            counters.valid_updates += 1;
            has_fresh_update = true;
        }
        if cache.len() > self.policy.max_cache_m1 {
            return Err(PrimaryError::CacheBudget);
        }
        let bytes = if history_rows > 0 {
            frame.wire_bytes
        } else {
            0
        };
        if self
            .bootstrap_rows
            .checked_add(history_rows)
            .map_or(true, |n| n > self.policy.bootstrap_max_rows)
            || self
                .bootstrap_bytes
                .checked_add(bytes)
                .map_or(true, |n| n > self.policy.bootstrap_max_bytes)
        {
            return Err(PrimaryError::BootstrapBudget);
        }
        counters.rx_rows += frame.rows.len() as u64;
        counters.discarded_history += history_rows as u64;
        self.bootstrap_rows += history_rows;
        self.bootstrap_bytes += bytes;
        self.cache = cache;
        self.counters = counters;
        if has_fresh_update {
            self.last_live_data = Some(super::LiveDataStamp {
                generation: self.generation,
                received_at_ms: frame.received_at_ms,
                received_mono: frame.received_mono,
                owner: frame.owner.clone(),
            });
            self.fresh_seen = true;
            if !self.gap_unresolved {
                self.state = PipelineState::LiveReady;
            }
        }
        Ok(())
    }

    /// Synchronous latch BEFORE the future runner awaits reconnect/proof IO.
    /// Fresh DATA cannot clear this canonical gap. Only a source-bound completed
    /// historical batch plus explicit readiness resolution may clear it.
    pub fn break_continuity(&mut self) {
        self.gap_unresolved = true;
        self.last_resolution = None;
        // Every new fault invalidates any staged proof, including a fault that
        // occurs during an already unresolved resync. No old completion clears it.
        self.gap_owner = Some(Arc::new(()));
        self.recovery_pending = None;
        if self.state != PipelineState::Hold {
            self.state = PipelineState::Recovering;
        }
    }

    pub fn begin_generation(
        &mut self,
        generation: u64,
        subscribed_at_ms: i64,
    ) -> Result<(), PrimaryError> {
        if generation <= self.generation || self.in_flight.is_some() {
            return Err(PrimaryError::Generation);
        }
        self.clock(subscribed_at_ms)?;
        self.break_continuity();
        self.generation = generation;
        self.subscribed_at_ms = subscribed_at_ms;
        self.fresh_seen = false;
        self.last_live_data = None;
        self.physical_owner = None;
        self.bootstrap_rows = 0;
        self.bootstrap_bytes = 0;
        Ok(())
    }

    /// Keep due actions until readiness actually assesses them, even if IO
    /// advances time before Update. A past/equal result asks for one prompt
    /// tick; that tick consumes elapsed thresholds, including when proof waits.
    pub(crate) fn next_action_ms(&self) -> Option<i64> {
        let label = self.next_label_ms()?;
        let close = label.checked_add(600000)?;
        let assessed_at = self
            .last_readiness_evaluation
            .filter(|(evaluated_label, _)| *evaluated_label == label)
            .map_or(label, |(_, at)| at);
        let boundary = self
            .cache
            .range(close..)
            .filter(|(_, v)| v.generation == self.generation)
            .map(|(_, v)| v.first_received_ms)
            .min();
        [
            Some(close + self.policy.close_grace_ms as i64),
            Some(close + self.policy.proof_wait_ms()),
            self.policy
                .observed_bo()
                .then_some(close + 2000 - self.clock_uncertainty_ms + 1),
            Some(close + 10000 - self.clock_uncertainty_ms + 1),
            boundary.map(|b| close.max(b) + self.policy.close_grace_ms as i64),
        ]
        .into_iter()
        .flatten()
        .filter(|at| *at > assessed_at)
        .min()
    }

    pub fn prepare(&mut self, now_ms: i64) -> Result<CanonicalStatus, PrimaryError> {
        if !self.physical_bound() {
            return Err(PrimaryError::Generation);
        }
        self.clock(now_ms)?;
        if let Some(label) = self.next_label_ms() {
            self.last_readiness_evaluation = Some((label, now_ms));
        }
        if self.in_flight.is_some() {
            return Ok(CanonicalStatus::AwaitingCommit);
        }
        let Some(label) = self.next_label_ms() else {
            return Ok(CanonicalStatus::Finished);
        };
        if self
            .calendar
            .as_ref()
            .is_some_and(|c| now_ms >= c.eod_check_at_ms())
        {
            return self.hold(PrimaryError::ReferenceUnproven);
        }
        let close = label.checked_add(600000).ok_or(PrimaryError::Timestamp)?;
        if now_ms < close {
            return Ok(CanonicalStatus::Waiting);
        }
        if now_ms
            .checked_sub(close)
            .and_then(|d| d.checked_add(self.clock_uncertainty_ms))
            .map_or(true, |d| d > 10000)
        {
            if !self.gap_unresolved {
                self.break_continuity();
            }
            self.state = PipelineState::Degraded;
            return Ok(CanonicalStatus::PendingProof {
                label_ms: label,
                reason: ExceptionReason::BoundaryUnproven,
                may_request: true,
            });
        }
        if self.policy.observed_bo() && now_ms - close + self.clock_uncertainty_ms > 2000 {
            self.break_continuity();
            return Ok(CanonicalStatus::PendingProof {
                label_ms: label,
                reason: ExceptionReason::BoundaryUnproven,
                may_request: false,
            });
        }
        if self.gap_unresolved || !self.fresh_seen {
            return Ok(CanonicalStatus::PendingProof {
                label_ms: label,
                reason: ExceptionReason::BoundaryUnproven,
                may_request: now_ms >= close + self.policy.proof_wait_ms(),
            });
        }
        if self.session_end_label() == Some(label) {
            return Ok(CanonicalStatus::PendingProof {
                label_ms: label,
                reason: ExceptionReason::SessionEnd,
                may_request: now_ms >= close + self.policy.proof_wait_ms(),
            });
        }
        let entries: Vec<_> = self.cache.range(label..close).map(|(_, v)| v).collect();
        let boundary = self
            .cache
            .range(close..)
            .filter(|(_, v)| v.generation == self.generation)
            .map(|(_, v)| v.first_received_ms)
            .min();
        let reason = if self.subscribed_at_ms > label {
            Some(ExceptionReason::StartupPartial)
        } else if entries.len() != 10 || entries.iter().any(|v| v.generation != self.generation) {
            Some(ExceptionReason::SparseUnproven)
        } else if boundary.is_none() {
            Some(ExceptionReason::BoundaryUnproven)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(CanonicalStatus::PendingProof {
                label_ms: label,
                reason,
                may_request: now_ms >= close + self.policy.proof_wait_ms(),
            });
        }
        let seal_anchor = close.max(boundary.ok_or(PrimaryError::Continuity)?);
        if now_ms < seal_anchor + self.policy.close_grace_ms as i64 {
            return Ok(CanonicalStatus::Waiting);
        }
        let rows: Vec<_> = entries.iter().map(|v| v.bar.clone()).collect();
        let open = DateTime::<Utc>::from_timestamp_millis(label).ok_or(PrimaryError::Timestamp)?;
        let values = aggregate_m1_window(&self.instrument, open, &rows)
            .map_err(|_| PrimaryError::Numeric)?;
        if values.minute_bitmap != 0b1111111111 {
            return self.hold(PrimaryError::Continuity);
        }
        let model_bar = runtime_bar(&values.provisional_bar)?;
        let canonical_digest =
            crate::input::canonical_digest(&model_bar).map_err(|_| PrimaryError::Numeric)?;
        let owner = Arc::new(());
        let receipt = CanonicalReceipt {
            config_sha256: self.config_sha256.clone(),
            policy_sha256: self.policy_sha256.clone(),
            origin: "WS_LIVE",
            reason: None,
            label_ms: label,
            close_ms: close,
            generation: self.generation,
            subscribed_at_ms: self.subscribed_at_ms,
            last_component_received_at_ms: entries
                .iter()
                .map(|v| v.last_changed_ms)
                .max()
                .ok_or(PrimaryError::Continuity)?,
            boundary_received_at_ms: boundary,
            proof_received_at_ms: None,
            rest_snapshot_sha256: None,
            canonical_ready_at_ms: now_ms,
            minute_bitmap: values.minute_bitmap,
            canonical_digest,
            actual_m1: values.actual_m1,
            source_frame_sha256: entries.iter().map(|v| v.frame_sha256.clone()).collect(),
        };
        self.in_flight = Some(owner.clone());
        Ok(CanonicalStatus::Ready(Box::new(AdmittedCanonicalM10 {
            owner,
            receipt,
            model_bar,
            observed_bo: self.policy.observed_bo(),
            rest_evidence: None,
        })))
    }

    /// Must be called by the future single session writer only AFTER durable
    /// model+feedback+commit publication. Data admission itself advances nothing.
    pub fn committed(
        &mut self,
        input: AdmittedCanonicalM10,
        published_at_ms: i64,
    ) -> Result<(), PrimaryError> {
        self.clock(published_at_ms)?;
        if self.gap_unresolved
            || self
                .in_flight
                .as_ref()
                .map_or(true, |owner| !Arc::ptr_eq(owner, &input.owner))
            || self.next_label_ms() != Some(input.receipt.label_ms)
            || input.receipt.generation != self.generation
            || published_at_ms < input.receipt.canonical_ready_at_ms
            || published_at_ms - input.receipt.close_ms + self.clock_uncertainty_ms > 10000
            || (self.policy.observed_bo()
                && published_at_ms - input.receipt.close_ms + self.clock_uncertainty_ms > 2000)
        {
            return self.hold(PrimaryError::Continuity);
        }
        if input.receipt.reason == Some(ExceptionReason::SessionEnd)
            && self.session_end_label() == Some(input.receipt.label_ms)
        {
            self.session_end_committed = true;
        }
        for bar in input.receipt.actual_m1 {
            self.committed.insert(bar.open_ts.timestamp_millis(), bar);
        }
        self.next += 1;
        self.counters.committed += 1;
        self.in_flight = None;
        Ok(())
    }
}

pub(super) fn runtime_bar(bar: &Bar) -> Result<HybridRuntimeBarEvent, PrimaryError> {
    let number = |value: rust_decimal::Decimal| -> Result<f64, PrimaryError> {
        let value: f64 = value
            .to_string()
            .parse()
            .map_err(|_| PrimaryError::Numeric)?;
        if !value.is_finite() {
            return Err(PrimaryError::Numeric);
        }
        Ok(value)
    };
    Ok(HybridRuntimeBarEvent {
        instrument: bar.instrument.clone(),
        close_time_utc: bar.open_ts.timestamp(),
        timeframe_sec: 600,
        is_final: true,
        origin: HybridRuntimeBarOrigin::Live,
        open: number(bar.open)?,
        high: number(bar.high)?,
        low: number(bar.low)?,
        close: number(bar.close)?,
        volume: number(bar.volume)?,
    })
}
