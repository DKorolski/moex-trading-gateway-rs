//! V8: a reason-bound, one-shot historical capability, never REST or LIVE.
//! Only the single writer calls this after a healthy data-only episode begins.
use super::*;
#[cfg(test)]
mod tests;

pub(in crate::primary::cache) struct DataGap {
    gap: Arc<()>,
    physical: Arc<()>,
    generation: u64,
    label: i64,
    at: i64,
}

#[derive(Debug, Serialize)]
pub(crate) struct WsHistoricalVersion {
    generation: u64,
    first_received_at_ms: i64,
    last_changed_at_ms: i64,
    frame_sha256: String,
    first_frame_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct WsRecoveryReceipt {
    #[serde(flatten)]
    pub canonical: CanonicalReceipt,
    pub source_transport: &'static str,
    pub processing_mode: &'static str,
    pub timeliness: &'static str,
    pub broker_execution: bool,
    pub episode: u8,
    pub episode_deadline_ms: i64,
    pub continuity_revision: u64,
    pub data_gap_at_ms: i64,
    pub boundary_frame_sha256: String,
    pub(crate) versions: Vec<WsHistoricalVersion>,
    pub virtual_callback_at_ms: i64,
    pub replay_deadline_ms: i64,
}

/// No constructor, Deserialize or Clone. Consumed only after durable feedback.
///
/// ```compile_fail
/// use finam_signal_shadow::primary::WsRecoveryInput;
/// let forged: WsRecoveryInput = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use finam_signal_shadow::primary::WsRecoveryInput;
/// fn duplicate(input: &WsRecoveryInput) -> WsRecoveryInput { (*input).clone() }
/// ```
#[derive(Debug)]
pub struct WsRecoveryInput {
    gap: Arc<()>,
    batch: Arc<()>,
    receipt: WsRecoveryReceipt,
    bar: HybridRuntimeBarEvent,
}
impl WsRecoveryInput {
    pub fn receipt(&self) -> &WsRecoveryReceipt {
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
pub(crate) struct AdmittedWsRecovery {
    pub input: WsRecoveryInput,
    pub completion: RecoveryCompletion,
}

impl PrimaryProcessor {
    pub(in crate::primary::cache) fn ws_gap_eligible(&self) -> bool {
        let Some(label) = self.next_label_ms() else {
            return false;
        };
        self.policy.ws_historical_recovery()
            && self.physical_owner.is_some()
            && self.physical_bound()
            && self.fresh_seen
            && self.in_flight.is_none()
            && self.subscribed_at_ms <= label
            && self.session_end_label() != Some(label)
            && self
                .quarantined
                .range(label..label + 600000)
                .next()
                .is_none()
            && self.cache.range(label..label + 600000).count() == 10
            && self
                .cache
                .range(label..label + 600000)
                .all(|(_, v)| v.generation == self.generation)
    }
    pub(in crate::primary::cache) fn capture_ws_data_gap(&mut self, now: i64) {
        if self.ws_gap_eligible() {
            self.late_ws_gap = Some(DataGap {
                gap: self.gap_owner.as_ref().unwrap().clone(),
                physical: self.physical_owner.as_ref().unwrap().clone(),
                generation: self.generation,
                label: self.next_label_ms().unwrap(),
                at: now,
            });
        }
    }
    /// This is deliberately separate from break_continuity(): real faults,
    /// ingress loss and new generations ALWAYS invalidate this right.
    pub(crate) fn relatch_data_gap(&mut self) {
        let old = self.late_ws_gap.take();
        self.break_continuity();
        if let Some(mut permit) = old {
            if self.ws_gap_eligible()
                && permit.generation == self.generation
                && Some(permit.label) == self.next_label_ms()
                && self
                    .physical_owner
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(p, &permit.physical))
            {
                permit.gap = self.gap_owner.as_ref().unwrap().clone();
                self.late_ws_gap = Some(permit);
            }
        }
    }
    pub(crate) fn ws_data_gap_evidence(&self, revision: u64) -> Option<serde_json::Value> {
        let permit = self.late_ws_gap.as_ref()?;
        Some(serde_json::json!({
            "event":"primary_ws_data_gap_authorized", "label_ms":permit.label,
            "generation":permit.generation,"subscribed_at_ms":self.subscribed_at_ms,
            "data_gap_at_ms":permit.at,"continuity_revision":revision,
            "physical_owner_bound":true,"reason":"LivePublicationDeadline",
        }))
    }
    pub(crate) fn prepare_ws_recovery(
        &mut self,
        episode: u8,
        episode_deadline_ms: i64,
        continuity_revision: u64,
        now: i64,
    ) -> Result<Option<AdmittedWsRecovery>, PrimaryError> {
        self.clock(now)?;
        let Some(label) = self.next_label_ms() else {
            return Ok(None);
        };
        self.last_readiness_evaluation = Some((label, now));
        if !self.gap_unresolved
            || !self.ws_gap_eligible()
            || now >= episode_deadline_ms
            || now >= self.run_deadline_ms
            || episode == 0
            || episode > 3
            || self
                .calendar
                .as_ref()
                .is_some_and(|c| now >= c.eod_check_at_ms())
            || self.recovery_pending.as_ref().is_some_and(|p| p.admitted)
        {
            return Ok(None);
        }
        let Some(permit) = &self.late_ws_gap else {
            return Ok(None);
        };
        if permit.generation != self.generation
            || permit.label != label
            || !self
                .gap_owner
                .as_ref()
                .is_some_and(|g| Arc::ptr_eq(g, &permit.gap))
            || !self
                .physical_owner
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &permit.physical))
        {
            return Ok(None);
        }
        let close = label + 600000;
        let Some(boundary) = self
            .cache
            .range(close..)
            .filter(|(_, v)| v.generation == self.generation)
            .min_by_key(|(_, v)| v.first_received_ms)
            .map(|(_, v)| v)
        else {
            return Ok(None);
        };
        if now <= close + 2000 - self.clock_uncertainty_ms
            || now < close.max(boundary.first_received_ms) + self.policy.close_grace_ms as i64
        {
            return Ok(None);
        }
        let entries: Vec<_> = self.cache.range(label..close).map(|(_, v)| v).collect();
        let rows: Vec<_> = entries.iter().map(|v| v.bar.clone()).collect();
        let values = aggregate_m1_window(&self.instrument, at(label)?, &rows)
            .map_err(|_| PrimaryError::Numeric)?;
        if values.minute_bitmap != 1023 {
            return self.hold(PrimaryError::Continuity);
        }
        let mut bar = runtime_bar(&values.provisional_bar)?;
        bar.origin = HybridRuntimeBarOrigin::Replay;
        let receipt = WsRecoveryReceipt {
            canonical: CanonicalReceipt {
                config_sha256: self.config_sha256.clone(),
                policy_sha256: self.policy_sha256.clone(),
                origin: "WS_RECOVERY_BACKFILL",
                reason: None,
                label_ms: label,
                close_ms: close,
                generation: self.generation,
                subscribed_at_ms: self.subscribed_at_ms,
                last_component_received_at_ms: entries
                    .iter()
                    .map(|v| v.last_changed_ms)
                    .max()
                    .unwrap(),
                boundary_received_at_ms: Some(boundary.first_received_ms),
                proof_received_at_ms: None,
                rest_snapshot_sha256: None,
                canonical_ready_at_ms: now,
                minute_bitmap: values.minute_bitmap,
                canonical_digest: crate::input::canonical_digest(&bar)
                    .map_err(|_| PrimaryError::Numeric)?,
                actual_m1: rows,
                source_frame_sha256: entries.iter().map(|v| v.frame_sha256.clone()).collect(),
            },
            source_transport: "WS",
            processing_mode: "BACKFILL_REPLAY",
            timeliness: "LATE",
            broker_execution: false,
            episode,
            episode_deadline_ms,
            continuity_revision,
            data_gap_at_ms: permit.at,
            boundary_frame_sha256: boundary.first_frame_sha256.clone(),
            versions: entries
                .iter()
                .map(|v| WsHistoricalVersion {
                    generation: v.generation,
                    first_received_at_ms: v.first_received_ms,
                    last_changed_at_ms: v.last_changed_ms,
                    frame_sha256: v.frame_sha256.clone(),
                    first_frame_sha256: v.first_frame_sha256.clone(),
                })
                .collect(),
            virtual_callback_at_ms: close,
            replay_deadline_ms: episode_deadline_ms.min(self.run_deadline_ms),
        };
        let batch = Arc::new(());
        let gap = permit.gap.clone();
        let input = WsRecoveryInput {
            gap: gap.clone(),
            batch: batch.clone(),
            receipt,
            bar,
        };
        let completion = RecoveryCompletion {
            gap,
            owner: batch.clone(),
            config_sha256: self.config_sha256.clone(),
            generation: self.generation,
            end_index: self.next + 1,
            source_sha256: input.provenance_sha256()?,
            cutover_ms: close,
        };
        // This atomically retires any REST pending ticket. A late reply is
        // retained/diagnosed by the coordinator, never another source admission.
        self.recovery_pending = Some(Pending {
            owner: batch,
            end_index: self.next + 1,
            admitted: true,
            responded: true,
        });
        Ok(Some(AdmittedWsRecovery { input, completion }))
    }
    pub(crate) fn recovered_ws(
        &mut self,
        input: WsRecoveryInput,
        now: i64,
    ) -> Result<(), PrimaryError> {
        self.clock(now)?;
        let r = &input.receipt.canonical;
        if !self.gap_unresolved
            || Some(r.label_ms) != self.next_label_ms()
            || r.generation != self.generation
            || now < r.canonical_ready_at_ms
            || now >= input.receipt.replay_deadline_ms
            || !self
                .gap_owner
                .as_ref()
                .is_some_and(|g| Arc::ptr_eq(g, &input.gap))
            || !self.recovery_pending.as_ref().is_some_and(|p| {
                p.admitted && p.end_index == self.next + 1 && Arc::ptr_eq(&p.owner, &input.batch)
            })
        {
            return self.hold(PrimaryError::Continuity);
        }
        self.committed_sources
            .insert(r.label_ms, input.provenance_sha256()?);
        self.cache
            .retain(|k, _| *k < r.label_ms || *k >= r.close_ms);
        for row in input.receipt.canonical.actual_m1 {
            self.committed.insert(row.open_ts.timestamp_millis(), row);
        }
        self.next += 1;
        self.counters.committed += 1;
        self.counters.recovery_committed += 1;
        self.late_ws_gap = None;
        Ok(())
    }
}
