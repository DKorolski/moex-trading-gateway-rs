//! Post-join semantic completion, not provider/cgroup/operational acceptance.
//! The writer prepares and seals its journal; only its parent can attest joins.
//! File verification never trusts semantic_eod_passed alone.
use serde::{Deserialize, Serialize};

pub const PREPARED_REASON: &str = "EOD_PRIMARY_PREPARED_NOT_DRAIN_ATTESTATION";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalSeal {
    pub sha256: String,
    pub bytes: u64,
    pub last_sequence: u64,
}

/// Read-only diagnostic DTO, NOT a runtime or restart capability. Its byte pin
/// must be supplied separately to the file verifier with the journal/calendar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub schema_version: u32,
    pub config_sha256: String,
    pub calendar_sha256: String,
    pub journal: Option<JournalSeal>,
    pub calendar_eod_trigger: bool,
    pub eod_check_at_ms: i64,
    pub stop_started_at_ms: i64,
    pub joined_at_ms: i64,
    pub stop_budget_ms: u64,
    pub elapsed_stop_ms: u64,
    pub clock_consistent: bool,
    pub socket_drained: bool,
    pub http_drained: bool,
    pub writer_drained: bool,
    pub fault_free: bool,
    pub continuity_resolved: bool,
    pub recovery_resolved: bool,
    pub model_prepared: bool,
    pub semantic_eod_passed: bool,
    pub cgroup_qualified: bool,
    pub network_qualified: bool,
}
impl Completion {
    pub(crate) fn qualifies(&self) -> bool {
        self.schema_version == 1
            && self.calendar_eod_trigger
            && self.stop_started_at_ms >= self.eod_check_at_ms
            && self.joined_at_ms >= self.stop_started_at_ms
            && self.clock_consistent
            && self.elapsed_stop_ms <= self.stop_budget_ms
            && (1..=15000).contains(&self.stop_budget_ms)
            && self.socket_drained
            && self.http_drained
            && self.writer_drained
            && self.fault_free
            && self.continuity_resolved
            && self.recovery_resolved
            && self.model_prepared
            && self.journal.is_some()
    }
}

pub(crate) struct FinishFacts {
    pub event: serde_json::Value,
    pub requested_eod: bool,
    pub io_drained: bool,
    pub continuity_resolved: bool,
    pub recovery_resolved: bool,
}
