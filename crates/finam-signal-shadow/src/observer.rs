//! Bounded new-segment JSONL evidence. No resume, truncate or append-to-old-file.
use crate::{
    feedback::{Action, Fill},
    segment::{ObservationMode, SeedObservation},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub action: Option<Action>,
    pub quantity_lots: u32,
    /// Actual branch reason when available; generic no-intent is not a diagnosis.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub mode: ObservationMode,
    pub instrument: String,
    pub model_label_utc_ms: i64,
    pub nominal_close_utc_ms: i64,
    pub provider_at_ms: Option<i64>,
    pub received_at_ms: i64,
    pub canonical_ready_at_ms: i64,
    pub model_start_at_ms: i64,
    pub result_at_ms: i64,
    pub canonical_input_sha256: String,
    /// Digest of supplied source evidence; does not imply M1 or verified coverage.
    pub raw_provenance_sha256: String,
    pub before: Value,
    pub after: Value,
    pub decision: Decision,
}

impl Decision {
    /// Call with the actual returned intent and the immediate post-step model
    /// observation, BEFORE feedback clears pending state. Never diagnoses a
    /// suppression from a no-intent outcome or reuses a stale pending reason.
    pub fn from_model_observation(
        action: Option<Action>,
        quantity_lots: u32,
        after: &Value,
    ) -> Result<Self, Error> {
        if action.is_some() != (quantity_lots > 0) {
            return Err(Error::Invalid);
        }
        let reason = match action {
            None => match after.get("no_intent_trace").filter(|v| !v.is_null()) {
                None => "model_returned_no_intent",
                Some(trace) => {
                    if trace["label_utc_ms"]
                        .as_i64()
                        .filter(|v| *v >= 0 && *v % 600_000 == 0)
                        .is_none()
                        || !after["pending_entry_reason"].is_null()
                        || !after["pending_exit_reason"].is_null()
                    {
                        return Err(Error::Invalid);
                    }
                    let reason = trace["reason"].as_str().ok_or(Error::Invalid)?;
                    match reason {
                        "before_wait"
                        | "minimum_range"
                        | "breakout_veto"
                        | "missing_previous_close"
                        | "missing_previous_range"
                        | "missing_session_start"
                        | "weekend"
                        | "position_hold"
                        | "live_orders_hold"
                        | "pending_entry_hold"
                        | "eod_entry_cutoff"
                        | "mr_position_hold" => reason,
                        _ => return Err(Error::Invalid),
                    }
                }
            },
            Some(action) => {
                let key = match action {
                    Action::EnterLong | Action::EnterShort => "pending_entry_reason",
                    Action::Exit => "pending_exit_reason",
                };
                after[key]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or(Error::Invalid)?
            }
        };
        Ok(Self {
            action,
            quantity_lots,
            reason: reason.into(),
        })
    }
}

#[derive(Debug)]
pub enum Error {
    Invalid,
    Limit,
    Closed,
    Io(io::Error),
    Json(serde_json::Error),
}

pub struct Observer {
    file: File,
    header: SeedObservation,
    bytes: u64,
    max_bytes: u64,
    max_row_bytes: usize,
    sequence: u64,
    last_label: Option<i64>,
    terminal: bool,
}

