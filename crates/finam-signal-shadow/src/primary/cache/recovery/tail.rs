//! V6 request lifecycle. Assessment is read-only; Pending consumes only the
//! request, never canonical/model/reference/retained authority.
use super::*;

pub const MAX_REST_READS: u8 = 4;
pub const REST_BACKOFF_MS: [u64; 3] = [1000, 2000, 4000];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRequestScope {
    pub episode: u8,
    pub ordinal: u8,
    pub deadline_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoveryWait {
    pub reason: &'static str,
    pub label_ms: i64,
    pub known_m1_ms: Vec<i64>,
    pub missing_known_m1_ms: Vec<i64>,
    pub empty_last_bucket: bool,
    pub assessed_at_ms: i64,
}

pub enum RecoveryOutcome {
    Ready(AdmittedRecovery),
    DataNotReady(RecoveryWait),
}

impl PrimaryProcessor {
    /// A completed transient failure consumes a request, not a source proof.
    /// The driver validates its real command receipt before calling this.
    pub(crate) fn retire_failed_recovery_request(
        &mut self,
        plan: &RecoveryPlan,
        now: i64,
    ) -> Result<(), PrimaryError> {
        self.clock(now)?;
        if !self.policy.data_wait_recovery() || !self.recovery_request_active(plan) {
            return Err(PrimaryError::Continuity);
        }
        self.recovery_pending
            .as_mut()
            .ok_or(PrimaryError::Continuity)?
            .responded = true;
        Ok(())
    }
    pub(crate) fn rest_tail_recovery(&self) -> bool {
        self.policy.rest_tail_recovery()
    }
    pub(crate) fn recovery_request_active(&self, plan: &RecoveryPlan) -> bool {
        self.matches_recovery(plan)
            && self.next == plan.next_index
            && self
                .recovery_pending
                .as_ref()
                .is_some_and(|p| !p.admitted && !p.responded)
    }
    pub fn issue_recovery_request(
        &mut self,
        cutover_ms: i64,
        now: i64,
        scope: RecoveryRequestScope,
        previous: Option<&RecoveryPlan>,
    ) -> Result<RecoveryPlan, PrimaryError> {
        self.clock(now)?;
        if !self.policy.rest_tail_recovery()
            || !(1..=3).contains(&scope.episode)
            || !(1..=MAX_REST_READS).contains(&scope.ordinal)
            || scope.deadline_ms <= now
            || scope.deadline_ms > self.run_deadline_ms
        {
            return Err(PrimaryError::Configuration);
        }
        if let Some(old) = previous {
            let old_scope = old.scope.ok_or(PrimaryError::Continuity)?;
            if !self.matches_recovery(old)
                || self.next != old.next_index
                || old.request.end().timestamp_millis() != cutover_ms
                || self
                    .recovery_pending
                    .as_ref()
                    .map_or(true, |p| p.admitted || !p.responded)
                || old_scope.episode != scope.episode
                || old_scope.ordinal.checked_add(1) != Some(scope.ordinal)
                || old_scope.deadline_ms != scope.deadline_ms
            {
                return Err(PrimaryError::Continuity);
            }
            self.recovery_pending = None;
        }
        self.plan_recovery_impl(cutover_ms, now, Some(scope))
    }

    /// Do not call the mutating admission until EVERY bucket has passed.
    /// Opaque AdmittedClosedM1Snapshot has already checked raw/hash/OHLCV.
    fn tail_assessment(
        &self,
        plan: &RecoveryPlan,
        source: &AdmittedClosedM1Snapshot,
        now: i64,
    ) -> Result<Option<RecoveryWait>, PrimaryError> {
        let evidence = source.evidence();
        if !self.matches_recovery(plan)
            || self.next != plan.next_index
            || plan.scope.is_none()
            || self
                .recovery_pending
                .as_ref()
                .map_or(true, |p| p.admitted || p.responded)
            || evidence.start != plan.request.start()
            || evidence.end != plan.request.end()
            || evidence.parts.len() != 1
        {
            return Err(PrimaryError::Continuity);
        }
        let part = &evidence.parts[0];
        if part.requested_at.timestamp_millis() < plan.issued_at_ms
            || part.received_at.timestamp_millis() > now
            || part.received_at < part.requested_at
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        if source.bars().any(|bar| bar.instrument != self.instrument) {
            return Err(PrimaryError::Identity);
        }
        if plan.startup_anchor {
            let start = self
                .calendar
                .as_ref()
                .ok_or(PrimaryError::ReferenceUnproven)?
                .reference_start_ms();
            if source.bars().next().is_none() {
                return Err(PrimaryError::ReferenceUnproven);
            }
            for (key, old) in self.reference.range(start..self.required[0]) {
                if !source.bars().any(|r| {
                    r.open_ts.timestamp_millis() == *key
                        && super::super::super::revisions::same_slot(old, r)
                }) {
                    return Err(PrimaryError::ReferenceConflict);
                }
            }
        }
        let mut wait = None;
        for index in plan.start_index..plan.end_index {
            let label = self.required[index];
            let rows: Vec<_> = source
                .bars()
                .filter(|r| (label..label + 600000).contains(&r.open_ts.timestamp_millis()))
                .collect();
            if !rows.is_empty() {
                // Per-row admission alone does not establish valid aggregate
                // arithmetic (e.g. Decimal volume sum overflow). Check every
                // nonempty bucket before returning Pending for the final tail.
                let bucket = source
                    .bucket(at(label)?)
                    .map_err(|_| PrimaryError::Numeric)?;
                runtime_bar(bucket.bar())?;
            }
            if index < self.next {
                let old: Vec<_> = self
                    .committed
                    .range(label..label + 600000)
                    .map(|(_, b)| b)
                    .collect();
                if old.len() != rows.len()
                    || old
                        .iter()
                        .zip(&rows)
                        .any(|(a, b)| !super::super::super::revisions::same_slot(a, b))
                {
                    return Err(PrimaryError::CanonicalConflict);
                }
                continue;
            }
            let known: BTreeSet<_> = self
                .cache
                .range(label..label + 600000)
                .map(|(t, _)| *t)
                .chain(
                    self.retained_ws
                        .range(label..label + 600000)
                        .map(|(t, _)| *t),
                )
                .collect();
            let actual: BTreeSet<_> = rows.iter().map(|r| r.open_ts.timestamp_millis()).collect();
            let missing: Vec<_> = known.difference(&actual).copied().collect();
            if rows.is_empty() || !missing.is_empty() {
                // Only the most recent unpublished bucket and a missing suffix
                // of KNOWN slots, not an invented dense grid, may be pending.
                if index + 1 != plan.end_index
                    || label + 600000 != plan.request.end().timestamp_millis()
                    || missing
                        .first()
                        .is_some_and(|first| actual.range(*first..).next().is_some())
                {
                    return Err(PrimaryError::CanonicalConflict);
                }
                wait = Some(RecoveryWait {
                    reason: "MissingRecentUnpublishedTail",
                    label_ms: label,
                    known_m1_ms: known.into_iter().collect(),
                    missing_known_m1_ms: missing,
                    empty_last_bucket: rows.is_empty(),
                    assessed_at_ms: now,
                });
            }
        }
        // Expired responses never become canonical, even when structurally
        // valid. A new request (not a retimed old answer) is necessary.
        if now > plan.deadline_ms || part.received_at.timestamp_millis() > plan.deadline_ms {
            return Ok(Some(RecoveryWait {
                reason: "RequestExpired",
                label_ms: plan.request.end().timestamp_millis() - 600000,
                known_m1_ms: vec![],
                missing_known_m1_ms: vec![],
                empty_last_bucket: false,
                assessed_at_ms: now,
            }));
        }
        if let Some(value) = wait.as_mut() {
            let first = self
                .required
                .get(self.next)
                .copied()
                .unwrap_or(plan.request.end().timestamp_millis());
            value.known_m1_ms = self
                .cache
                .range(first..plan.request.end().timestamp_millis())
                .map(|(t, _)| *t)
                .chain(
                    self.retained_ws
                        .range(first..plan.request.end().timestamp_millis())
                        .map(|(t, _)| *t),
                )
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
        }
        Ok(wait)
    }

    pub fn prepare_recovery_request(
        &mut self,
        plan: &RecoveryPlan,
        source: &AdmittedClosedM1Snapshot,
        now: i64,
    ) -> Result<RecoveryOutcome, PrimaryError> {
        self.clock(now)?;
        if !self.policy.rest_tail_recovery() {
            return Err(PrimaryError::Configuration);
        }
        let assessment = match self.tail_assessment(plan, source, now) {
            Ok(value) => value,
            Err(
                error @ (PrimaryError::CanonicalConflict
                | PrimaryError::Identity
                | PrimaryError::ReferenceConflict
                | PrimaryError::Numeric),
            ) => return self.hold(error),
            Err(error) => return Err(error),
        };
        if let Some(wait) = assessment {
            self.recovery_pending
                .as_mut()
                .ok_or(PrimaryError::Continuity)?
                .responded = true;
            return Ok(RecoveryOutcome::DataNotReady(wait));
        }
        self.apply_recovery(plan, source, now)
            .map(RecoveryOutcome::Ready)
    }
}

#[cfg(test)]
mod tests;
