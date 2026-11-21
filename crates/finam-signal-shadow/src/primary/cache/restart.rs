use super::*;
impl PrimaryProcessor {
    /// Called only after deterministic reconstruction of the admitted prefix.
    /// Restore committed math/frontier, NEVER an old socket, cache or DATA age.
    pub(crate) fn restore_prefix(
        &mut self,
        prefix: &crate::primary::restart::CommittedPrefix,
    ) -> Result<(), PrimaryError> {
        if self.config_sha256 != prefix.binding.config_sha256
            || self.required != prefix.required
            || self.next != 0
            || !self.cache.is_empty()
            || !self.committed.is_empty()
            || self.in_flight.is_some()
            || self.physical_owner.is_some()
            || self.gap_unresolved
            || self.fresh_seen
            || prefix.inputs.len() >= self.required.len()
        {
            return Err(PrimaryError::Continuity);
        }
        for input in &prefix.inputs {
            for row in &input.actual_m1 {
                self.committed
                    .insert(row.open_ts.timestamp_millis(), row.clone());
            }
        }
        self.next = prefix.inputs.len();
        self.counters.committed = self.next as u64;
        self.counters.restart_committed = self.next as u64;
        // Transport/bootstrap start from zero for the NEW physical connection.
        self.state = PipelineState::Bootstrap;
        Ok(())
    }
}
