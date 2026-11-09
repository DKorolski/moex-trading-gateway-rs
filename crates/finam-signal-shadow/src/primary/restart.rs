//! Pinned committed-prefix reconstruction, never persisted model-state import.
//! The binding/inventory must be supplied independently of the journal. Their
//! hashes prove local identity, not provider authentication or live readiness.
use crate::{feedback::CandleFill, observer::Step, seed::digest, segment::SeedObservation};
use broker_core::{event::Bar, HybridRuntimeBarEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub(crate) mod journal;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartBinding {
    pub schema_version: u32,
    pub config_sha256: String,
    pub previous_config_sha256: String,
    pub previous_journal_sha256: String,
    pub previous_segment_id: String,
    pub required_labels_sha256: String,
    pub committed_labels: usize,
    pub max_reconstruction_ms: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartError {
    Binding,
    Bounds,
    Clock,
    Journal,
    Incomplete,
    Source,
    Model,
    Evidence,
    Stopped,
}

/// Only complete pinned journal admission constructs this capability. It is
/// consumed once; neither a saved model observation nor a bare Bar can mint it.
/// ```compile_fail
/// use finam_signal_shadow::primary::restart::CommittedPrefix;
/// let forged: CommittedPrefix = serde_json::from_str("{}").unwrap();
/// ```
/// ```compile_fail
/// use finam_signal_shadow::primary::restart::CommittedPrefix;
/// fn duplicate(p: &CommittedPrefix) -> CommittedPrefix { (*p).clone() }
/// ```
pub struct CommittedPrefix {
    pub(crate) binding: RestartBinding,
    pub(crate) header: SeedObservation,
    pub(crate) required: Vec<i64>,
    pub(crate) inputs: Vec<CommittedInput>,
    pub(crate) admitted_at_ms: i64,
}
pub(crate) struct CommittedInput {
    pub origin: String,
    pub reason: Option<String>,
    pub bar: HybridRuntimeBarEvent,
    pub actual_m1: Vec<Bar>,
    pub minute_bitmap: u16,
    pub source_sha256: String,
    pub step: Step,
    pub feedback: Option<(CandleFill, Value)>,
}
impl CommittedPrefix {
    pub fn binding(&self) -> &RestartBinding {
        &self.binding
    }
    pub fn next_label_ms(&self) -> i64 {
        self.required[self.inputs.len()]
    }
    pub fn committed_labels(&self) -> usize {
        self.inputs.len()
    }
}

pub fn inventory_sha256(required: &[i64]) -> String {
    digest(&serde_json::to_vec(required).expect("integer inventory"))
}

pub fn admit(
    config: &super::WsPrimaryConfig,
    binding: RestartBinding,
    bytes: &[u8],
    required: &[i64],
    now_ms: i64,
) -> Result<CommittedPrefix, RestartError> {
    journal::admit(config, binding, bytes, required, now_ms)
}

/// Newly published historical source, not WS_LIVE or a fabricated REST receipt.
/// Raw M1 math remains independently reproducible in each linked segment; the
/// predecessor journal must also be retained for full original-source lineage.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestartSource {
    pub config_sha256: String,
    pub policy_sha256: String,
    pub origin: String,
    pub previous_journal_sha256: String,
    pub previous_source_sha256: String,
    pub label_ms: i64,
    pub close_ms: i64,
    pub canonical_digest: String,
    pub minute_bitmap: u16,
    pub actual_m1: Vec<Bar>,
    pub virtual_callback_at_ms: i64,
    pub virtual_result_at_ms: i64,
    pub virtual_received_at_ms: i64,
    pub virtual_ready_at_ms: i64,
    pub reconstructed_at_ms: i64,
}

pub(crate) fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
