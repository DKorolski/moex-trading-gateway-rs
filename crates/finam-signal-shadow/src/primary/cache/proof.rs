//! A timely mandatory proof creates the same canonical capability as WS, but
//! never gains WS provenance. Audit is comparison-only and cannot mint one.
use super::*;

fn proof_prices_compatible(old: &Bar, new: &Bar, observed: bool) -> bool {
    new.open == old.open
        && new.high >= old.high
        && new.low <= old.low
        && (new.volume >= old.volume
            || (observed && super::super::revisions::same_prices(old, new)))
}

impl PrimaryProcessor {
    /// Comparison failure cannot poison eligible WS. V4 additionally permits
    /// a whole admitted REST source for an unpublished exception slot; V1–V3
    /// retain their original price-admission rules.
    pub(crate) fn rest_source_prices_compatible(
        &self,
        source: &AdmittedClosedM1Snapshot,
        reason: ExceptionReason,
    ) -> bool {
        let label = source.evidence().start.timestamp_millis();
        let priority = self.policy.admitted_rest_source()
            || (reason == ExceptionReason::StartupPartial
                && self.subscribed_at_ms > label
                && self.session_end_label() != Some(label)
                && self.policy.rest_precommit_ws_first());
        self.cache.range(label..label + 600000).all(|(_, old)| {
            source.bars().any(|new| {
                new.open_ts == old.bar.open_ts
                    && new.instrument == old.bar.instrument
                    && (priority
                        || proof_prices_compatible(&old.bar, new, self.policy.observed_bo()))
            })
        })
    }
    /// None grants no permission: WS already sealed/committed or is ready to
    /// seal. The owner must compare this answer as audit, never call the model.
    pub fn prepare_rest_exception(
        &mut self,
        source: &AdmittedClosedM1Snapshot,
        reason: ExceptionReason,
        now_ms: i64,
    ) -> Result<Option<Box<AdmittedCanonicalM10>>, PrimaryError> {
        self.clock(now_ms)?;
        let label = source.evidence().start.timestamp_millis();
        let close = source.evidence().end.timestamp_millis();
        if close.checked_sub(label) != Some(600000) || source.evidence().parts.len() != 1 {
            return Err(PrimaryError::ReferenceUnproven);
        }
        if self.next_label_ms().is_none()
            || self.next_label_ms().is_some_and(|next| label < next)
            || self.in_flight.is_some()
        {
            return Ok(None);
        }
        if self.next_label_ms() != Some(label)
            || self.gap_unresolved
            || !self.fresh_seen
            || (reason == ExceptionReason::SessionEnd) != (self.session_end_label() == Some(label))
            || self
                .calendar
                .as_ref()
                .is_some_and(|c| now_ms >= c.eod_check_at_ms())
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let part = &source.evidence().parts[0];
        let received = part.received_at.timestamp_millis();
        let requested = part.requested_at.timestamp_millis();
        if requested < close + self.policy.proof_wait_ms()
            || received < requested
            || received > now_ms
            || received - requested > self.policy.proof_timeout_ms as i64
            || now_ms - close + self.clock_uncertainty_ms > 10000
            || (self.policy.observed_bo() && now_ms - close + self.clock_uncertainty_ms > 2000)
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let entries: Vec<_> = self
            .cache
            .range(label..close)
            .map(|(_, v)| v.clone())
            .collect();
        let boundary = self
            .cache
            .range(close..)
            .filter(|(_, v)| v.generation == self.generation)
            .map(|(_, v)| v.first_received_ms)
            .min();
        let required_reason = if self.session_end_label() == Some(label) {
            Some(ExceptionReason::SessionEnd)
        } else if self.subscribed_at_ms > label {
            Some(ExceptionReason::StartupPartial)
        } else if entries.len() != 10 || entries.iter().any(|v| v.generation != self.generation) {
            Some(ExceptionReason::SparseUnproven)
        } else if boundary.is_none() {
            Some(ExceptionReason::BoundaryUnproven)
        } else {
            None
        };
        // Coverage can improve while the single GET is in flight. Its original
        // reason is not an immutable property of the later WS cache.
        if required_reason.is_some()
            && required_reason != Some(reason)
            && !(reason == ExceptionReason::SparseUnproven
                && required_reason == Some(ExceptionReason::BoundaryUnproven))
        {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let bucket = source
            .bucket(at(label)?)
            .map_err(|_| PrimaryError::ReferenceUnproven)?;
        let rows = bucket.actual_m1();
        // V4 explicitly permits whole admitted REST in any still-unpublished
        // exception slot. Older policies retain startup-only price priority.
        // No missing known minute, identity failure, late source, replay or
        // post-commit rewrite gains permission through this choice.
        let rest_authority = self.policy.admitted_rest_source()
            || (self.policy.rest_precommit_ws_first()
                && required_reason == Some(ExceptionReason::StartupPartial));
        for version in &entries {
            let Some(final_row) = rows.iter().find(|b| b.open_ts == version.bar.open_ts) else {
                self.note_change(
                    "REST_PROOF",
                    Some(&version.bar),
                    None,
                    Some(source.sha256()),
                )?;
                return self.hold(PrimaryError::CanonicalConflict);
            };
            self.note_change(
                "REST_PROOF",
                Some(&version.bar),
                Some(final_row),
                Some(source.sha256()),
            )?;
            if final_row.instrument != self.instrument
                || (!rest_authority
                    && !proof_prices_compatible(&version.bar, final_row, self.policy.observed_bo()))
            {
                return self.hold(PrimaryError::CanonicalConflict);
            }
        }
        if required_reason.is_none() {
            // Never bypass grace. The coordinator retains this validated answer
            // and compares exact final rows before the eventual WS callback.
            return Ok(None);
        }
        let model_bar = runtime_bar(bucket.bar())?;
        let receipt = CanonicalReceipt {
            config_sha256: self.config_sha256.clone(),
            policy_sha256: self.policy_sha256.clone(),
            origin: "REST_EXCEPTION",
            reason: required_reason,
            label_ms: label,
            close_ms: close,
            generation: self.generation,
            subscribed_at_ms: self.subscribed_at_ms,
            last_component_received_at_ms: received,
            boundary_received_at_ms: None,
            proof_received_at_ms: Some(received),
            rest_snapshot_sha256: Some(source.sha256().to_owned()),
            canonical_ready_at_ms: now_ms,
            minute_bitmap: bucket.minute_bitmap(),
            canonical_digest: crate::input::canonical_digest(&model_bar)
                .map_err(|_| PrimaryError::Numeric)?,
            actual_m1: rows.to_vec(),
            source_frame_sha256: vec![],
        };
        let owner = Arc::new(());
        self.in_flight = Some(owner.clone());
        Ok(Some(Box::new(AdmittedCanonicalM10 {
            owner,
            receipt,
            model_bar,
            observed_bo: self.policy.observed_bo(),
            rest_evidence: Some(source.evidence().clone()),
        })))
    }

    /// Compare exact final M1 inventory AND values. Even equal aggregate M10
    /// cannot hide a differing minute bitmap/volume distribution.
    pub fn compare_rest_audit(
        &mut self,
        source: &AdmittedClosedM1Snapshot,
    ) -> Result<(), PrimaryError> {
        if self.state == PipelineState::Hold {
            return Err(PrimaryError::Held);
        }
        let label = source.evidence().start.timestamp_millis();
        let close = source.evidence().end.timestamp_millis();
        if close - label != 600000 || !self.required[..self.next].contains(&label) {
            return Err(PrimaryError::ReferenceUnproven);
        }
        let expected: Vec<_> = self
            .committed
            .range(label..close)
            .map(|(_, bar)| bar.clone())
            .collect();
        let actual: Vec<_> = source.bars().collect();
        if expected.len() != actual.len() {
            let missing = expected
                .iter()
                .find(|a| !actual.iter().any(|b| a.open_ts == b.open_ts));
            let extra = actual
                .iter()
                .copied()
                .find(|b| !expected.iter().any(|a| a.open_ts == b.open_ts));
            self.note_change(
                "REST_AUDIT_INVENTORY",
                missing,
                extra,
                Some(source.sha256()),
            )?;
            return self.hold(PrimaryError::CanonicalConflict);
        }
        for (a, b) in expected.iter().zip(actual) {
            self.note_change("REST_AUDIT", Some(a), Some(b), Some(source.sha256()))?;
            if !super::super::revisions::compatible(a, b, self.policy.observed_bo()) {
                return self.hold(PrimaryError::CanonicalConflict);
            }
        }
        Ok(())
    }
}

fn at(ms: i64) -> Result<DateTime<Utc>, PrimaryError> {
    DateTime::from_timestamp_millis(ms).ok_or(PrimaryError::Timestamp)
}
