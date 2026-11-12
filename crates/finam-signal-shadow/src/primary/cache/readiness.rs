//! Diagnostic only: evaluate the actual pre-deadline predicates without
//! minting an input or replacing the later authoritative prepare() check.
use super::*;

impl PrimaryProcessor {
    pub(crate) fn readiness_diagnostic(&self, now: i64) -> Option<serde_json::Value> {
        let label = self.next_label_ms()?;
        let close = label + 600000;
        let rows: Vec<_> = self.cache.range(label..close).map(|(_, v)| v).collect();
        let bitmap = |current: bool| {
            rows.iter()
                .filter(|v| !current || v.generation == self.generation)
                .fold(0u16, |bits, v| {
                    bits | (1 << ((v.bar.open_ts.timestamp_millis() - label) / 60000))
                })
        };
        let boundary = self
            .cache
            .range(close..)
            .filter(|(_, v)| v.generation == self.generation)
            .map(|(_, v)| v.first_received_ms)
            .min();
        let grace = boundary.map(|b| close.max(b) + self.policy.close_grace_ms as i64);
        let reason = if self.subscribed_at_ms > label {
            "StartupPartial"
        } else if bitmap(false) != 1023 || bitmap(true) != 1023 {
            "SparseUnproven"
        } else if self.quarantined.range(label..close).next().is_some() {
            "ProvisionalRevision"
        } else if boundary.is_none() {
            "BoundaryUnproven"
        } else if grace.is_some_and(|t| now < t) {
            "WsGracePending"
        } else {
            "ReadyPredicates"
        };
        let deadline = close + 2000 - self.clock_uncertainty_ms;
        let outcome = if self.gap_unresolved {
            "RECOVERY"
        } else if now > deadline {
            "RECOVERY_REQUIRED"
        } else if now < close {
            "WAITING_CLOSE"
        } else if reason == "ReadyPredicates" {
            "WS_READY_CANDIDATE"
        } else {
            "WAITING"
        };
        Some(
            serde_json::json!({"label_ms":label,"close_ms":close,"generation":self.generation,
            "subscribed_at_ms":self.subscribed_at_ms,"cached_bitmap":bitmap(false),
            "current_generation_bitmap":bitmap(true),"quarantined_slots":self.quarantined.range(label..close).copied().collect::<Vec<_>>(),
            "predeadline_reason":reason,"boundary_received_at_ms":boundary,"grace_due_at_ms":grace,
            "writer_processed_at_ms":now,"live_deadline_ms":deadline,"remaining_ms":deadline-now,
            "last_component_changed_at_ms":rows.iter().map(|v|v.last_changed_ms).max(),
            "latest_current_generation_data_received_at_ms":self.last_live_data.as_ref().map(|v|v.received_at_ms),
            "outcome":outcome,"diagnostic_only":true,"admission_relaxed":false}),
        )
    }
}
