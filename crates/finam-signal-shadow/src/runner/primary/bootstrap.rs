//! One independent reference GET per physical generation. Neither its start nor
//! end is inferred from received WS row timestamps/counts. Pending data retains
//! the original receive stamp and hash; a failed proof cannot reopen bootstrap.
use super::*;
use chrono::{DateTime, Utc};

#[derive(Clone)]
pub struct BootstrapTicket {
    owner: Arc<()>,
    plan: ClosedM1RequestPlan,
    deadline_ms: i64,
    issued_at_ms: i64,
    plan_sha256: String,
}
impl BootstrapTicket {
    pub fn plan(&self) -> &ClosedM1RequestPlan {
        &self.plan
    }
    pub fn deadline_ms(&self) -> i64 {
        self.deadline_ms
    }
    pub fn plan_sha256(&self) -> &str {
        &self.plan_sha256
    }
}
pub struct BootstrapBridge {
    config_sha256: String,
    generation: u64,
    reference_start: DateTime<Utc>,
    run_deadline_ms: i64,
    pending: Option<(WireFrame, BootstrapTicket)>,
    attempted: bool,
    failed: bool,
}
impl BootstrapBridge {
    /// reference_start must come from independently fixed raw-session history,
    /// including the preceding actual trading session (not calendar "last 5h").
    pub fn new(
        config_sha256: String,
        generation: u64,
        reference_start: DateTime<Utc>,
        run_deadline_ms: i64,
    ) -> Result<Self, CoordinatorError> {
        if generation == 0
            || reference_start.timestamp() % 60 != 0
            || reference_start.timestamp_subsec_nanos() != 0
            || config_sha256.len() != 64
        {
            return Err(CoordinatorError::Binding);
        }
        Ok(Self {
            config_sha256,
            generation,
            reference_start,
            run_deadline_ms,
            pending: None,
            attempted: false,
            failed: false,
        })
    }
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn on_frame(
        &mut self,
        owner: &mut Coordinator,
        frame: WireFrame,
    ) -> Result<Option<BootstrapTicket>, CoordinatorError> {
        if self.failed {
            return Err(CoordinatorError::Held);
        }
        if owner.processor.config_sha256() != self.config_sha256
            || frame.generation() != self.generation
        {
            return Err(CoordinatorError::Binding);
        }
        if self.pending.is_some() {
            owner.processor.break_continuity();
            owner.failed = true;
            self.failed = true;
            return Err(CoordinatorError::Source(PrimaryError::Continuity));
        }
        match owner.on_frame(frame.clone()) {
            Ok(()) => Ok(None),
            Err(CoordinatorError::Source(PrimaryError::ReferenceUnproven)) => {
                if self.attempted {
                    self.failed = true;
                    owner.failed = true;
                    return Err(CoordinatorError::Held);
                }
                let now = crate::timing::now_ms();
                let end = DateTime::from_timestamp_millis(now.div_euclid(60000) * 60000)
                    .ok_or(CoordinatorError::Binding)?;
                let plan = ClosedM1RequestPlan::new(self.reference_start, end, vec![end])
                    .map_err(|_| CoordinatorError::Binding)?;
                plan.preflight(
                    DateTime::from_timestamp_millis(now).ok_or(CoordinatorError::Binding)?,
                )
                .map_err(|_| CoordinatorError::Binding)?;
                let deadline_ms = (now + 5000).min(self.run_deadline_ms);
                if deadline_ms <= now {
                    return Err(CoordinatorError::Rest(RestFailure::Timeout));
                }
                let body = serde_json::to_vec(&(
                    &self.config_sha256,
                    self.generation,
                    self.reference_start,
                    end,
                    now,
                    deadline_ms,
                ))
                .map_err(|_| CoordinatorError::Binding)?;
                let ticket = BootstrapTicket {
                    owner: Arc::new(()),
                    plan,
                    deadline_ms,
                    issued_at_ms: now,
                    plan_sha256: crate::seed::digest(&body),
                };
                owner.event(serde_json::json!({"event":"primary_bootstrap_proof_required","generation":self.generation,
                    "frame_sha256":frame.sha256(),"original_received_at_ms":frame.received_at_ms(),
                    "start":self.reference_start,"end":end,"plan_sha256":ticket.plan_sha256,"deadline_ms":deadline_ms}))?;
                self.attempted = true;
                self.pending = Some((frame, ticket.clone()));
                Ok(Some(ticket))
            }
            Err(error) => Err(error),
        }
    }
    pub fn complete(
        &mut self,
        owner: &mut Coordinator,
        ticket: BootstrapTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<(), CoordinatorError> {
        let Some((_, expected)) = &self.pending else {
            return Err(CoordinatorError::Ticket);
        };
        if !Arc::ptr_eq(&expected.owner, &ticket.owner) {
            return Err(CoordinatorError::Ticket);
        }
        let (frame, _) = self.pending.take().ok_or(CoordinatorError::Ticket)?;
        self.failed = true;
        let outcome = Self::complete_inner(owner, ticket, frame, result);
        if outcome.is_err() {
            owner.failed = true;
            owner.processor.break_continuity();
        } else {
            self.failed = false;
        }
        outcome
    }
    pub(crate) fn complete_recovering(
        &mut self,
        owner: &mut Coordinator,
        ticket: BootstrapTicket,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<(), CoordinatorError> {
        if owner.processor.gap_unresolved()
            && matches!(
                &result,
                Err(RestFailure::Unavailable | RestFailure::Timeout)
            )
        {
            let Some((_, expected)) = &self.pending else {
                return Err(CoordinatorError::Ticket);
            };
            if !Arc::ptr_eq(&expected.owner, &ticket.owner) {
                return Err(CoordinatorError::Ticket);
            }
            self.pending.take();
            self.failed = true;
            owner.processor.break_continuity();
            return Err(CoordinatorError::Rest(result.expect_err("matched error")));
        }
        self.complete(owner, ticket, result)
    }
    fn complete_inner(
        owner: &mut Coordinator,
        ticket: BootstrapTicket,
        frame: WireFrame,
        result: Result<AdmittedClosedM1Snapshot, RestFailure>,
    ) -> Result<(), CoordinatorError> {
        let source = result.map_err(CoordinatorError::Rest)?;
        let now = crate::timing::now_ms();
        if now > ticket.deadline_ms
            || source.evidence().start != ticket.plan.start()
            || source.evidence().end != ticket.plan.end()
            || source.evidence().parts.len() != 1
            || source.evidence().parts.iter().any(|p| {
                p.requested_at.timestamp_millis() < ticket.issued_at_ms
                    || p.received_at.timestamp_millis() > now
                    || p.received_at.timestamp_millis() > ticket.deadline_ms
            })
        {
            return Err(CoordinatorError::Rest(RestFailure::Integrity));
        }
        owner
            .session
            .record_primary_reference(&source)
            .map_err(CoordinatorError::Session)?;
        let result = owner.processor.add_reference(&source);
        owner.source_result(result)?;
        let generation = frame.generation();
        let result = owner.processor.resume_bootstrap_frame(frame, now);
        owner.source_result(result)?;
        owner.event(
            serde_json::json!({"event":"primary_bootstrap_proof_applied","generation":generation,
            "plan_sha256":ticket.plan_sha256,"source_sha256":source.sha256(),
            "history_proves_data_readiness":false}),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
