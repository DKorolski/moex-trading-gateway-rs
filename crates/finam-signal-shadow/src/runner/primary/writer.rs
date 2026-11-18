//! One bounded mailbox, one blocking owner of model + journal. No disk/model
//! work is executed in the socket/HTTP runtime tasks. A dropped caller cannot
//! detach a reusable model operation or declare a drained/clean run.
use super::{
    bootstrap::{BootstrapBridge, BootstrapTicket},
    http::HttpOutcome,
    live::{Cause, Faults, Job},
    *,
};
use crate::{
    primary::LiveDataStamp,
    socket::primary::{FailureSlot, Subscription},
    transport::StopHandle,
};
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::Instant,
};

pub(super) enum Operation {
    BeginRecovery(crate::socket::primary::Failure),
    BeginDataRecovery(crate::socket::primary::Failure),
    RecoveryDataWait(super::reconnect::Snapshot),
    RecoveryFailure {
        audit: super::reconnect::Snapshot,
        stage: super::reconnect::Stage,
    },
    PrepareAttempt {
        generation: u64,
        slot: FailureSlot,
    },
    RecoveryPlan(i64),
    RecoveryRequest {
        cutover: i64,
        previous: Option<Box<crate::primary::RecoveryPlan>>,
    },
    ResolveRecovery(Box<(super::reconnect::Episode, super::reconnect::Attempt)>),
    Bind(Subscription),
    Frame(WireFrame),
    Tick,
    Rest(Job, Box<HttpOutcome>),
    DrainAudit(RestTicket, Box<HttpOutcome>),
    #[cfg(test)]
    Final(serde_json::Value),
    Finalize(crate::primary::eod::FinishFacts),
    #[cfg(test)]
    Pause(std::sync::Arc<tests::Pause>),
}
pub(super) struct Update {
    pub episode: Option<super::reconnect::Episode>,
    pub attempt: Option<super::reconnect::Attempt>,
    pub subscription: Option<Subscription>,
    pub recovery_plan: Option<crate::primary::RecoveryPlan>,
    pub recovery_error: Option<super::reconnect::Error>,
    pub recovery_wait: Option<crate::primary::RecoveryWait>,
    pub recovery_ready: bool,
    pub completed_recovery_sha256: Option<String>,
    pub transport_failure: Option<crate::socket::primary::Failure>,
    pub bootstrap: Option<BootstrapTicket>,
    pub proof: Option<RestTicket>,
    pub audit: Option<RestTicket>,
    pub data: Option<LiveDataStamp>,
    pub bootstrap_pending: bool,
    pub committed: u64,
    pub next_wake_ms: i64,
    pub finalized: bool,
    pub eod_prepared: bool,
    pub journal_seal: Option<crate::primary::eod::JournalSeal>,
}
struct Core {
    gate: Option<crate::primary::ContinuityGate>,
    reference_start: DateTime<Utc>,
    run_deadline_ms: i64,
    recovery_budget: super::reconnect::RunBudget,
    recovering: bool,
    recovery_reads: u8,
    expected_revision: u64,
    seen_revision: u64,
    owner: Coordinator,
    bootstrap: BootstrapBridge,
    stop: StopHandle,
    faults: Faults,
    socket_fault: FailureSlot,
}
impl Core {
    fn step(&mut self, op: Operation) -> Result<Update, Cause> {
        let data_only = matches!(&op, Operation::BeginDataRecovery(_));
        if let Operation::RecoveryFailure { audit, stage } = op {
            self.owner
                .event(
                    serde_json::json!({"event":"primary_recovery_attempt_failed",
                "stage_at_failure":stage,"audit":audit}),
                )
                .map_err(Cause::Coordinator)?;
            return Ok(self.update());
        }
        if let Operation::Finalize(facts) = op {
            return self.finalize(facts);
        }
        if let Operation::DrainAudit(ticket, outcome) = op {
            if !self.stop.is_stopped() {
                return Err(Cause::Binding);
            }
            if matches!(outcome.result, Err(RestFailure::Cancelled))
                && outcome.receipt.auth_generation.is_some()
            {
                self.owner
                    .cancel_audit_for_stop(&ticket)
                    .map_err(Cause::Coordinator)?;
            } else {
                if let Ok(source) = &outcome.result {
                    if !self.owner.processor.rest_nonblocking() {
                        self.owner
                            .session
                            .record_primary_reference(source)
                            .map_err(|e| Cause::Coordinator(CoordinatorError::Session(e)))?;
                    }
                }
                if self
                    .owner
                    .complete_audit(ticket, outcome.result)
                    .map_err(Cause::Coordinator)?
                    == AuditStatus::Conflict
                {
                    return Err(Cause::Coordinator(CoordinatorError::Source(
                        PrimaryError::CanonicalConflict,
                    )));
                }
            }
            return Ok(self.update());
        }
        #[cfg(test)]
        if let Operation::Final(event) = op {
            let recorded = self.owner.event(event).is_ok();
            // Stage A never calls this a full session/EOD success. B supplies
            // independent calendar, SESSION_END proof and terminal verifier.
            let finished = matches!(
                self.owner.session.finish("INCOMPLETE_primary_bounded_stop"),
                Ok(()) | Err(SessionError::Incomplete)
            );
            let mut result = self.update();
            result.finalized = recorded && finished;
            return Ok(result);
        }
        if self.gate.is_none() {
            if let Some(fault) = self.socket_fault.first() {
                self.owner.processor.break_continuity();
                self.faults.record(Cause::Socket(Box::new(fault)));
            }
        }
        if let Some(fault) = self.faults.first() {
            return Err(fault);
        }
        if self.stop.is_stopped() {
            return Err(Cause::RequestedStop);
        }
        if let Some(gate) = &self.gate {
            let state = gate.snapshot();
            if let Some(failure) = state.failure {
                if state.revision != self.seen_revision {
                    self.owner.latch_recovery().map_err(Cause::Coordinator)?;
                    self.seen_revision = state.revision;
                }
                if !matches!(
                    op,
                    Operation::BeginRecovery(_)
                        | Operation::BeginDataRecovery(_)
                        | Operation::PrepareAttempt { .. }
                        | Operation::ResolveRecovery(_)
                ) {
                    let mut update = self.update();
                    update.transport_failure = Some(failure);
                    return Ok(update);
                }
            }
        }
        match op {
            Operation::RecoveryDataWait(audit) => {
                if !self.owner.processor.data_wait_recovery() || !self.recovering {
                    return Err(Cause::Binding);
                }
                self.owner
                    .event(
                        serde_json::json!({"event":"primary_recovery_waiting_first_data",
                    "audit":audit,"history_proves_live_readiness":false}),
                    )
                    .map_err(Cause::Coordinator)?;
                return Ok(self.update());
            }
            Operation::BeginRecovery(failure) | Operation::BeginDataRecovery(failure) => {
                let episode = self
                    .recovery_budget
                    .begin_impl_for_writer(&mut self.owner, failure, data_only)
                    .map_err(Cause::Recovery)?;
                let mut event = serde_json::json!({"event":"primary_recovery_episode_started","audit":episode.audit().snapshot()});
                if self.owner.processor.ws_historical_recovery() {
                    event["continuity_revision"] = serde_json::json!(
                        self.gate
                            .as_ref()
                            .ok_or(Cause::Binding)?
                            .snapshot()
                            .revision
                    );
                }
                self.owner.event(event).map_err(Cause::Coordinator)?;
                self.recovering = true;
                self.recovery_reads = 0;
                if self.owner.processor.data_wait_recovery() {
                    let gate = self.gate.as_ref().ok_or(Cause::Binding)?;
                    let state = gate.snapshot();
                    if state.generation == self.owner.processor.generation()
                        && state.lost
                        && state.failure.is_none()
                    {
                        self.expected_revision = state.revision;
                        self.seen_revision = state.revision;
                    }
                }
                let mut update = self.update();
                update.episode = Some(episode);
                return Ok(update);
            }
            Operation::PrepareAttempt { generation, slot } => {
                let gate = self.gate.as_ref().ok_or(Cause::Binding)?;
                self.expected_revision = gate.begin_generation(generation).ok_or(Cause::Binding)?;
                self.seen_revision = self.expected_revision;
                self.owner.latch_recovery().map_err(Cause::Coordinator)?;
                self.owner
                    .processor
                    .begin_generation(generation, crate::timing::now_ms())
                    .map_err(|e| Cause::Coordinator(CoordinatorError::Source(e)))?;
                self.socket_fault = slot;
                self.bootstrap = BootstrapBridge::new(
                    self.owner.processor.config_sha256().into(),
                    generation,
                    self.reference_start,
                    self.run_deadline_ms,
                )
                .map_err(Cause::Coordinator)?;
                self.owner.event(serde_json::json!({"event":"primary_recovery_attempt_prepared","generation":generation,"continuity_revision":self.expected_revision})).map_err(Cause::Coordinator)?;
                return Ok(self.update());
            }
            Operation::RecoveryPlan(cutover) => {
                let plan = self
                    .owner
                    .plan_recovery(cutover)
                    .map_err(Cause::Coordinator)?;
                let mut update = self.update();
                update.recovery_plan = Some(plan);
                return Ok(update);
            }
            Operation::RecoveryRequest { cutover, previous } => {
                let audit = self
                    .recovery_budget
                    .last_audit()
                    .ok_or(Cause::Binding)?
                    .snapshot();
                if crate::timing::now_ms() >= audit.deadline_ms {
                    return Err(Cause::Recovery(super::reconnect::Error::EpisodeDeadline));
                }
                if !self.recovering
                    || audit.generation != self.owner.processor.generation()
                    || !audit.canonical_unresolved
                    || (audit.transport_unresolved
                        && !(self.owner.processor.data_wait_recovery()
                            && audit.stage == super::reconnect::Stage::Resync
                            && self
                                .gate
                                .as_ref()
                                .is_some_and(|g| g.snapshot().failure.is_none())))
                {
                    return Err(Cause::Binding);
                }
                if self.recovery_reads >= crate::primary::MAX_REST_READS {
                    return Err(Cause::Recovery(
                        super::reconnect::Error::RestTailRequestsExhausted,
                    ));
                }
                let scope = crate::primary::RecoveryRequestScope {
                    episode: audit.episode,
                    ordinal: self.recovery_reads + 1,
                    deadline_ms: audit.deadline_ms,
                };
                let plan = self
                    .owner
                    .issue_recovery_request(cutover, scope, previous.as_deref())
                    .map_err(Cause::Coordinator)?;
                self.recovery_reads += 1;
                let mut update = self.update();
                update.recovery_plan = Some(plan);
                return Ok(update);
            }
            Operation::ResolveRecovery(pair) => {
                let (mut episode, attempt) = *pair;
                let state = self.gate.as_ref().ok_or(Cause::Binding)?.snapshot();
                let result = if state.revision != self.expected_revision || state.failure.is_some()
                {
                    None
                } else {
                    self.owner.resolve_recovery().map_err(Cause::Coordinator)?
                };
                if let Some(resolved) = result {
                    if self
                        .gate
                        .as_ref()
                        .ok_or(Cause::Binding)?
                        .resolve(resolved.generation(), self.expected_revision)
                    {
                        episode
                            .canonical_ready(&self.owner, &attempt, &resolved)
                            .map_err(Cause::Recovery)?;
                        self.recovering = false;
                        self.owner.event(serde_json::json!({"event":"primary_recovery_episode_ready","audit":episode.audit().snapshot()})).map_err(Cause::Coordinator)?;
                        return Ok(self.update());
                    }
                }
                self.owner.latch_recovery().map_err(Cause::Coordinator)?;
                let mut update = self.update();
                update.episode = Some(episode);
                update.attempt = Some(attempt);
                update.recovery_error = Some(resolution_error(
                    self.gate
                        .as_ref()
                        .ok_or(Cause::Binding)?
                        .snapshot()
                        .failure
                        .as_ref(),
                ));
                return Ok(update);
            }
            _ => (),
        }
        let mut bootstrap = None;
        let mut completed_tick = None;
        let mut subscription = None;
        match op {
            Operation::Bind(receipt) => {
                self.owner
                    .bind_subscription(&receipt)
                    .map_err(Cause::Coordinator)?;
                subscription = Some(receipt);
            }
            Operation::Frame(frame) => {
                let result = self.bootstrap.on_frame(&mut self.owner, frame);
                if self.gate.is_some()
                    && matches!(
                        &result,
                        Err(CoordinatorError::Source(PrimaryError::Continuity))
                    )
                    && self.owner.check().is_ok()
                {
                    let failure = crate::socket::primary::Failure {
                        generation: self.owner.processor.generation(),
                        kind: crate::socket::primary::Kind::Source(PrimaryError::Continuity),
                        liveness: None,
                    };
                    self.socket_fault.record(failure.clone());
                    let mut update = self.update();
                    update.transport_failure = Some(failure);
                    return Ok(update);
                }
                bootstrap = result.map_err(Cause::Coordinator)?;
            }
            Operation::Tick => (),
            Operation::Rest(job, outcome) => {
                if self.owner.processor.rest_nonblocking() && outcome.authority_rejected() {
                    return Err(Cause::Http(
                        outcome.result.err().unwrap_or(RestFailure::Integrity),
                    ));
                }
                let outcome = *outcome;
                // An audit/comparison must remain independently reproducible,
                // not just a claimed MATCH and a byte count. Bootstrap retains
                // its source in the bridge; proof retains it after bounds checks
                // in the coordinator (including a response held during grace).
                if !(matches!(job, Job::Bootstrap(_) | Job::Proof(_))
                    || self.owner.processor.rest_tail_recovery() && matches!(job, Job::Recovery(_))
                    || self.owner.processor.rest_nonblocking() && matches!(job, Job::Audit(_)))
                {
                    if let Ok(source) = &outcome.result {
                        self.owner
                            .session
                            .record_primary_reference(source)
                            .map_err(|e| Cause::Coordinator(CoordinatorError::Session(e)))?;
                    }
                }
                let bytes = outcome.receipt.body.as_ref().map(|b| b.bytes).or_else(|| {
                    outcome.result.as_ref().ok().map(|s| {
                        s.evidence()
                            .parts
                            .iter()
                            .map(|p| p.raw_body.len())
                            .sum::<usize>()
                    })
                });
                self.owner
                    .event(
                        serde_json::json!({"event":"primary_http_completed","role":job.role(),
                    "receipt":outcome.receipt,"complete_response_body_bytes":bytes,
                    "response_body_bytes_unknown_on_failure":bytes.is_none()}),
                    )
                    .map_err(Cause::Coordinator)?;
                match job {
                    Job::Recovery(plan) => match outcome.result {
                        Ok(source) => {
                            if self.owner.processor.rest_tail_recovery() {
                                let progress = self
                                    .owner
                                    .complete_recovery_request(&plan, &source)
                                    .map_err(Cause::Coordinator)?;
                                let mut update = self.update();
                                update.completed_recovery_sha256 = Some(plan.sha256().to_owned());
                                match progress {
                                    RecoveryProgress::DataNotReady(wait) => {
                                        update.recovery_wait = Some(wait)
                                    }
                                    RecoveryProgress::Ready(_) => update.recovery_ready = true,
                                    RecoveryProgress::Retired => (),
                                }
                                return Ok(update);
                            }
                            self.owner
                                .complete_recovery(&plan, &source)
                                .map_err(Cause::Coordinator)?;
                        }
                        Err(error @ (RestFailure::Unavailable | RestFailure::Timeout)) => {
                            if self.owner.processor.data_wait_recovery() {
                                let r = &outcome.receipt;
                                let now = crate::timing::now_ms();
                                if !r.executor_completed
                                    || r.deadline_ms != plan.deadline_ms()
                                    || r.started_at_ms < plan.issued_at_ms()
                                    || r.finished_at_ms < r.started_at_ms
                                    || r.finished_at_ms > now
                                    || r.get_attempts > 1
                                    || (r.get_attempts > 0 && r.auth_generation.is_none())
                                {
                                    return Err(Cause::Http(RestFailure::Integrity));
                                }
                                if self.owner.processor.recovery_request_active(&plan) {
                                    self.owner
                                        .processor
                                        .retire_failed_recovery_request(&plan, now)
                                        .map_err(|e| {
                                            Cause::Coordinator(CoordinatorError::Source(e))
                                        })?;
                                } else if self.owner.processor.ws_historical_recovery()
                                    && self.owner.recovery_completion.is_some()
                                {
                                    self.owner.event(serde_json::json!({
                                        "event":"primary_recovery_request_stale_discarded",
                                        "ticket_sha256":plan.sha256(),"generation":plan.generation(),
                                        "at_ms":now,"callback":false,"error":error
                                    })).map_err(Cause::Coordinator)?;
                                    let mut update = self.update();
                                    update.completed_recovery_sha256 =
                                        Some(plan.sha256().to_owned());
                                    return Ok(update);
                                } else {
                                    return Err(Cause::Binding);
                                }
                                self.owner.event(serde_json::json!({"event":"primary_recovery_request_failed",
                                    "ticket_sha256":plan.sha256(),"scope":plan.scope(),"generation":plan.generation(),
                                    "error":error,"receipt":r,"at_ms":now,"callback":false,
                                    "validated_source":false})).map_err(Cause::Coordinator)?;
                            }
                            let mut update = self.update();
                            update.completed_recovery_sha256 = Some(plan.sha256().to_owned());
                            update.recovery_error = Some(super::reconnect::Error::Rest(error));
                            return Ok(update);
                        }
                        Err(error) => return Err(Cause::Http(error)),
                    },
                    Job::Bootstrap(ticket) => {
                        let result = if self.recovering {
                            self.bootstrap.complete_recovering(
                                &mut self.owner,
                                ticket,
                                outcome.result,
                            )
                        } else {
                            self.bootstrap
                                .complete(&mut self.owner, ticket, outcome.result)
                        };
                        if self.recovering {
                            if let Err(CoordinatorError::Rest(
                                error @ (RestFailure::Unavailable | RestFailure::Timeout),
                            )) = &result
                            {
                                let mut update = self.update();
                                update.recovery_error = Some(super::reconnect::Error::Rest(*error));
                                return Ok(update);
                            }
                        }
                        if self.gate.is_some()
                            && self.owner.processor.consistent_sources()
                            && matches!(
                                &result,
                                Err(CoordinatorError::Source(PrimaryError::Continuity))
                            )
                            && self.owner.check().is_ok()
                        {
                            let failure = crate::socket::primary::Failure {
                                generation: self.owner.processor.generation(),
                                kind: crate::socket::primary::Kind::Source(
                                    PrimaryError::Continuity,
                                ),
                                liveness: None,
                            };
                            self.socket_fault.record(failure.clone());
                            let mut update = self.update();
                            update.transport_failure = Some(failure);
                            return Ok(update);
                        }
                        result.map_err(Cause::Coordinator)?;
                    }
                    Job::Proof(ticket) => {
                        completed_tick = Some(
                            self.owner
                                .complete_proof(ticket, outcome.result)
                                .map_err(Cause::Coordinator)?,
                        )
                    }
                    Job::Audit(ticket) => {
                        let status = self
                            .owner
                            .complete_audit(ticket, outcome.result)
                            .map_err(Cause::Coordinator)?;
                        if status == AuditStatus::Conflict {
                            return Err(Cause::Coordinator(CoordinatorError::Source(
                                PrimaryError::CanonicalConflict,
                            )));
                        }
                    }
                }
            }
            #[cfg(test)]
            Operation::Pause(gate) => gate.wait(&self.stop),
            #[cfg(test)]
            Operation::Final(_) => unreachable!(),
            Operation::Finalize(_) | Operation::DrainAudit(_, _) => unreachable!(),
            Operation::BeginRecovery(_)
            | Operation::BeginDataRecovery(_)
            | Operation::RecoveryFailure { .. }
            | Operation::RecoveryDataWait(_)
            | Operation::PrepareAttempt { .. }
            | Operation::RecoveryPlan(_)
            | Operation::RecoveryRequest { .. }
            | Operation::ResolveRecovery(_) => unreachable!(),
        }
        if self.stop.is_stopped() {
            return Err(Cause::RequestedStop);
        }
        if self.recovering
            && !self.bootstrap.pending()
            && self.owner.processor.ws_historical_recovery()
        {
            let state = self.gate.as_ref().ok_or(Cause::Binding)?.snapshot();
            if state.failure.is_none()
                && state.revision == self.expected_revision
                && self.socket_fault.first().is_none()
            {
                let audit = self
                    .recovery_budget
                    .last_audit()
                    .ok_or(Cause::Binding)?
                    .snapshot();
                if !audit.transport_unresolved {
                    self.owner
                        .try_ws_recovery(audit.episode, audit.deadline_ms, state.revision)
                        .map_err(Cause::Coordinator)?;
                }
            }
        }
        let tick = if self.bootstrap.pending() || self.recovering {
            None
        } else {
            Some(match completed_tick {
                // A known proof outage can complete exactly as grace expires.
                // Re-evaluate WS now; filtering past timer instants must not
                // postpone a ready canonical bucket to the latency fence.
                Some(Tick::AwaitingProof) => self.owner.tick().map_err(Cause::Coordinator)?,
                Some(tick) => tick,
                None => self.owner.tick().map_err(Cause::Coordinator)?,
            })
        };
        let proof = match tick {
            Some(Tick::ProofRequired(ticket)) => Some(ticket),
            _ => None,
        };
        if !self.recovering && self.owner.processor.gap_unresolved() {
            if let Some(gate) = &self.gate {
                if self.owner.processor.data_wait_recovery() || gate.healthy() {
                    let failure = crate::socket::primary::Failure {
                        generation: self.owner.processor.generation(),
                        kind: crate::socket::primary::Kind::Source(PrimaryError::Continuity),
                        liveness: None,
                    };
                    if self.owner.processor.data_wait_recovery() {
                        let state = gate
                            .data_gap_state(failure.generation)
                            .ok_or(Cause::Binding)?;
                        if let Some(physical) = state.failure {
                            let mut update = self.update();
                            update.transport_failure = Some(physical);
                            return Ok(update);
                        }
                        self.owner.event(serde_json::json!({"event":"primary_data_wait_started",
                            "generation":failure.generation,"reason":"CanonicalBoundaryOrRecentTailUnproven",
                            "physical_reconnect":false})).map_err(Cause::Coordinator)?;
                        if let Some(evidence) =
                            self.owner.processor.ws_data_gap_evidence(state.revision)
                        {
                            self.owner.event(evidence).map_err(Cause::Coordinator)?;
                        }
                    } else {
                        self.socket_fault.record(failure.clone());
                    }
                    let mut update = self.update();
                    update.transport_failure = Some(failure);
                    return Ok(update);
                }
            }
        }
        let audit = if self.bootstrap.pending() || proof.is_some() || self.recovering {
            None
        } else {
            self.owner.take_audit().map_err(Cause::Coordinator)?
        };
        let mut result = self.update();
        result.bootstrap = bootstrap;
        result.proof = proof;
        result.audit = audit;
        result.subscription = subscription;
        Ok(result)
    }
    fn update(&self) -> Update {
        let now = crate::timing::now_ms();
        let next_wake_ms = self
            .owner
            .processor
            .next_action_ms()
            .unwrap_or(now.saturating_add(60000));
        Update {
            episode: None,
            attempt: None,
            subscription: None,
            recovery_plan: None,
            recovery_error: None,
            recovery_wait: None,
            recovery_ready: self.owner.processor.ws_historical_recovery()
                && self.owner.recovery_completion.is_some(),
            completed_recovery_sha256: None,
            transport_failure: None,
            bootstrap: None,
            proof: None,
            audit: None,
            data: self.owner.processor.last_live_data(),
            bootstrap_pending: self.bootstrap.pending(),
            committed: self.owner.processor.counters().committed,
            next_wake_ms,
            finalized: false,
            eod_prepared: false,
            journal_seal: None,
        }
    }
    fn finalize(&mut self, facts: crate::primary::eod::FinishFacts) -> Result<Update, Cause> {
        if self.owner.session.has_models() {
            self.owner
                .event(serde_json::json!({"event":"primary_models_final_frontier",
                "source_committed":self.owner.processor.counters().committed,
                "next_label_ms":self.owner.processor.next_label_ms(),"held":self.owner.failed,
                "at_ms":crate::timing::now_ms(),"writer_join_attested":false}))
                .map_err(Cause::Coordinator)?;
        }
        let ready = facts.requested_eod
            && facts.io_drained
            && facts.continuity_resolved
            && facts.recovery_resolved
            && self.faults.first().is_none()
            && self.gate.as_ref().is_some_and(|g| g.healthy())
            && !self.recovering
            && !self.bootstrap.pending()
            && !self.owner.failed
            && self.owner.processor.eod_complete(crate::timing::now_ms())
            && self.owner.session.eod_model_ready();
        let snapshot = self.owner.session.eod_snapshot();
        let recorded=self.owner.event(facts.event).is_ok()&&self.owner.event(serde_json::json!({"event":"primary_eod_prepared",
            "prepared":ready,"at_ms":crate::timing::now_ms(),"model":snapshot,"writer_join_attested":false,
            "calendar_sha256":self.owner.processor.calendar().map(|c|c.sha256()),"canonical_labels":self.owner.processor.counters().committed,
            "audit_status":self.owner.audit_status()})).is_ok();
        let finish = self.owner.session.finish(if ready && recorded {
            crate::primary::eod::PREPARED_REASON
        } else {
            "INCOMPLETE_primary_bounded_stop"
        });
        let mut update = self.update();
        update.finalized = recorded && matches!(finish, Ok(()) | Err(SessionError::Incomplete));
        update.eod_prepared = ready && recorded && finish.is_ok();
        if update.eod_prepared {
            update.journal_seal = self.owner.session.journal_seal();
        }
        Ok(update)
    }
}
fn resolution_error(failure: Option<&crate::socket::primary::Failure>) -> super::reconnect::Error {
    match failure.map(|failure| failure.kind) {
        Some(crate::socket::primary::Kind::Auth(class)) => super::reconnect::Error::Auth(class),
        Some(kind) => super::reconnect::Error::Transport(kind),
        None => super::reconnect::Error::Source(PrimaryError::ReferenceUnproven),
    }
}
struct Command {
    op: Operation,
    reply: oneshot::Sender<Result<Update, Cause>>,
    #[cfg(test)]
    clock: Option<(i64, std::time::Instant)>,
}
pub(super) struct Writer {
    tx: Option<mpsc::Sender<Command>>,
    join: Option<JoinHandle<()>>,
    stop: StopHandle,
    stop_deadline: Option<Instant>,
    faults: Faults,
    stop_budget: Duration,
}
impl Writer {
    #[cfg(test)]
    pub fn spawn(
        owner: Coordinator,
        reference_start: DateTime<Utc>,
        stop: StopHandle,
        faults: Faults,
        socket_fault: FailureSlot,
    ) -> Result<Self, Cause> {
        Self::spawn_mode(owner, reference_start, stop, faults, socket_fault, None)
    }
    pub fn spawn_mode(
        mut owner: Coordinator,
        reference_start: DateTime<Utc>,
        stop: StopHandle,
        faults: Faults,
        socket_fault: FailureSlot,
        gate: Option<crate::primary::ContinuityGate>,
    ) -> Result<Self, Cause> {
        if let Some(calendar) = owner.processor.calendar() {
            calendar
                .validate_entry(crate::timing::now_ms())
                .map_err(Cause::Calendar)?;
            if reference_start.timestamp_millis() != calendar.reference_start_ms() {
                return Err(Cause::Binding);
            }
        }
        if let Some(gate) = &gate {
            owner.session.bind_live_continuity(gate.clone());
        }
        let (config, _) = owner.session.primary_runtime().ok_or(Cause::Binding)?;
        let run_deadline_ms = config.config().run_deadline_utc_ms;
        let stop_budget = Duration::from_millis(config.config().stop_timeout_ms.min(15000));
        let bootstrap = BootstrapBridge::new(
            owner.processor.config_sha256().into(),
            owner.processor.generation(),
            reference_start,
            config.config().run_deadline_utc_ms,
        )
        .map_err(Cause::Coordinator)?;
        let recovery_budget = super::reconnect::RunBudget::new(&owner).map_err(Cause::Recovery)?;
        let mut core = Core {
            gate,
            reference_start,
            run_deadline_ms,
            recovery_budget,
            recovering: false,
            recovery_reads: 0,
            expected_revision: 0,
            seen_revision: 0,
            owner,
            bootstrap,
            stop: stop.clone(),
            faults: faults.clone(),
            socket_fault,
        };
        let (tx, mut rx) = mpsc::channel::<Command>(1);
        let worker_stop = stop.clone();
        let worker_faults = faults.clone();
        let join = tokio::task::spawn_blocking(move || {
            while let Some(command) = rx.blocking_recv() {
                // Test-only barrier AFTER a real submitted Frame has executed,
                // but BEFORE its owned reply is sent. No production seam.
                #[cfg(test)]
                let frame_pause = if matches!(&command.op, Operation::Frame(_))
                    && core
                        .owner
                        .frame_reply_pause
                        .as_ref()
                        .is_some_and(|(at, _)| command.clock.is_some_and(|clock| clock.0 >= *at))
                {
                    core.owner.frame_reply_pause.take().map(|(_, gate)| gate)
                } else {
                    None
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    #[cfg(test)]
                    if let Some(clock) = command.clock {
                        return crate::timing::with_fixture_sample(clock, || core.step(command.op));
                    }
                    core.step(command.op)
                }))
                .unwrap_or(Err(Cause::WriterPanic));
                #[cfg(test)]
                if let Some(gate) = frame_pause {
                    gate.wait(&worker_stop);
                }
                if let Err(cause) = &result {
                    if !matches!(cause, Cause::RequestedStop) {
                        worker_faults.record(cause.clone());
                    }
                    worker_stop.stop();
                }
                if command.reply.send(result).is_err() {
                    worker_stop.stop();
                    break;
                }
            }
            // Core/session destruction is part of the blocking task join.
            drop(core);
        });
        Ok(Self {
            tx: Some(tx),
            join: Some(join),
            stop,
            stop_deadline: None,
            faults,
            stop_budget,
        })
    }
    pub fn drain_deadline(&mut self) -> Instant {
        *self
            .stop_deadline
            .get_or_insert_with(|| Instant::now() + self.stop_budget)
    }
    pub fn stop_budget_ms(&self) -> u64 {
        self.stop_budget.as_millis() as u64
    }
    pub fn cap_drain_deadline(&mut self, limit: Instant) -> Instant {
        let deadline = self.drain_deadline().min(limit);
        self.stop_deadline = Some(deadline);
        deadline
    }
    pub async fn call(&mut self, op: Operation) -> Result<Update, Cause> {
        self.call_until(op, None).await
    }
    pub async fn call_until(
        &mut self,
        op: Operation,
        deadline: Option<Instant>,
    ) -> Result<Update, Cause> {
        let (reply, mut response) = oneshot::channel();
        let command = Command {
            op,
            reply,
            #[cfg(test)]
            clock: crate::timing::fixture_sample(),
        };
        self.tx
            .as_ref()
            .ok_or(Cause::WriterGone)?
            .try_send(command)
            .map_err(|_| Cause::WriterGone)?;
        tokio::select! {biased;
            result=&mut response=>result.map_err(|_|Cause::WriterGone)?,
            _=self.stop.cancelled()=>{
                let deadline=self.drain_deadline();
                tokio::time::timeout_at(deadline,&mut response).await.map_err(|_|Cause::DrainTimeout)?.map_err(|_|Cause::WriterGone)?
            }
            _=async{match deadline{Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await}}=>{
                self.stop.stop();let stop_deadline=self.drain_deadline();
                let _=tokio::time::timeout_at(stop_deadline,&mut response).await;
                Err(Cause::Recovery(super::reconnect::Error::EpisodeDeadline))
            }
        }
    }
    pub async fn join(&mut self, deadline: Instant) -> bool {
        self.tx.take();
        let Some(mut join) = self.join.take() else {
            return false;
        };
        match tokio::time::timeout_at(deadline, &mut join).await {
            Ok(Ok(())) => true,
            _ => {
                self.faults.record(Cause::DrainTimeout);
                join.abort();
                false
            }
        }
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.stop.stop();
        self.tx.take();
        if let Some(join) = self.join.take() {
            join.abort();
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    #[test]
    fn a_late_malformed_frame_or_negative_auth_is_not_a_retryable_reference_gap() {
        let malformed = crate::socket::primary::Failure {
            generation: 2,
            kind: crate::socket::primary::Kind::Source(PrimaryError::Numeric),
            liveness: None,
        };
        assert_eq!(
            resolution_error(Some(&malformed)),
            super::super::reconnect::Error::Transport(malformed.kind)
        );
        let denied = crate::socket::primary::Failure {
            generation: 2,
            kind: crate::socket::primary::Kind::Auth(crate::auth::PrimaryAuthClass::Authentication),
            liveness: None,
        };
        assert_eq!(
            resolution_error(Some(&denied)),
            super::super::reconnect::Error::Auth(crate::auth::PrimaryAuthClass::Authentication)
        );
    }
    pub struct Pause {
        pub entered: std::sync::atomic::AtomicBool,
        pub release: std::sync::atomic::AtomicBool,
        pub ignore_stop: bool,
    }
    impl Pause {
        pub fn new() -> Self {
            Self {
                entered: std::sync::atomic::AtomicBool::new(false),
                release: std::sync::atomic::AtomicBool::new(false),
                ignore_stop: false,
            }
        }
        pub(super) fn wait(&self, stop: &StopHandle) {
            self.entered
                .store(true, std::sync::atomic::Ordering::SeqCst);
            while !self.release.load(std::sync::atomic::Ordering::SeqCst)
                && (self.ignore_stop || !stop.is_stopped())
            {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