impl Observer {
    pub fn record_transport(&mut self, event: &impl Serialize) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        self.append("transport_event", event)
    }
    /// The caller provides an isolated, trusted output directory. create_new
    /// refuses an existing file (including an existing final-component symlink).
    pub fn create(
        path: &Path,
        header: SeedObservation,
        max_bytes: u64,
        max_row_bytes: usize,
    ) -> Result<Self, Error> {
        if header.mode != ObservationMode::Warmup
            || header.live_observation_count != 0
            || max_row_bytes == 0
            || max_row_bytes > 1_048_576
            || max_bytes == 0
        {
            return Err(Error::Invalid);
        }
        let line = encode(&serde_json::json!({"sequence":0,"kind":"seed","observation":&header}))?;
        if line.len() > max_row_bytes || line.len() as u64 > max_bytes {
            return Err(Error::Limit);
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(Error::Io)?;
        file.write_all(&line)
            .and_then(|_| file.sync_all())
            .map_err(Error::Io)?;
        Ok(Self {
            file,
            header,
            bytes: line.len() as u64,
            max_bytes,
            max_row_bytes,
            sequence: 0,
            last_label: None,
            terminal: false,
        })
    }

    /// Record the caller's exact run plan before transport starts. The caller
    /// must supply only non-secret plan fields, never credentials or auth headers.
    pub fn record_run_plan(&mut self, plan: &impl Serialize) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        self.append("run_plan", plan)
    }

    /// Retain admitted source evidence before advancing the model. The digest
    /// is the admission digest used by Step::raw_provenance_sha256, not a hash
    /// of this JSONL envelope. Raw response bodies remain byte-exact strings.
    /// No implicit deduplication: callers decide which admitted snapshots were
    /// selected. Row/output limits and terminal-on-write-failure apply as usual.
    pub fn record_source(
        &mut self,
        snapshot: &broker_finam::sparse_m10::AdmittedClosedM1Snapshot,
    ) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        self.append(
            "source_snapshot",
            &serde_json::json!({
                "snapshot_sha256": snapshot.sha256(),
                "evidence": snapshot.evidence(),
                "accounting": snapshot.accounting(),
            }),
        )
    }

    pub fn record_step(&mut self, step: &Step) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if !valid_step(step)
            || self
                .last_label
                .is_some_and(|label| step.model_label_utc_ms <= label)
            || (step.mode == ObservationMode::LiveObserved
                && step.nominal_close_utc_ms <= self.header.observation_start_utc_ms)
        {
            self.terminal = true;
            return Err(Error::Invalid);
        }
        self.append("model_step", step)?;
        self.last_label = Some(step.model_label_utc_ms);
        Ok(())
    }

    /// Feedback is an independent event, never rewritten into the decision row.
    pub fn record_feedback(&mut self, mode: ObservationMode, fill: &Fill) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if fill.quote.event_at_ms <= fill.request.decision_at_ms
            || fill.quote.received_at_ms < fill.quote.event_at_ms
            || fill.applied_at_ms < fill.quote.received_at_ms
        {
            self.terminal = true;
            return Err(Error::Invalid);
        }
        self.append(
            "hypothetical_feedback",
            &serde_json::json!({"mode":mode,"fill":fill}),
        )
    }

    pub fn finish(&mut self, reason: &str) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if reason.is_empty() || reason.len() > 256 {
            self.terminal = true;
            return Err(Error::Invalid);
        }
        self.append("segment_end", &serde_json::json!({"reason":reason}))?;
        self.terminal = true;
        Ok(())
    }

    fn append<T: Serialize>(&mut self, kind: &str, event: &T) -> Result<(), Error> {
        let result = (|| {
            let sequence = self.sequence.checked_add(1).ok_or(Error::Limit)?;
            let line = encode(&serde_json::json!({
                "sequence":sequence, "kind":kind,
                "run_id":self.header.run_id, "segment_id":self.header.segment_id,
                "config_sha256":self.header.config_sha256, "seed_sha256":self.header.seed_sha256,
                "semantic_config_sha256":self.header.semantic_config_sha256,
                "model_sha256":self.header.model_sha256,
                "reference_sha256":self.header.reference_sha256,
                "feedback_policy_sha256":self.header.feedback_policy_sha256, "event":event
            }))?;
            let bytes = self
                .bytes
                .checked_add(line.len() as u64)
                .ok_or(Error::Limit)?;
            if line.len() > self.max_row_bytes || bytes > self.max_bytes {
                return Err(Error::Limit);
            }
            self.file
                .write_all(&line)
                .and_then(|_| self.file.sync_all())
                .map_err(Error::Io)?;
            self.sequence = sequence;
            self.bytes = bytes;
            Ok(())
        })();
        if result.is_err() {
            self.terminal = true;
        }
        result
    }
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let mut bytes = serde_json::to_vec(value).map_err(Error::Json)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_step(step: &Step) -> bool {
    let trace_bound = step
        .after
        .get("no_intent_trace")
        .filter(|v| !v.is_null())
        .map_or(true, |trace| {
            trace["label_utc_ms"].as_i64() == Some(step.model_label_utc_ms)
                && (step.decision.action.is_some()
                    || Decision::from_model_observation(None, 0, &step.after)
                        .is_ok_and(|decision| decision == step.decision))
        });
    trace_bound
        && step.model_label_utc_ms >= 0
        && step.model_label_utc_ms % 600_000 == 0
        && step.model_label_utc_ms.checked_add(600_000) == Some(step.nominal_close_utc_ms)
        && step.received_at_ms >= 0
        && step.canonical_ready_at_ms >= step.nominal_close_utc_ms
        && step.canonical_ready_at_ms >= step.received_at_ms
        && step.model_start_at_ms >= step.canonical_ready_at_ms
        && step.result_at_ms >= step.model_start_at_ms
        && !step.instrument.is_empty()
        && step.instrument.len() <= 128
        && hash(&step.canonical_input_sha256)
        && hash(&step.raw_provenance_sha256)
        && step.before.is_object()
        && step.after.is_object()
        && !step.decision.reason.is_empty()
        && step.decision.reason.len() <= 256
        && (step.decision.action.is_some() == (step.decision.quantity_lots > 0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    NotComparable,
    DifferentInput,
    SameInputMatch,
    SameInputModelDrift,
}

/// Conservative report binding check, not proof of ALOR provenance or full
/// session coverage. Different run/segment IDs are expected for isolated contexts.
/// Semantic config equality excludes only run/segment attribution. The full
/// config hash remains required provenance, but need not match across reports.
pub fn report_identities_compatible(left: &SeedObservation, right: &SeedObservation) -> bool {
    let identities = |h: &SeedObservation| {
        [
            h.semantic_config_sha256.clone(),
            h.seed_sha256.clone(),
            h.model_sha256.clone(),
            h.reference_sha256.clone(),
            h.feedback_policy_sha256.clone(),
        ]
    };
    let a = identities(left);
    let b = identities(right);
    a.iter().chain(b.iter()).all(|s| hash(s))
        && hash(&left.config_sha256)
        && hash(&right.config_sha256)
        && a == b
        && left.mode == ObservationMode::Warmup
        && right.mode == ObservationMode::Warmup
        && left.live_observation_count == 0
        && right.live_observation_count == 0
        && left.last_history_label_utc == right.last_history_label_utc
        && left.observation_start_utc_ms == right.observation_start_utc_ms
        && left.virtual_position_lots == right.virtual_position_lots
        && left.state.is_object()
        && right.state.is_object()
        && semantic(&left.state) == semantic(&right.state)
}

pub fn compare_report_steps(
    left_header: &SeedObservation,
    left: &Step,
    right_header: &SeedObservation,
    right: &Step,
) -> Comparison {
    if !report_identities_compatible(left_header, right_header) {
        return Comparison::NotComparable;
    }
    compare_steps(left, right)
}

fn semantic(state: &Value) -> Value {
    let mut result = state.clone();
    if let Some(object) = result.as_object_mut() {
        // Host-generated IDs are attribution, not model semantics.
        for key in ["pending_entry_request_id", "pending_exit_request_id"] {
            if let Some(id) = object.get_mut(key) {
                *id = Value::Bool(!id.is_null());
            }
        }
    }
    result
}

/// Pairwise semantic comparison only. Caller must separately prove reference,
/// profile, seed and feedback equivalence; this is not an ALOR parity verdict.
pub fn compare_steps(left: &Step, right: &Step) -> Comparison {
    if !valid_step(left)
        || !valid_step(right)
        || left.instrument != right.instrument
        || left.model_label_utc_ms != right.model_label_utc_ms
        || left.mode != right.mode
    {
        return Comparison::NotComparable;
    }
    if left.canonical_input_sha256 != right.canonical_input_sha256 {
        return Comparison::DifferentInput;
    }
    if semantic(&left.before) == semantic(&right.before)
        && semantic(&left.after) == semantic(&right.after)
        && left.decision == right.decision
    {
        Comparison::SameInputMatch
    } else {
        Comparison::SameInputModelDrift
    }
}
