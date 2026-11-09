//! Single canonical writer and nonblocking REST-role dispatch. No socket/CLI
//! entry is enabled until the remaining WS-primary implementation is complete.
use crate::{
    primary::{CanonicalStatus, ExceptionReason, PrimaryError, PrimaryProcessor, WireFrame},
    session::{ObservedSession, SessionError},
};
use broker_finam::sparse_m10::{AdmittedClosedM1Snapshot, ClosedM1RequestPlan};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub mod bootstrap;
pub mod http;
pub mod live;
pub mod reconnect;
mod runtimes;
mod writer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RestFailure {
    Unavailable,
    Timeout,
    Authentication,
    Integrity,
    Cancelled,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AuditStatus {
    CancelledForStop,
    NotScheduled,
    Pending,
    Match,
    VolumeRevision,
    Unavailable,
    Conflict,
    EvidenceOnly,
    EvidenceUnavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoordinatorError {
    Binding,
    Held,
    Source(PrimaryError),
    Session(SessionError),
    Rest(RestFailure),
    Ticket,
}

#[derive(Debug, Clone)]
pub struct RestTicket {
    owner: Arc<()>,
    label_ms: i64,
    reason: Option<ExceptionReason>,
    issued_at_ms: i64,
    deadline_ms: i64,
}
impl RestTicket {
    pub fn label_ms(&self) -> i64 {
        self.label_ms
    }
    pub fn reason(&self) -> Option<ExceptionReason> {
        self.reason
    }
    pub fn deadline_ms(&self) -> i64 {
        self.deadline_ms
    }
    pub fn issued_at_ms(&self) -> i64 {
        self.issued_at_ms
    }
    pub fn request_plan(&self) -> Result<ClosedM1RequestPlan, PrimaryError> {
        let start = chrono::DateTime::from_timestamp_millis(self.label_ms)
            .ok_or(PrimaryError::Timestamp)?;
        let end = start
            .checked_add_signed(chrono::Duration::minutes(10))
            .ok_or(PrimaryError::Timestamp)?;
        ClosedM1RequestPlan::new(start, end, vec![end]).map_err(|_| PrimaryError::ReferenceUnproven)
    }
}

#[derive(Debug)]
pub enum Tick {
    Waiting,
    AwaitingProof,
    Degraded,
    Finished,
    ProofRequired(RestTicket),
    Published {
        label_ms: i64,
        origin: &'static str,
        source_sha256: String,
        committed_at_ms: i64,
    },
}
#[derive(Debug, Default, Serialize)]
pub struct RoleCounts {
    pub proof_scheduled: u64,
    pub session_end_scheduled: u64,
    pub audit_scheduled: u64,
    pub proof_completed: u64,
    pub audit_completed: u64,
    pub unavailable: u64,
}
struct Slot {
    owner: Arc<()>,
    completed: bool,
    /// A source-admitted response may arrive before the WS grace has elapsed.
    response: Option<AdmittedClosedM1Snapshot>,
}

pub struct Coordinator {
    processor: PrimaryProcessor,
    session: runtimes::Runtimes,
    failed: bool,
    proofs: BTreeMap<i64, Slot>,
    audit_slots: BTreeSet<i64>,
    audit_pending: Option<RestTicket>,
    audit_status: AuditStatus,
    audit_every: Option<u16>,
    last_audit_issued: Option<i64>,
    last_committed: Option<i64>,
    counts: RoleCounts,
    recovery_completion: Option<crate::primary::RecoveryCompletion>,
    #[cfg(test)]
    proof_wait_io: Option<Arc<tests::ProofWaitIo>>,
}
impl Coordinator {
    fn source_result<T>(&mut self, result: Result<T, PrimaryError>) -> Result<T, CoordinatorError> {
        for change in self.processor.take_source_changes() {
            self.event(serde_json::json!({"event":"primary_source_change","generation":self.processor.generation(),"recorded_at_ms":crate::timing::now_ms(),"change":change}))?;
        }
        result.map_err(CoordinatorError::Source)
    }

    fn compare_input(
        &mut self,
        input: &crate::primary::AdmittedCanonicalM10,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<bool, CoordinatorError> {
        let result = input.compare_rest(source);
        let expected = &input.receipt().actual_m1;
        let actual: Vec<_> = source.bars().collect();
        let changed = expected.len() != actual.len()
            || expected
                .iter()
                .zip(&actual)
                .any(|(a, b)| !crate::primary::wire::same_values(a, b));
        if changed {
            self.event(serde_json::json!({"event":"primary_pre_callback_comparison","label_ms":input.receipt().label_ms,
                "classification":if result.is_ok(){"VOLUME_ONLY"}else{"PRICE_OR_INVENTORY_CONFLICT"},
                "expected":expected,"observed":actual,"source_sha256":source.sha256(),"model_input_rewritten":false}))?;
        }
        result.map(|()| changed).map_err(CoordinatorError::Source)
    }
    pub(super) fn cancel_audit_for_stop(
        &mut self,
        ticket: &RestTicket,
    ) -> Result<(), CoordinatorError> {
        let pending = self
            .audit_pending
            .as_ref()
            .ok_or(CoordinatorError::Ticket)?;
        if !Arc::ptr_eq(&pending.owner, &ticket.owner) {
            return Err(CoordinatorError::Ticket);
        }
        self.audit_pending = None;
        self.audit_status = AuditStatus::CancelledForStop;
        self.event(serde_json::json!({"event":"primary_audit_result","label_ms":ticket.label_ms,"status":self.audit_status,"source_sha256":null}))
    }
    /// Must run before any transport await. The accepted calendar fixes the full
    /// inventory/reference independently; an already reconstructed prefix is not
    /// silently replaced with an empty model or a caller-shortened label list.
    pub fn bind_calendar(
        &mut self,
        calendar: crate::primary::calendar::AdmittedCalendar,
    ) -> Result<(), CoordinatorError> {
        self.check()?;
        self.processor
            .bind_calendar(calendar)
            .map_err(|_| CoordinatorError::Binding)?;
        let evidence = self
            .processor
            .calendar()
            .ok_or(CoordinatorError::Binding)?
            .evidence();
        self.event(serde_json::json!({"event":"primary_calendar_bound","evidence":evidence,"bound_at_ms":crate::timing::now_ms()}))
    }
    /// Called synchronously before reconnect/proof awaits. Retire old tickets,
    /// but never reset the model or clear a failed callback.
    pub fn latch_recovery(&mut self) -> Result<(), CoordinatorError> {
        self.check()?;
        if !self.session.recoverable_primary() {
            self.failed = true;
            return Err(CoordinatorError::Held);
        }
        self.processor.break_continuity();
        self.recovery_completion = None;
        for slot in self.proofs.values_mut() {
            slot.completed = true;
            slot.response = None;
        }
        if self.audit_pending.take().is_some() {
            self.audit_status = AuditStatus::Unavailable;
        }
        Ok(())
    }
    pub fn plan_recovery(
        &mut self,
        cutover_ms: i64,
    ) -> Result<crate::primary::RecoveryPlan, CoordinatorError> {
        self.check()?;
        self.processor
            .plan_recovery(cutover_ms, crate::timing::now_ms())
            .map_err(CoordinatorError::Source)
    }
    pub fn complete_recovery(
        &mut self,
        plan: &crate::primary::RecoveryPlan,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<usize, CoordinatorError> {
        self.check()?;
        if self.recovery_completion.is_some() {
            return Err(CoordinatorError::Held);
        }
        let result = self
            .processor
            .admit_recovery(plan, source, crate::timing::now_ms());
        let admitted = self.source_result(result)?;
        let count = admitted.inputs.len();
        self.failed = true;
        self.event(serde_json::json!({"event":"primary_recovery_source_validated","plan_sha256":plan.sha256(),
            "scope":if plan.startup_anchor(){"STARTUP_ANCHOR"}else{"MODEL_GAP"},"generation":self.processor.generation(),
            "start_ms":plan.request().start().timestamp_millis(),"issued_at_ms":plan.issued_at_ms(),"deadline_ms":plan.deadline_ms(),
            "requested_at_ms":source.evidence().parts[0].requested_at.timestamp_millis(),"received_at_ms":source.evidence().parts[0].received_at.timestamp_millis(),
            "source_sha256":source.sha256(),"overlap_labels":plan.overlap_labels(),"replayed_labels":count,
            "cutover_ms":source.evidence().end.timestamp_millis(),"history_proves_live_readiness":false}))?;
        for input in admitted.inputs {
            let publication = self
                .session
                .recovery(&input, source)
                .map_err(CoordinatorError::Session)?;
            self.processor
                .recovered(input, publication.committed_at_ms)
                .map_err(CoordinatorError::Source)?;
            self.last_committed = Some(publication.label_ms);
        }
        self.recovery_completion = Some(admitted.completion);
        self.failed = false;
        Ok(count)
    }
    pub fn resolve_recovery(
        &mut self,
    ) -> Result<Option<crate::primary::ResolvedRecovery>, CoordinatorError> {
        self.check()?;
        let completion = self
            .recovery_completion
            .as_ref()
            .ok_or(CoordinatorError::Ticket)?;
        let result = self
            .processor
            .finish_recovery(completion, crate::timing::now_ms())
            .map_err(CoordinatorError::Source)?;
        if let Some(resolved) = &result {
            self.event(serde_json::json!({"event":"primary_canonical_recovery_resolved","generation":resolved.generation(),
                "source_sha256":resolved.source_sha256(),"cutover_ms":resolved.cutover_ms(),"resolved_at_ms":crate::timing::now_ms()}))?;
            self.recovery_completion = None;
        }
        Ok(result)
    }
    pub fn bind_subscription(
        &mut self,
        receipt: &crate::socket::primary::Subscription,
    ) -> Result<(), CoordinatorError> {
        self.check()?;
        self.processor
            .bind_subscription(receipt, crate::timing::now_ms())
            .map_err(CoordinatorError::Source)?;
        self.event(serde_json::json!({"event":"primary_subscription_sent","generation":receipt.generation(),
            "sent_at_ms":receipt.sent_at_ms(),"auth_generation":receipt.auth_generation(),"provider_ack":false}))
    }
    pub fn new(
        processor: PrimaryProcessor,
        session: ObservedSession,
    ) -> Result<Self, CoordinatorError> {
        Self::with_consumers(processor, runtimes::Runtimes::single(session))
    }
    /// Static same-config/policy cohort: one feed, one GET scheduler and one
    /// frontier. All consumers must own the same root STOP and next label.
    /// This library entry does not authorize aggregate cohort EOD or restart.
    pub fn new_group(
        processor: PrimaryProcessor,
        sessions: Vec<ObservedSession>,
    ) -> Result<Self, CoordinatorError> {
        Self::with_consumers(processor, runtimes::Runtimes::group(sessions)?)
    }
    fn with_consumers(
        processor: PrimaryProcessor,
        session: runtimes::Runtimes,
    ) -> Result<Self, CoordinatorError> {
        let (hash, label, policy) = session.primary_binding().ok_or(CoordinatorError::Binding)?;
        if hash != processor.config_sha256() || Some(label) != processor.next_label_ms() {
            return Err(CoordinatorError::Binding);
        }
        let audit_every = policy.audit_every_commits;
        Ok(Self {
            processor,
            session,
            failed: false,
            proofs: BTreeMap::new(),
            audit_slots: BTreeSet::new(),
            audit_pending: None,
            audit_status: AuditStatus::NotScheduled,
            audit_every,
            last_audit_issued: None,
            last_committed: None,
            counts: RoleCounts::default(),
            recovery_completion: None,
            #[cfg(test)]
            proof_wait_io: None,
        })
    }
    pub fn processor(&self) -> &PrimaryProcessor {
        &self.processor
    }
    pub fn role_counts(&self) -> &RoleCounts {
        &self.counts
    }
    pub fn audit_status(&self) -> AuditStatus {
        self.audit_status
    }
    fn check(&self) -> Result<(), CoordinatorError> {
        if self.failed {
            Err(CoordinatorError::Held)
        } else {
            Ok(())
        }
    }
    fn event(&mut self, event: serde_json::Value) -> Result<(), CoordinatorError> {
        if let Err(error) = self.session.record_transport(&event) {
            self.failed = true;
            return Err(CoordinatorError::Session(error));
        }
        #[cfg(test)]
        if event["event"] == "primary_proof_waiting_ws_grace"
            || event["event"] == "primary_precommit_rest_selected"
            || event["event"] == "primary_rest_proof_late_ignored"
            || event["event"] == "primary_rest_observation"
        {
            if let Some(io) = self.proof_wait_io.take() {
                io.advance_and_wait(&self.session.primary_runtime().unwrap().1);
            }
        }
        Ok(())
    }
    pub fn on_frame(&mut self, frame: WireFrame) -> Result<(), CoordinatorError> {
        self.check()?;
        // The bootstrap bridge retains unproven frames without re-timestamping.
        let result = self.processor.ingest(frame, crate::timing::now_ms());
        self.source_result(result)
    }
    pub fn tick(&mut self) -> Result<Tick, CoordinatorError> {
        self.check()?;
        let now = crate::timing::now_ms();
        match self
            .processor
            .prepare(now)
            .map_err(CoordinatorError::Source)?
        {
            CanonicalStatus::Ready(input) => self.publish(*input),
            CanonicalStatus::Finished => Ok(Tick::Finished),
            CanonicalStatus::AwaitingCommit => Ok(Tick::AwaitingProof),
            CanonicalStatus::Waiting => Ok(Tick::Waiting),
            CanonicalStatus::PendingProof {
                label_ms,
                reason,
                may_request,
            } => {
                if self.processor.gap_unresolved() {
                    return Ok(Tick::Degraded);
                }
                if !self.processor.subscription_data_ready() {
                    return Ok(Tick::Waiting);
                }
                if !may_request {
                    return Ok(Tick::Waiting);
                }
                if self.proofs.contains_key(&label_ms) {
                    return Ok(Tick::AwaitingProof);
                }
                let owner = Arc::new(());
                self.proofs.insert(
                    label_ms,
                    Slot {
                        owner: owner.clone(),
                        completed: false,
                        response: None,
                    },
                );
                self.counts.proof_scheduled += 1;
                if reason == ExceptionReason::SessionEnd {
                    self.counts.session_end_scheduled += 1;
                }
                let ticket = RestTicket {
                    owner,
                    label_ms,
                    reason: Some(reason),
                    issued_at_ms: now,
                    deadline_ms: self.processor.proof_deadline_ms(label_ms, now),
                };
                self.event(
                    serde_json::json!({"event":"primary_rest_proof_scheduled","label_ms":label_ms,
                    "reason":reason,"issued_at_ms":now,"deadline_ms":ticket.deadline_ms}),
                )?;
                Ok(Tick::ProofRequired(ticket))
            }
        }
    }
    fn publish(
        &mut self,
        input: crate::primary::AdmittedCanonicalM10,
    ) -> Result<Tick, CoordinatorError> {
        // Before model mutation: unwinding cannot leave a reusable operation.
        self.failed = true;
        let label = input.receipt().label_ms;
        let origin = input.receipt().origin;
        let comparison = self.proofs.get_mut(&label).and_then(|s| s.response.take());
        let mut volume_revision = false;
        if let Some(source) = &comparison {
            match self.compare_input(&input, source) {
                Ok(revised) => volume_revision = revised,
                Err(error) => {
                    self.processor.break_continuity();
                    self.event(serde_json::json!({"event":"primary_data_conflict_before_callback","label_ms":label}))?;
                    return Err(error);
                }
            }
        }
        let publication = self
            .session
            .canonical(&input)
            .map_err(CoordinatorError::Session)?;
        self.processor
            .committed(input, publication.committed_at_ms)
            .map_err(CoordinatorError::Source)?;
        self.failed = false;
        self.last_committed = Some(label);
        if let Some(source) = comparison {
            self.event(serde_json::json!({"event":"primary_proof_became_comparison","label_ms":label,"result":if volume_revision{"VOLUME_REVISION"}else{"MATCH"},"source_sha256":source.sha256()}))?;
        }
        Ok(Tick::Published {
            label_ms: label,
            origin,
            source_sha256: publication.source_sha256,
            committed_at_ms: publication.committed_at_ms,
        })
    }
    fn response_bounds(ticket: &RestTicket, source: &AdmittedClosedM1Snapshot, now: i64) -> bool {
        source.evidence().start.timestamp_millis() == ticket.label_ms
            && source.evidence().end.timestamp_millis() == ticket.label_ms + 600000
            && source.evidence().parts.len() == 1
            && source.evidence().parts.iter().all(|p| {
                p.requested_at.timestamp_millis() >= ticket.issued_at_ms
                    && p.received_at.timestamp_millis() <= ticket.deadline_ms
                    && p.received_at.timestamp_millis() <= now
            })
    }
    pub fn complete_proof(
        &mut self,
        ticket: RestTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<Tick, CoordinatorError> {
        self.check()?;
        let slot = self
            .proofs
            .get_mut(&ticket.label_ms)
            .ok_or(CoordinatorError::Ticket)?;
        if slot.completed || !Arc::ptr_eq(&slot.owner, &ticket.owner) {
            return Err(CoordinatorError::Ticket);
        }
        slot.completed = true;
        self.counts.proof_completed += 1;
        if self.processor.rest_nonblocking() {
            return self.complete_nonblocking_proof(ticket, result);
        }
        let now = crate::timing::now_ms();
        let source = match result {
            Ok(source) => source,
            Err(failure) => {
                self.event(serde_json::json!({"event":"primary_rest_proof_failed","label_ms":ticket.label_ms,"class":failure}))?;
                if matches!(failure, RestFailure::Unavailable | RestFailure::Timeout) {
                    self.counts.unavailable += 1;
                    return Ok(Tick::AwaitingProof);
                }
                self.failed = true;
                return Err(CoordinatorError::Rest(failure));
            }
        };
        // A valid but late proof is not an instruction to roll back a WS
        // callback or to relabel the same stale data as a fresh WS fallback.
        // All non-timing bounds remain fail-closed, including future receipt.
        let identity_bounds = source.evidence().start.timestamp_millis() == ticket.label_ms
            && source.evidence().end.timestamp_millis() == ticket.label_ms + 600000
            && source.evidence().parts.len() == 1
            && source.evidence().parts.iter().all(|p| {
                p.requested_at.timestamp_millis() >= ticket.issued_at_ms
                    && p.received_at >= p.requested_at
                    && p.received_at.timestamp_millis() <= now
            });
        if self.processor.rest_precommit_ws_first()
            && identity_bounds
            && (now >= ticket.deadline_ms
                || source.evidence().parts[0].received_at.timestamp_millis() >= ticket.deadline_ms)
        {
            let inventory = self.processor.validate_known_rest_inventory(&source);
            self.source_result(inventory)?;
            self.session
                .record_primary_reference(&source)
                .map_err(|error| {
                    self.failed = true;
                    CoordinatorError::Session(error)
                })?;
            self.event(serde_json::json!({"event":"primary_rest_proof_late_ignored",
                "label_ms":ticket.label_ms,"deadline_ms":ticket.deadline_ms,
                "processed_at_ms":now,"source_sha256":source.sha256(),
                "ws_already_committed":self.processor.next_label_ms().map_or(true,|label|label>ticket.label_ms),
                "model_input_rewritten":false}))?;
            return Ok(Tick::AwaitingProof);
        }
        if !Self::response_bounds(&ticket, &source, now) {
            self.failed = true;
            return Err(CoordinatorError::Rest(RestFailure::Integrity));
        }
        self.session
            .record_primary_reference(&source)
            .map_err(|error| {
                self.failed = true;
                CoordinatorError::Session(error)
            })?;
        if self
            .processor
            .next_label_ms()
            .map_or(true, |label| label > ticket.label_ms)
        {
            let result = self.processor.compare_rest_audit(&source);
            let revised = self.processor.pending_volume_revision();
            self.source_result(result)?;
            self.event(serde_json::json!({"event":"primary_proof_became_comparison","label_ms":ticket.label_ms,"result":if revised{"VOLUME_REVISION"}else{"MATCH"},"source_sha256":source.sha256()}))?;
            return Ok(Tick::Waiting);
        }
        if let CanonicalStatus::Ready(input) = self
            .processor
            .prepare(now)
            .map_err(CoordinatorError::Source)?
        {
            if let Err(error) = self.compare_input(&input, &source) {
                self.failed = true;
                self.processor.break_continuity();
                self.event(serde_json::json!({"event":"primary_data_conflict_before_callback","label_ms":ticket.label_ms}))?;
                return Err(error);
            }
            let published = self.publish(*input)?;
            let result = self.processor.compare_rest_audit(&source);
            self.source_result(result)?;
            return Ok(published);
        }
        let result = self.processor.prepare_rest_exception(
            &source,
            ticket.reason.ok_or(CoordinatorError::Ticket)?,
            now,
        );
        match self.source_result(result)? {
            Some(input) => {
                if self.processor.rest_precommit_ws_first()
                    && input.receipt().reason == Some(ExceptionReason::StartupPartial)
                {
                    self.event(
                        serde_json::json!({"event":"primary_precommit_rest_selected",
                        "label_ms":ticket.label_ms,"source_sha256":source.sha256(),
                        "selected_at_ms":crate::timing::now_ms(),"deadline_ms":ticket.deadline_ms,
                        "origin":"REST_EXCEPTION","model_input_rewritten":false}),
                    )?;
                }
                self.publish(*input)
            }
            None => {
                self.event(serde_json::json!({"event":"primary_proof_waiting_ws_grace","label_ms":ticket.label_ms,"source_sha256":source.sha256()}))?;
                self.proofs
                    .get_mut(&ticket.label_ms)
                    .ok_or(CoordinatorError::Ticket)?
                    .response = Some(source);
                Ok(Tick::Waiting)
            }
        }
    }

    fn response_identity(ticket: &RestTicket, source: &AdmittedClosedM1Snapshot, now: i64) -> bool {
        source.evidence().start.timestamp_millis() == ticket.label_ms
            && source.evidence().end.timestamp_millis() == ticket.label_ms + 600000
            && source.evidence().parts.len() == 1
            && source.evidence().parts.iter().all(|p| {
                p.requested_at.timestamp_millis() >= ticket.issued_at_ms
                    && p.received_at >= p.requested_at
                    && p.received_at.timestamp_millis() <= now
            })
    }

    fn rest_observation_failure(
        &mut self,
        ticket: &RestTicket,
        role: &str,
        class: RestFailure,
    ) -> Result<(), CoordinatorError> {
        self.event(serde_json::json!({"event":"primary_rest_nonblocking_failed","label_ms":ticket.label_ms,
            "role":role,"class":class,"issued_at_ms":ticket.issued_at_ms,"deadline_ms":ticket.deadline_ms,
            "recorded_at_ms":crate::timing::now_ms(),"nonblocking":true,"model_input_rewritten":false,
            "model_impact":"NOT_EVALUATED_REQUIRES_OFFLINE_REPLAY"}))
    }

    fn complete_nonblocking_proof(
        &mut self,
        ticket: RestTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<Tick, CoordinatorError> {
        let source = match result {
            Ok(source) => source,
            Err(class) => {
                self.counts.unavailable += 1;
                self.rest_observation_failure(&ticket, "Proof", class)?;
                return Ok(Tick::AwaitingProof);
            }
        };
        if !Self::response_identity(&ticket, &source, crate::timing::now_ms()) {
            self.rest_observation_failure(&ticket, "Proof", RestFailure::Integrity)?;
            return Ok(Tick::AwaitingProof);
        }
        self.session
            .record_primary_reference(&source)
            .map_err(|error| {
                self.failed = true;
                CoordinatorError::Session(error)
            })?;
        let assessment = self.processor.assess_rest(&source);
        let observed_at = crate::timing::now_ms();
        let late = observed_at >= ticket.deadline_ms
            || source.evidence().parts[0].received_at.timestamp_millis() >= ticket.deadline_ms;
        self.event(serde_json::json!({"event":"primary_rest_observation","role":"Proof","label_ms":ticket.label_ms,
            "source_sha256":source.sha256(),"assessment":assessment,"late":late,"deadline_ms":ticket.deadline_ms,
            "recorded_at_ms":observed_at,"nonblocking":true,"model_input_rewritten":false}))?;
        let now = crate::timing::now_ms();
        // Every source-consuming condition is rechecked after evidence IO.
        // Returning AwaitingProof asks the real writer to assess WS immediately.
        if late
            || now >= ticket.deadline_ms
            || self
                .processor
                .next_label_ms()
                .map_or(true, |next| next > ticket.label_ms)
            || !assessment.missing_known_m1_ms.is_empty()
        {
            return Ok(Tick::AwaitingProof);
        }
        if let CanonicalStatus::Ready(input) = self
            .processor
            .prepare(now)
            .map_err(CoordinatorError::Source)?
        {
            return self.publish(*input);
        }
        let reason = ticket.reason.ok_or(CoordinatorError::Ticket)?;
        if !self
            .processor
            .rest_source_prices_compatible(&source, reason)
        {
            self.event(serde_json::json!({"event":"primary_rest_source_price_rejected","label_ms":ticket.label_ms,
                "source_sha256":source.sha256(),"reason":reason,"nonblocking":true,"model_input_rewritten":false}))?;
            return Ok(Tick::AwaitingProof);
        }
        let result =
            self.processor
                .prepare_rest_exception(&source, reason, crate::timing::now_ms());
        match self.source_result(result)? {
            Some(input) => {
                self.event(serde_json::json!({"event":"primary_precommit_rest_selected","label_ms":ticket.label_ms,
                    "source_sha256":source.sha256(),"selected_at_ms":crate::timing::now_ms(),
                    "deadline_ms":ticket.deadline_ms,"origin":"REST_EXCEPTION","model_input_rewritten":false}))?;
                self.publish(*input)
            }
            None => Ok(Tick::AwaitingProof),
        }
    }
    /// Ticket counters measure scheduled roles, not a claim of actual HTTP GETs.
    /// At most one audit per selected slot, >=60min between issues, proof first.
    pub fn take_audit(&mut self) -> Result<Option<RestTicket>, CoordinatorError> {
        self.check()?;
        let Some(every) = self.audit_every else {
            return Ok(None);
        };
        let now = crate::timing::now_ms();
        let Some(label) = self.last_committed else {
            return Ok(None);
        };
        if self.audit_pending.is_some()
            || self.processor.counters().committed % u64::from(every) != 0
            || self.audit_slots.contains(&label)
            || self
                .last_audit_issued
                .is_some_and(|last| now - last < 3600000)
            || self.proofs.values().any(|slot| !slot.completed)
        {
            return Ok(None);
        }
        let ticket = RestTicket {
            owner: Arc::new(()),
            label_ms: label,
            reason: None,
            issued_at_ms: now,
            deadline_ms: now + 5000,
        };
        self.audit_slots.insert(label);
        self.audit_pending = Some(ticket.clone());
        self.audit_status = AuditStatus::Pending;
        self.last_audit_issued = Some(now);
        self.counts.audit_scheduled += 1;
        self.event(
            serde_json::json!({"event":"primary_audit_scheduled","label_ms":label,"at_ms":now}),
        )?;
        Ok(Some(ticket))
    }
    pub fn complete_audit(
        &mut self,
        ticket: RestTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<AuditStatus, CoordinatorError> {
        self.check()?;
        let pending = self
            .audit_pending
            .as_ref()
            .ok_or(CoordinatorError::Ticket)?;
        if !Arc::ptr_eq(&pending.owner, &ticket.owner) {
            return Err(CoordinatorError::Ticket);
        }
        self.audit_pending = None;
        self.counts.audit_completed += 1;
        if self.processor.rest_nonblocking() {
            return self.complete_nonblocking_audit(ticket, result);
        }
        let source_sha256 = result.as_ref().ok().map(|s| s.sha256().to_owned());
        let status = match result {
            Err(RestFailure::Unavailable | RestFailure::Timeout) => {
                self.counts.unavailable += 1;
                AuditStatus::Unavailable
            }
            Err(failure) => {
                self.failed = true;
                return Err(CoordinatorError::Rest(failure));
            }
            Ok(source) => {
                if !Self::response_bounds(&ticket, &source, crate::timing::now_ms()) {
                    self.failed = true;
                    return Err(CoordinatorError::Rest(RestFailure::Integrity));
                }
                let result = self.processor.compare_rest_audit(&source);
                let revised = self.processor.pending_volume_revision();
                if self.source_result(result).is_err() {
                    self.failed = true;
                    AuditStatus::Conflict
                } else if revised {
                    AuditStatus::VolumeRevision
                } else {
                    AuditStatus::Match
                }
            }
        };
        self.audit_status = status;
        self.event(serde_json::json!({"event":"primary_audit_result","label_ms":ticket.label_ms,"status":status,"source_sha256":source_sha256}))?;
        Ok(status)
    }

    fn complete_nonblocking_audit(
        &mut self,
        ticket: RestTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<AuditStatus, CoordinatorError> {
        let source = match result {
            Ok(source) if Self::response_identity(&ticket, &source, crate::timing::now_ms()) => {
                source
            }
            result => {
                let class = result.err().unwrap_or(RestFailure::Integrity);
                self.counts.unavailable += 1;
                self.rest_observation_failure(&ticket, "Audit", class)?;
                self.audit_status = AuditStatus::EvidenceUnavailable;
                self.event(serde_json::json!({"event":"primary_audit_result","label_ms":ticket.label_ms,
                    "status":self.audit_status,"source_sha256":null,"class":class,"nonblocking":true}))?;
                return Ok(self.audit_status);
            }
        };
        self.session
            .record_primary_reference(&source)
            .map_err(|error| {
                self.failed = true;
                CoordinatorError::Session(error)
            })?;
        let assessment = self.processor.assess_rest(&source);
        self.audit_status = AuditStatus::EvidenceOnly;
        self.event(
            serde_json::json!({"event":"primary_audit_result","label_ms":ticket.label_ms,
            "status":self.audit_status,"source_sha256":source.sha256(),"assessment":assessment,
            "recorded_at_ms":crate::timing::now_ms(),"nonblocking":true}),
        )?;
        Ok(self.audit_status)
    }
}

#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod tests;
