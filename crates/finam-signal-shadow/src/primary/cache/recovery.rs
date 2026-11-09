//! REST-bound, one-shot historical repair. All overlap and missing buckets are
//! validated before a callback. A fresh WS generation alone cannot mint repair.
use super::*;
use broker_finam::sparse_m10::ClosedM1RequestPlan;
use chrono::{Datelike, Timelike};

#[derive(Clone, Debug)]
pub struct RecoveryPlan {
    gap: Arc<()>,
    owner: Arc<()>,
    config_sha256: String,
    generation: u64,
    start_index: usize,
    next_index: usize,
    end_index: usize,
    startup_anchor: bool,
    issued_at_ms: i64,
    deadline_ms: i64,
    request: ClosedM1RequestPlan,
    sha256: String,
}
impl RecoveryPlan {
    pub fn request(&self) -> &ClosedM1RequestPlan {
        &self.request
    }
    pub fn deadline_ms(&self) -> i64 {
        self.deadline_ms
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn overlap_labels(&self) -> usize {
        self.next_index - self.start_index
    }
    pub fn missed_labels(&self) -> usize {
        self.end_index - self.next_index
    }
    pub fn startup_anchor(&self) -> bool {
        self.startup_anchor
    }
    pub fn issued_at_ms(&self) -> i64 {
        self.issued_at_ms
    }
}
pub(super) struct Pending {
    owner: Arc<()>,
    end_index: usize,
    admitted: bool,
}

#[derive(Debug, Serialize)]
pub struct RecoveryReceipt {
    #[serde(flatten)]
    pub canonical: CanonicalReceipt,
    pub recovery_plan_sha256: String,
    pub cutover_ms: i64,
    pub actual_requested_at_ms: i64,
    pub actual_received_at_ms: i64,
    pub virtual_callback_at_ms: i64,
    pub replay_deadline_ms: i64,
}
/// No public constructor, Clone or Deserialize. This is never a live input.
///
/// ```compile_fail
/// use finam_signal_shadow::primary::RecoveryInput;
/// let forged: RecoveryInput = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use finam_signal_shadow::primary::RecoveryInput;
/// fn duplicate(input: &RecoveryInput) -> RecoveryInput { (*input).clone() }
/// ```
#[derive(Debug)]
pub struct RecoveryInput {
    gap: Arc<()>,
    batch: Arc<()>,
    receipt: RecoveryReceipt,
    bar: HybridRuntimeBarEvent,
}
impl RecoveryInput {
    pub fn receipt(&self) -> &RecoveryReceipt {
        &self.receipt
    }
    pub fn model_bar(&self) -> &HybridRuntimeBarEvent {
        &self.bar
    }
    pub fn provenance_sha256(&self) -> Result<String, PrimaryError> {
        serde_json::to_vec(&self.receipt)
            .map(|v| crate::seed::digest(&v))
            .map_err(|_| PrimaryError::Configuration)
    }
}
pub struct AdmittedRecovery {
    pub(crate) inputs: Vec<RecoveryInput>,
    pub(crate) completion: RecoveryCompletion,
}
impl AdmittedRecovery {
    pub fn steps(&self) -> usize {
        self.inputs.len()
    }
}
/// May be retained while waiting for admitted fresh DATA. Not a boolean escape.
pub struct RecoveryCompletion {
    gap: Arc<()>,
    owner: Arc<()>,
    config_sha256: String,
    generation: u64,
    end_index: usize,
    source_sha256: String,
    cutover_ms: i64,
}
pub struct ResolvedRecovery {
    resolution: Arc<()>,
    config_sha256: String,
    generation: u64,
    physical_owner: Arc<()>,
    cutover_ms: i64,
    source_sha256: String,
}
impl ResolvedRecovery {
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn config_sha256(&self) -> &str {
        &self.config_sha256
    }
    pub fn cutover_ms(&self) -> i64 {
        self.cutover_ms
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub(crate) fn matches_subscription(
        &self,
        subscription: &crate::socket::primary::Subscription,
    ) -> bool {
        self.generation == subscription.generation()
            && Arc::ptr_eq(&self.physical_owner, &subscription.owner)
    }
}

impl PrimaryProcessor {
    pub fn plan_recovery(
        &mut self,
        cutover_ms: i64,
        now: i64,
    ) -> Result<RecoveryPlan, PrimaryError> {
        self.clock(now)?;
        if !self.gap_unresolved
            || !self.physical_bound()
            || self.in_flight.is_some()
            || self.recovery_pending.is_some()
        {
            return Err(PrimaryError::Continuity);
        }
        let next = self
            .next_label_ms()
            .ok_or(PrimaryError::ReferenceUnproven)?;
        if cutover_ms % 600000 != 0
            || cutover_ms != now.div_euclid(600000) * 600000
            || cutover_ms < next
            || cutover_ms > now
            || cutover_ms > self.run_deadline_ms
            || cutover_ms - next > self.policy.recovery_max_gap_ms as i64
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let end_index = self.required.partition_point(|label| *label < cutover_ms);
        let start_index = self.next.saturating_sub(self.policy.recovery_overlap_m10);
        let labels = &self.required[start_index..end_index];
        let next_label = self
            .required
            .get(end_index)
            .copied()
            .unwrap_or_else(|| self.required.last().unwrap() + 600000);
        if next_label != cutover_ms || labels.windows(2).any(|p| p[1] - p[0] != 600000) {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let startup_anchor = labels.is_empty();
        let start = if startup_anchor {
            // No model label was committed or missed. Still require a fresh,
            // nonempty closed-history proof over the independently fixed
            // calendar reference, never an empty GET or a DATA-only latch reset.
            let calendar = self
                .calendar
                .as_ref()
                .ok_or(PrimaryError::ReferenceUnproven)?;
            if self.next != 0 || end_index != 0 || cutover_ms != self.required[0] {
                return Err(PrimaryError::ReferenceUnproven);
            }
            calendar.reference_start_ms()
        } else {
            self.required[start_index]
        };
        let local = at(start)? + chrono::Duration::hours(3);
        let last = at(cutover_ms - 1)? + chrono::Duration::hours(3);
        if !startup_anchor
            && (local.date_naive() != last.date_naive()
                || local.weekday().number_from_monday() > 5
                || local.hour() * 60 + local.minute() < 420
                || last.hour() * 60 + last.minute() >= 1430)
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let request = ClosedM1RequestPlan::new(at(start)?, at(cutover_ms)?, vec![at(cutover_ms)?])
            .map_err(|_| PrimaryError::ReferenceUnproven)?;
        request
            .preflight(at(now)?)
            .map_err(|_| PrimaryError::ReferenceUnproven)?;
        let deadline_ms = (now + self.policy.proof_timeout_ms as i64).min(self.run_deadline_ms);
        if deadline_ms <= now {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let gap = self.gap_owner.clone().ok_or(PrimaryError::Continuity)?;
        let owner = Arc::new(());
        let body = serde_json::to_vec(&(
            &self.config_sha256,
            &self.policy_sha256,
            self.generation,
            start,
            cutover_ms,
            self.next,
            now,
            deadline_ms,
            startup_anchor,
            self.calendar.as_ref().map(|c| c.sha256()),
        ))
        .map_err(|_| PrimaryError::Configuration)?;
        self.recovery_pending = Some(Pending {
            owner: owner.clone(),
            end_index,
            admitted: false,
        });
        Ok(RecoveryPlan {
            gap,
            owner,
            config_sha256: self.config_sha256.clone(),
            generation: self.generation,
            start_index,
            next_index: self.next,
            end_index,
            startup_anchor,
            issued_at_ms: now,
            deadline_ms,
            request,
            sha256: crate::seed::digest(&body),
        })
    }

    pub fn admit_recovery(
        &mut self,
        plan: &RecoveryPlan,
        source: &AdmittedClosedM1Snapshot,
        now: i64,
    ) -> Result<AdmittedRecovery, PrimaryError> {
        self.clock(now)?;
        if !self.matches_recovery(plan)
            || self.recovery_pending.as_ref().is_some_and(|p| p.admitted)
            || self.next != plan.next_index
        {
            return Err(PrimaryError::Continuity);
        }
        let evidence = source.evidence();
        if now > plan.deadline_ms
            || evidence.start != plan.request.start()
            || evidence.end != plan.request.end()
            || evidence.parts.len() != 1
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let part = &evidence.parts[0];
        let received = part.received_at.timestamp_millis();
        let requested = part.requested_at.timestamp_millis();
        if requested < plan.issued_at_ms
            || received > now
            || received > plan.deadline_ms
            || received < requested
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        // Validate ALL overlap and all missing buckets before minting any step.
        let mut inputs = Vec::new();
        if plan.startup_anchor {
            let calendar = self
                .calendar
                .as_ref()
                .ok_or(PrimaryError::ReferenceUnproven)?;
            if plan.next_index != 0
                || plan.end_index != 0
                || self.next != 0
                || evidence.start.timestamp_millis() != calendar.reference_start_ms()
                || evidence.end.timestamp_millis() != self.required[0]
                || source.bars().next().is_none()
            {
                return Err(PrimaryError::ReferenceUnproven);
            }
            let actual: BTreeMap<_, _> = source
                .bars()
                .map(|r| (r.open_ts.timestamp_millis(), r))
                .collect();
            for (key, old) in self
                .reference
                .range(calendar.reference_start_ms()..self.required[0])
            {
                if actual.get(key).map_or(true, |r| {
                    !super::super::revisions::compatible(old, r, self.policy.observed_bo())
                }) {
                    return self.hold(PrimaryError::ReferenceConflict);
                }
            }
            self.add_reference(source)?;
        }
        for index in plan.start_index..plan.end_index {
            let label = self.required[index];
            let bucket = source
                .bucket(at(label)?)
                .map_err(|_| PrimaryError::ReferenceUnproven)?;
            let rows = bucket.actual_m1();
            if bucket.bar().instrument != self.instrument {
                return self.hold(PrimaryError::Identity);
            }
            if index < self.next {
                let committed: Vec<_> = self
                    .committed
                    .range(label..label + 600000)
                    .map(|(_, row)| row.clone())
                    .collect();
                if committed.len() != rows.len() {
                    let missing = committed
                        .iter()
                        .find(|a| !rows.iter().any(|b| a.open_ts == b.open_ts));
                    let extra = rows
                        .iter()
                        .find(|b| !committed.iter().any(|a| a.open_ts == b.open_ts));
                    self.note_change("RECOVERY_OVERLAP", missing, extra, Some(source.sha256()))?;
                    return self.hold(PrimaryError::CanonicalConflict);
                }
                if let Some((a, b)) = committed.iter().zip(rows).find(|(a, b)| {
                    !super::super::revisions::compatible(a, b, self.policy.observed_bo())
                }) {
                    self.note_change("RECOVERY_OVERLAP", Some(a), Some(b), Some(source.sha256()))?;
                    return self.hold(PrimaryError::CanonicalConflict);
                }
                for (a, b) in committed.iter().zip(rows) {
                    self.note_change("RECOVERY_OVERLAP", Some(a), Some(b), Some(source.sha256()))?;
                }
                continue;
            }
            let cached: Vec<_> = self
                .cache
                .range(label..label + 600000)
                .map(|(_, v)| v.clone())
                .collect();
            for entry in cached {
                let Some(final_row) = rows.iter().find(|row| row.open_ts == entry.bar.open_ts)
                else {
                    self.note_change(
                        "RECOVERY_PENDING",
                        Some(&entry.bar),
                        None,
                        Some(source.sha256()),
                    )?;
                    return self.hold(PrimaryError::CanonicalConflict);
                };
                self.note_change(
                    "RECOVERY_PENDING",
                    Some(&entry.bar),
                    Some(final_row),
                    Some(source.sha256()),
                )?;
                if final_row.open != entry.bar.open
                    || final_row.high < entry.bar.high
                    || final_row.low > entry.bar.low
                    || (final_row.volume < entry.bar.volume
                        && !(self.policy.observed_bo()
                            && super::super::revisions::same_prices(&entry.bar, final_row)))
                {
                    return self.hold(PrimaryError::CanonicalConflict);
                }
            }
            let mut bar = runtime_bar(bucket.bar())?;
            bar.origin = HybridRuntimeBarOrigin::Replay;
            let receipt = RecoveryReceipt {
                canonical: CanonicalReceipt {
                    config_sha256: self.config_sha256.clone(),
                    policy_sha256: self.policy_sha256.clone(),
                    origin: "RECOVERY_BACKFILL",
                    reason: None,
                    label_ms: label,
                    close_ms: label + 600000,
                    generation: self.generation,
                    subscribed_at_ms: self.subscribed_at_ms,
                    last_component_received_at_ms: received,
                    boundary_received_at_ms: None,
                    proof_received_at_ms: Some(received),
                    rest_snapshot_sha256: Some(source.sha256().into()),
                    canonical_ready_at_ms: now,
                    minute_bitmap: bucket.minute_bitmap(),
                    canonical_digest: crate::input::canonical_digest(&bar)
                        .map_err(|_| PrimaryError::Numeric)?,
                    actual_m1: rows.to_vec(),
                    source_frame_sha256: vec![],
                },
                recovery_plan_sha256: plan.sha256.clone(),
                cutover_ms: evidence.end.timestamp_millis(),
                actual_requested_at_ms: requested,
                actual_received_at_ms: received,
                virtual_callback_at_ms: label + 600000,
                replay_deadline_ms: (now + 10000).min(self.run_deadline_ms),
            };
            inputs.push(RecoveryInput {
                gap: plan.gap.clone(),
                batch: plan.owner.clone(),
                receipt,
                bar,
            });
        }
        self.recovery_pending
            .as_mut()
            .ok_or(PrimaryError::Continuity)?
            .admitted = true;
        Ok(AdmittedRecovery {
            inputs,
            completion: RecoveryCompletion {
                gap: plan.gap.clone(),
                owner: plan.owner.clone(),
                config_sha256: self.config_sha256.clone(),
                generation: self.generation,
                end_index: plan.end_index,
                source_sha256: source.sha256().into(),
                cutover_ms: evidence.end.timestamp_millis(),
            },
        })
    }
    fn matches_recovery(&self, plan: &RecoveryPlan) -> bool {
        self.gap_unresolved
            && self.generation == plan.generation
            && self.config_sha256 == plan.config_sha256
            && self
                .gap_owner
                .as_ref()
                .is_some_and(|gap| Arc::ptr_eq(gap, &plan.gap))
            && self.recovery_pending.as_ref().is_some_and(|pending| {
                Arc::ptr_eq(&pending.owner, &plan.owner) && pending.end_index == plan.end_index
            })
    }
    pub(crate) fn recovered(
        &mut self,
        input: RecoveryInput,
        published_at_ms: i64,
    ) -> Result<(), PrimaryError> {
        self.clock(published_at_ms)?;
        let receipt = &input.receipt.canonical;
        if !self.gap_unresolved
            || self.next_label_ms() != Some(receipt.label_ms)
            || self.generation != receipt.generation
            || !self
                .gap_owner
                .as_ref()
                .is_some_and(|gap| Arc::ptr_eq(gap, &input.gap))
            || !self.recovery_pending.as_ref().is_some_and(|p| {
                p.admitted && Arc::ptr_eq(&p.owner, &input.batch) && self.next < p.end_index
            })
            || published_at_ms < receipt.canonical_ready_at_ms
            || published_at_ms > input.receipt.replay_deadline_ms
        {
            return self.hold(PrimaryError::Continuity);
        }
        self.cache
            .retain(|key, _| *key < receipt.label_ms || *key >= receipt.close_ms);
        for row in input.receipt.canonical.actual_m1 {
            self.committed.insert(row.open_ts.timestamp_millis(), row);
        }
        self.next += 1;
        self.counters.committed += 1;
        self.counters.recovery_committed += 1;
        Ok(())
    }
    pub(crate) fn finish_recovery(
        &mut self,
        completion: &RecoveryCompletion,
        now: i64,
    ) -> Result<Option<ResolvedRecovery>, PrimaryError> {
        self.clock(now)?;
        if !self.gap_unresolved
            || self.next != completion.end_index
            || self.generation != completion.generation
            || self.config_sha256 != completion.config_sha256
            || !self
                .gap_owner
                .as_ref()
                .is_some_and(|gap| Arc::ptr_eq(gap, &completion.gap))
            || !self.recovery_pending.as_ref().is_some_and(|p| {
                p.admitted && p.end_index == self.next && Arc::ptr_eq(&p.owner, &completion.owner)
            })
        {
            return Err(PrimaryError::Continuity);
        }
        let Some(data) = &self.last_live_data else {
            return Ok(None);
        };
        if self
            .next_label_ms()
            .is_some_and(|next| now - (next + 600000) + self.clock_uncertainty_ms > 10000)
        {
            return Ok(None);
        }
        if !self.fresh_seen
            || !self.physical_bound()
            || data.generation != self.generation
            || data.received_at_ms > now
            || now - data.received_at_ms >= 1200000
            || now >= self.run_deadline_ms
        {
            return Ok(None);
        }
        let physical_owner = data.owner.clone();
        #[cfg(test)]
        let physical_owner = physical_owner.or_else(|| self.fixture_unbound.then(|| Arc::new(())));
        let physical_owner = physical_owner.ok_or(PrimaryError::Generation)?;
        self.gap_unresolved = false;
        self.gap_owner = None;
        self.recovery_pending = None;
        self.state = PipelineState::LiveReady;
        let resolution = Arc::new(());
        self.last_resolution = Some(resolution.clone());
        Ok(Some(ResolvedRecovery {
            resolution,
            config_sha256: self.config_sha256.clone(),
            generation: self.generation,
            physical_owner,
            cutover_ms: completion.cutover_ms,
            source_sha256: completion.source_sha256.clone(),
        }))
    }

    pub(crate) fn accepts_resolution(&self, receipt: &ResolvedRecovery) -> bool {
        !self.gap_unresolved
            && self.state == PipelineState::LiveReady
            && self.generation == receipt.generation
            && self.config_sha256 == receipt.config_sha256
            && self
                .last_resolution
                .as_ref()
                .is_some_and(|owner| Arc::ptr_eq(owner, &receipt.resolution))
    }
}
fn at(ms: i64) -> Result<DateTime<Utc>, PrimaryError> {
    DateTime::from_timestamp_millis(ms).ok_or(PrimaryError::Timestamp)
}

#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod tests;
