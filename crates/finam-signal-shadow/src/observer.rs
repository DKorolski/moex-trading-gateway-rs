//! Bounded new-segment JSONL evidence. No resume, truncate or append-to-old-file.
use crate::{
    feedback::{Action, Fill},
    segment::{ObservationMode, SeedObservation},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
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
    pending_canonical: Option<(i64, String)>,
    last_retained_source: Option<String>,
    journal_hash: Sha256,
    sealed: bool,
    #[cfg(test)]
    fail_kind: Option<&'static str>,
    #[cfg(test)]
    crash_pause: Option<(&'static str, bool, std::path::PathBuf)>,
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
            pending_canonical: None,
            last_retained_source: None,
            journal_hash: Sha256::new_with_prefix(&line),
            sealed: false,
            #[cfg(test)]
            fail_kind: None,
            #[cfg(test)]
            crash_pause: None,
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
        )?;
        self.last_retained_source = Some(snapshot.sha256().into());
        Ok(())
    }

    pub fn record_step(&mut self, step: &Step) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if !valid_step(step)
            || self
                .pending_canonical
                .as_ref()
                .is_some_and(|(label, hash)| {
                    *label != step.model_label_utc_ms || hash != &step.raw_provenance_sha256
                })
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

    pub fn record_candle_feedback(
        &mut self,
        mode: ObservationMode,
        fill: &crate::feedback::CandleFill,
        state_after: &Value,
    ) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if self.header.feedback_policy_sha256 != crate::feedback::candle_policy_fingerprint()
            || fill.policy != crate::feedback::CANDLE_POLICY
            || self.last_label != Some(fill.model_label_utc_ms)
            || fill.model_label_utc_ms.checked_add(600_000) != Some(fill.nominal_close_utc_ms)
            || fill.received_at_ms < fill.nominal_close_utc_ms
            || fill.request.decision_at_ms < fill.received_at_ms
            || fill.applied_at_ms < fill.request.decision_at_ms
            || fill.price_ticks <= 0
            || fill.request.quantity_lots != 1
            || !hash(&fill.canonical_input_sha256)
            || !hash(&fill.raw_provenance_sha256)
            || !state_after.is_object()
        {
            self.terminal = true;
            return Err(Error::Invalid);
        }
        self.append(
            "hypothetical_candle_feedback",
            &serde_json::json!({
                "mode":mode, "fill":fill, "state_after":state_after,
                "price_assumption":"accepted_closed_m10_close",
                "broker_execution":false, "quotes_observed":false
            }),
        )
    }

    pub(crate) fn record_canonical_source(
        &mut self,
        input: &crate::primary::AdmittedCanonicalM10,
    ) -> Result<String, Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        let receipt = input.receipt();
        if self.pending_canonical.is_some()
            || receipt.config_sha256 != self.header.config_sha256
            || self
                .last_label
                .is_some_and(|label| receipt.label_ms <= label)
        {
            return Err(Error::Invalid);
        }
        let hash = input.provenance_sha256().map_err(|_| Error::Invalid)?;
        self.append(
            "canonical_source",
            &serde_json::json!({
                "source_sha256":hash,"receipt":receipt,"rest_evidence":input.rest_evidence(),
            }),
        )?;
        self.pending_canonical = Some((receipt.label_ms, hash.clone()));
        Ok(hash)
    }

    pub(crate) fn record_recovery_source(
        &mut self,
        input: &crate::primary::RecoveryInput,
        source: &broker_finam::sparse_m10::AdmittedClosedM1Snapshot,
    ) -> Result<String, Error> {
        let receipt = input.receipt();
        let canonical = &receipt.canonical;
        if self.terminal
            || self.pending_canonical.is_some()
            || canonical.config_sha256 != self.header.config_sha256
            || canonical.origin != "RECOVERY_BACKFILL"
            || canonical.rest_snapshot_sha256.as_deref() != Some(source.sha256())
            || self
                .last_label
                .is_some_and(|label| canonical.label_ms <= label)
        {
            return Err(Error::Invalid);
        }
        if self.last_retained_source.as_deref() != Some(source.sha256()) {
            self.record_source(source)?;
        }
        let hash = input.provenance_sha256().map_err(|_| Error::Invalid)?;
        self.append(
            "canonical_source",
            &serde_json::json!({"source_sha256":hash,"receipt":receipt,
            "retained_rest_snapshot_sha256":source.sha256(),"rest_evidence":null}),
        )?;
        self.pending_canonical = Some((canonical.label_ms, hash.clone()));
        Ok(hash)
    }

    pub(crate) fn record_canonical_commit(
        &mut self,
        label: i64,
        source_sha256: &str,
        model_result_at_ms: i64,
        published_at_ms: i64,
    ) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if self.last_label != Some(label)
            || self.pending_canonical.as_ref() != Some(&(label, source_sha256.to_owned()))
            || published_at_ms < model_result_at_ms
        {
            return Err(Error::Invalid);
        }
        self.append(
            "canonical_commit",
            &serde_json::json!({"model_label_utc_ms":label,
            "source_sha256":source_sha256,"model_result_at_ms":model_result_at_ms,
            "publication_checked_at_ms":published_at_ms,"feedback_complete":true}),
        )?;
        self.pending_canonical = None;
        Ok(())
    }

    pub(crate) fn record_restart_source(
        &mut self,
        source: &crate::primary::restart::RestartSource,
    ) -> Result<String, Error> {
        if self.terminal
            || self.pending_canonical.is_some()
            || source.config_sha256 != self.header.config_sha256
            || self.last_label.is_some_and(|l| source.label_ms <= l)
        {
            return Err(Error::Invalid);
        }
        let hash = crate::seed::digest(&serde_json::to_vec(source).map_err(Error::Json)?);
        self.append(
            "restart_source",
            &serde_json::json!({"source_sha256":hash,"receipt":source}),
        )?;
        self.pending_canonical = Some((source.label_ms, hash.clone()));
        Ok(hash)
    }

    pub fn finish(&mut self, reason: &str) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::Closed);
        }
        if reason.is_empty() || reason.len() > 256 {
            self.terminal = true;
            return Err(Error::Invalid);
        }
        let reason = if self.pending_canonical.is_some() {
            "INCOMPLETE_canonical_operation"
        } else {
            reason
        };
        self.append("segment_end", &serde_json::json!({"reason":reason}))?;
        self.terminal = true;
        self.sealed = true;
        Ok(())
    }

    /// Only a completed fsync journal has a seal. This does NOT attest that its
    /// writer thread has joined; that fact is captured separately by the parent.
    pub(crate) fn seal(&self) -> Option<crate::primary::eod::JournalSeal> {
        (self.sealed && self.pending_canonical.is_none()).then(|| {
            crate::primary::eod::JournalSeal {
                sha256: format!("{:x}", self.journal_hash.clone().finalize()),
                bytes: self.bytes,
                last_sequence: self.sequence,
            }
        })
    }

    fn append<T: Serialize>(&mut self, kind: &str, event: &T) -> Result<(), Error> {
        #[cfg(test)]
        self.crash_pause(kind, true);
        let result = (|| {
            #[cfg(test)]
            if self.fail_kind == Some(kind) {
                return Err(Error::Io(io::Error::other("fixture-only write failure")));
            }
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
            self.journal_hash.update(&line);
            Ok(())
        })();
        #[cfg(test)]
        if result.is_ok() {
            self.crash_pause(kind, false);
        }
        if result.is_err() {
            self.terminal = true;
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn fail_on_kind(&mut self, kind: &'static str) {
        self.fail_kind = Some(kind);
    }

    /// Child-process crash instrumentation, compiled out of every release.
    #[cfg(test)]
    pub(crate) fn pause_for_crash(
        &mut self,
        kind: &'static str,
        before: bool,
        marker: std::path::PathBuf,
    ) {
        self.crash_pause = Some((kind, before, marker));
    }
    #[cfg(test)]
    fn crash_pause(&self, kind: &str, before: bool) {
        if let Some((target, phase, marker)) = &self.crash_pause {
            if *target == kind && *phase == before {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(marker)
                    .unwrap();
                file.write_all(b"DURABLE_CRASH_FRONTIER\n").unwrap();
                file.sync_all().unwrap();
                loop {
                    std::thread::park_timeout(std::time::Duration::from_secs(1));
                }
            }
        }
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

pub(crate) fn valid_step(step: &Step) -> bool {
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
