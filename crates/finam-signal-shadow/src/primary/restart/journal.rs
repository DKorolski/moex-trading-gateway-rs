use super::super::json as unique;
use super::*;
use crate::{observer::Decision, segment::ObservationMode, ValidatedConfig};
use broker_finam::sparse_m10::{
    AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1,
};
use chrono::{DateTime, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::BTreeMap;

// Owned persisted schema, in the SAME serialization order as CanonicalReceipt.
// Recomputing its typed digest is intentionally not hashing a JSON object map.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Canonical {
    config_sha256: String,
    policy_sha256: String,
    origin: String,
    reason: Option<String>,
    label_ms: i64,
    close_ms: i64,
    generation: u64,
    subscribed_at_ms: i64,
    last_component_received_at_ms: i64,
    boundary_received_at_ms: Option<i64>,
    proof_received_at_ms: Option<i64>,
    rest_snapshot_sha256: Option<String>,
    canonical_ready_at_ms: i64,
    minute_bitmap: u16,
    canonical_digest: String,
    actual_m1: Vec<Bar>,
    source_frame_sha256: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    #[serde(flatten)]
    canonical: Canonical,
    recovery_plan_sha256: String,
    cutover_ms: i64,
    actual_requested_at_ms: i64,
    actual_received_at_ms: i64,
    virtual_callback_at_ms: i64,
    replay_deadline_ms: i64,
}
struct Pending {
    origin: String,
    reason: Option<String>,
    bar: HybridRuntimeBarEvent,
    rows: Vec<Bar>,
    bitmap: u16,
    hash: String,
    ready: i64,
    historical: bool,
    step: Option<Step>,
    feedback: Option<(CandleFill, Value)>,
    publication: Option<i64>,
    step_clock: Option<[i64; 4]>,
}

pub(super) fn admit(
    config: &super::super::WsPrimaryConfig,
    binding: RestartBinding,
    bytes: &[u8],
    required: &[i64],
    now: i64,
) -> Result<CommittedPrefix, RestartError> {
    let result = scan(config, &binding, bytes, required, now, false)?;
    Ok(CommittedPrefix {
        binding,
        header: result.header,
        required: required.to_vec(),
        inputs: result.inputs,
        admitted_at_ms: now,
    })
}
pub(crate) struct JournalScan {
    pub header: SeedObservation,
    pub inputs: Vec<CommittedInput>,
    pub terminal_reason: Option<String>,
    pub last_sequence: u64,
    pub position_lots: i64,
    pub sources: BTreeMap<String, AdmittedClosedM1Snapshot>,
}
/// Complete file inspection creates no restart/runtime capability, and uses
/// the CURRENT segment identity rather than synthesizing a successor config.
pub(crate) fn complete(
    config: &super::super::WsPrimaryConfig,
    bytes: &[u8],
    required: &[i64],
    now: i64,
) -> Result<JournalScan, RestartError> {
    let config_sha256 = config.fingerprint().map_err(|_| RestartError::Binding)?;
    let expected = RestartBinding {
        schema_version: 1,
        config_sha256: config_sha256.clone(),
        previous_config_sha256: config_sha256,
        previous_journal_sha256: digest(bytes),
        previous_segment_id: config.base.segment_id.clone(),
        required_labels_sha256: inventory_sha256(required),
        committed_labels: required.len(),
        max_reconstruction_ms: 10000,
    };
    scan(config, &expected, bytes, required, now, true)
}
fn scan(
    config: &super::super::WsPrimaryConfig,
    binding: &RestartBinding,
    bytes: &[u8],
    required: &[i64],
    now: i64,
    full: bool,
) -> Result<JournalScan, RestartError> {
    let validated = ValidatedConfig::from_ws_primary(config).map_err(|_| RestartError::Binding)?;
    let c = validated.config();
    if binding.schema_version != 1
        || binding.config_sha256 != validated.fingerprint()
        || !hash(&binding.previous_config_sha256)
        || !hash(&binding.previous_journal_sha256)
        || (!full
            && (c.previous_segment_id.as_deref() != Some(binding.previous_segment_id.as_str())
                || c.segment_id == binding.previous_segment_id))
        || (full && c.segment_id != binding.previous_segment_id)
        || binding.previous_segment_id.is_empty()
        || digest(bytes) != binding.previous_journal_sha256
        || inventory_sha256(required) != binding.required_labels_sha256
    {
        return Err(RestartError::Binding);
    }
    if bytes.is_empty()
        || bytes.len() as u64
            > if full {
                c.max_output_bytes.min(64 * 1024 * 1024)
            } else {
                c.max_input_bytes.min(16 * 1024 * 1024)
            }
        || !bytes.ends_with(b"\n")
        || required.is_empty()
        || required.len() > 101
        || required[0] != c.observation_start_utc_ms
        || required[0] < 0
        || required[0] % 600000 != 0
        || required
            .windows(2)
            .any(|p| p[1].checked_sub(p[0]) != Some(600000))
        || if full {
            binding.committed_labels != required.len()
        } else {
            binding.committed_labels >= required.len()
        }
        || !(1..=10000).contains(&binding.max_reconstruction_ms)
    {
        return Err(RestartError::Bounds);
    }
    if now
        < if full {
            required.last().unwrap() + 600000
        } else {
            required[binding.committed_labels]
        }
        || (!full && now >= c.run_deadline_utc_ms)
        || required
            .last()
            .and_then(|l| l.checked_add(600000))
            .map_or(true, |t| t > c.run_deadline_utc_ms)
    {
        return Err(RestartError::Clock);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RestartError::Journal)?;
    let mut lines = text.lines();
    let first = parse(lines.next().ok_or(RestartError::Journal)?)?;
    if first["sequence"].as_u64() != Some(0)
        || first["kind"] != "seed"
        || first.as_object().map_or(true, |m| m.len() != 3)
    {
        return Err(RestartError::Journal);
    }
    let header: SeedObservation =
        serde_json::from_value(first["observation"].clone()).map_err(|_| RestartError::Journal)?;
    if header.config_sha256 != binding.previous_config_sha256
        || header.semantic_config_sha256 != validated.semantic_fingerprint()
        || header.run_id != c.run_id
        || header.segment_id != binding.previous_segment_id
        || header.observation_start_utc_ms != c.observation_start_utc_ms
        || header.seed_sha256 != c.seed_sha256
        || header.model_sha256 != c.model_sha256
        || header.reference_sha256 != c.reference_sha256
        || header.feedback_policy_sha256 != c.feedback_policy_sha256
        || header.mode != ObservationMode::Warmup
        || header.live_observation_count != 0
        || header.virtual_position_lots != 0
        || (full && header.previous_segment_id != c.previous_segment_id)
    {
        return Err(RestartError::Binding);
    }
    let mut sources = BTreeMap::<String, AdmittedClosedM1Snapshot>::new();
    let mut pending: Option<Pending> = None;
    let mut inputs = Vec::new();
    let mut ended = false;
    let mut sequence = 0u64;
    let mut state = header.state.clone();
    let policy = config
        .primary
        .fingerprint()
        .map_err(|_| RestartError::Binding)?;
    let mut last_fill_sequence = 0;
    let mut position_lots = 0i64;
    let mut terminal_reason = None;
    let mut reconstruction: Option<RestartBinding> = None;
    let mut reconstructed_sources = 0;
    for line in lines {
        if ended || sequence >= 16384 {
            return Err(RestartError::Journal);
        }
        let row = parse(line)?;
        sequence += 1;
        if row["sequence"].as_u64() != Some(sequence)
            || row.as_object().map_or(true, |m| m.len() != 11)
        {
            return Err(RestartError::Journal);
        }
        for (key, expected) in [
            ("run_id", &header.run_id),
            ("segment_id", &header.segment_id),
            ("config_sha256", &header.config_sha256),
            ("seed_sha256", &header.seed_sha256),
            ("semantic_config_sha256", &header.semantic_config_sha256),
            ("model_sha256", &header.model_sha256),
            ("reference_sha256", &header.reference_sha256),
            ("feedback_policy_sha256", &header.feedback_policy_sha256),
        ] {
            if row[key].as_str() != Some(expected.as_str()) {
                return Err(RestartError::Binding);
            }
        }
        let event = &row["event"];
        // The single-journal restart/EOD protocol cannot attest a cohort's
        // shared publication frontier using only one consumer's local commit.
        if event["event"] == "primary_runtime_cohort_bound" {
            return Err(RestartError::Incomplete);
        }
        match row["kind"].as_str().ok_or(RestartError::Journal)? {
            "source_snapshot" => {
                let evidence: ClosedM1SnapshotEvidenceV1 =
                    serde_json::from_value(event["evidence"].clone())
                        .map_err(|_| RestartError::Source)?;
                let source = admit_rest(evidence, now)?;
                if event["snapshot_sha256"].as_str() != Some(source.sha256()) {
                    return Err(RestartError::Source);
                }
                sources.insert(source.sha256().into(), source);
            }
            "canonical_source" | "restart_source" => {
                if pending.is_some() || inputs.len() >= binding.committed_labels {
                    return Err(RestartError::Incomplete);
                }
                let label = required[inputs.len()];
                let source_hash = event["source_sha256"]
                    .as_str()
                    .filter(|s| hash(s))
                    .ok_or(RestartError::Source)?
                    .to_owned();
                let p = if row["kind"] == "restart_source" {
                    let anchor = reconstruction.as_ref().ok_or(RestartError::Binding)?;
                    if inputs.len() != reconstructed_sources
                        || reconstructed_sources >= anchor.committed_labels
                    {
                        return Err(RestartError::Incomplete);
                    }
                    let r: RestartSource = serde_json::from_value(event["receipt"].clone())
                        .map_err(|_| RestartError::Source)?;
                    if r.config_sha256 != header.config_sha256
                        || r.policy_sha256 != policy
                        || r.origin != "RESTART_BACKFILL"
                        || !hash(&r.previous_journal_sha256)
                        || !hash(&r.previous_source_sha256)
                        || r.previous_journal_sha256 != anchor.previous_journal_sha256
                        || r.label_ms != label
                        || r.close_ms != label + 600000
                        || r.virtual_received_at_ms < r.close_ms
                        || r.virtual_ready_at_ms < r.virtual_received_at_ms
                        || r.virtual_callback_at_ms < r.virtual_ready_at_ms
                        || r.virtual_result_at_ms < r.virtual_callback_at_ms
                        || r.reconstructed_at_ms < r.virtual_result_at_ms
                        || r.reconstructed_at_ms > now
                        || typed_hash(&r)? != source_hash
                    {
                        return Err(RestartError::Source);
                    }
                    let bar = math(&r.actual_m1, label, r.minute_bitmap, &r.canonical_digest)?;
                    reconstructed_sources += 1;
                    Pending {
                        origin: "RESTART_BACKFILL".into(),
                        reason: None,
                        bar,
                        rows: r.actual_m1,
                        bitmap: r.minute_bitmap,
                        hash: source_hash,
                        ready: r.reconstructed_at_ms,
                        historical: true,
                        step: None,
                        feedback: None,
                        publication: None,
                        step_clock: Some([
                            r.virtual_received_at_ms,
                            r.virtual_ready_at_ms,
                            r.virtual_callback_at_ms,
                            r.virtual_result_at_ms,
                        ]),
                    }
                } else {
                    if reconstruction
                        .as_ref()
                        .is_some_and(|anchor| reconstructed_sources != anchor.committed_labels)
                    {
                        return Err(RestartError::Incomplete);
                    }
                    canonical(
                        event,
                        &source_hash,
                        label,
                        (
                            &header.config_sha256,
                            &policy,
                            full && Some(label) == required.last().copied(),
                        ),
                        now,
                        &validated,
                        &sources,
                    )?
                };
                pending = Some(p);
            }
            "model_step" => {
                let p = pending.as_mut().ok_or(RestartError::Journal)?;
                let step: Step =
                    serde_json::from_value(event.clone()).map_err(|_| RestartError::Journal)?;
                if p.step.is_some()
                    || !crate::observer::valid_step(&step)
                    || step.instrument != "IMOEXF"
                    || step.model_label_utc_ms != p.bar.close_time_utc * 1000
                    || step.raw_provenance_sha256 != p.hash
                    || step.canonical_input_sha256
                        != crate::input::canonical_digest(&p.bar)
                            .map_err(|_| RestartError::Source)?
                    || step.mode
                        != if p.historical {
                            ObservationMode::BackfillReplay
                        } else {
                            ObservationMode::LiveObserved
                        }
                    || step.before != state
                    || step.result_at_ms > now
                    || p.step_clock.is_some_and(|clock| {
                        clock
                            != [
                                step.received_at_ms,
                                step.canonical_ready_at_ms,
                                step.model_start_at_ms,
                                step.result_at_ms,
                            ]
                    })
                    || (!p.historical && step.canonical_ready_at_ms < p.ready)
                    || (!p.historical
                        && validated.primary_policy().is_some_and(|p| p.observed_bo())
                        && step.result_at_ms - step.nominal_close_utc_ms
                            + validated.config().clock_uncertainty_ms as i64
                            > 2000)
                {
                    return Err(RestartError::Journal);
                }
                if Decision::from_model_observation(
                    step.decision.action,
                    step.decision.quantity_lots,
                    &step.after,
                )
                .map_err(|_| RestartError::Model)?
                    != step.decision
                {
                    return Err(RestartError::Model);
                }
                state = step.after.clone();
                p.step = Some(step);
            }
            "hypothetical_candle_feedback" => {
                let p = pending.as_mut().ok_or(RestartError::Journal)?;
                let step = p.step.as_ref().ok_or(RestartError::Journal)?;
                let fill: CandleFill = serde_json::from_value(event["fill"].clone())
                    .map_err(|_| RestartError::Journal)?;
                if p.feedback.is_some()
                    || event["mode"]
                        != serde_json::to_value(step.mode).map_err(|_| RestartError::Journal)?
                    || event["broker_execution"] != false
                    || event["quotes_observed"] != false
                    || fill.policy != crate::feedback::CANDLE_POLICY
                    || Some(fill.request.action) != step.decision.action
                    || fill.request.quantity_lots != 1
                    || fill.request.quantity_lots != step.decision.quantity_lots
                    || fill.request.instrument != "IMOEXF"
                    || fill.request.sequence != last_fill_sequence + 1
                    || fill.model_label_utc_ms != step.model_label_utc_ms
                    || fill.nominal_close_utc_ms != step.nominal_close_utc_ms
                    || fill.canonical_input_sha256 != step.canonical_input_sha256
                    || fill.raw_provenance_sha256 != p.hash
                    || fill.received_at_ms != step.received_at_ms
                    || fill.request.decision_at_ms < step.result_at_ms
                    || fill.applied_at_ms < fill.request.decision_at_ms
                    || fill.applied_at_ms > now
                    || Some(fill.price_ticks)
                        != p.rows
                            .last()
                            .and_then(|r| r.close.checked_mul(Decimal::new(2, 0)))
                            .filter(|v| v.fract().is_zero())
                            .and_then(|v| v.to_i64())
                    || !event["state_after"].is_object()
                {
                    return Err(RestartError::Journal);
                }
                let expected_position = match fill.request.action {
                    crate::feedback::Action::EnterLong if position_lots == 0 => 1,
                    crate::feedback::Action::EnterShort if position_lots == 0 => -1,
                    crate::feedback::Action::Exit if position_lots != 0 => 0,
                    _ => return Err(RestartError::Model),
                };
                if fill.position_lots != expected_position
                    || fill.average_price_ticks
                        != if expected_position == 0 {
                            None
                        } else {
                            Some(fill.price_ticks)
                        }
                    || event["state_after"]["last_position_qty"].as_f64()
                        != Some(expected_position as f64)
                {
                    return Err(RestartError::Model);
                }
                position_lots = expected_position;
                last_fill_sequence = fill.request.sequence;
                state = event["state_after"].clone();
                p.feedback = Some((fill, state.clone()));
            }
            "canonical_commit" => {
                let p = pending.take().ok_or(RestartError::Journal)?;
                let step = p.step.ok_or(RestartError::Incomplete)?;
                let result = integer(event, "model_result_at_ms")?;
                let published = integer(event, "publication_checked_at_ms")?;
                if step.decision.action.is_some() != p.feedback.is_some()
                    || p.publication.is_none()
                    || event["model_label_utc_ms"] != step.model_label_utc_ms
                    || event["source_sha256"] != p.hash
                    || event["feedback_complete"] != true
                    || result
                        < if p.historical {
                            p.ready
                        } else {
                            step.result_at_ms
                        }
                    || published < result
                    || published < p.publication.unwrap()
                    || published > now
                    || (!p.historical
                        && published - step.nominal_close_utc_ms + c.clock_uncertainty_ms as i64
                            > c.lmax_ms as i64)
                    || (!p.historical
                        && validated.primary_policy().is_some_and(|p| p.observed_bo())
                        && published - step.nominal_close_utc_ms + c.clock_uncertainty_ms as i64
                            > 2000)
                {
                    return Err(RestartError::Incomplete);
                }
                inputs.push(CommittedInput {
                    origin: p.origin,
                    reason: p.reason,
                    bar: p.bar,
                    actual_m1: p.rows,
                    minute_bitmap: p.bitmap,
                    source_sha256: p.hash,
                    step,
                    feedback: p.feedback,
                });
            }
            "transport_event" => {
                let name = event["event"].as_str().ok_or(RestartError::Journal)?;
                if [
                    "model_step_durable_publication",
                    "recovery_step_durable_publication",
                    "restart_step_durable_publication",
                ]
                .contains(&name)
                {
                    let p = pending.as_mut().ok_or(RestartError::Journal)?;
                    let step = p.step.as_ref().ok_or(RestartError::Incomplete)?;
                    let at = if name == "model_step_durable_publication" {
                        integer(event, "published_at_ms")?
                    } else {
                        integer(event, "actual_model_result_at_ms")?
                    };
                    if p.publication.is_some()
                        || event["model_label_utc_ms"] != step.model_label_utc_ms
                        || at
                            < if p.historical {
                                p.ready
                            } else {
                                step.result_at_ms
                            }
                        || at > now
                    {
                        return Err(RestartError::Journal);
                    }
                    p.publication = Some(at);
                }
            }
            "segment_end" => {
                if pending.is_some() {
                    return Err(RestartError::Incomplete);
                }
                ended = true;
                let reason = event["reason"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 256)
                    .ok_or(RestartError::Journal)?;
                terminal_reason = Some(reason.to_owned());
            }
            "run_plan" => {
                if pending.is_some() {
                    return Err(RestartError::Journal);
                }
                if let Some(value) = event.get("restart_binding") {
                    let anchor: RestartBinding =
                        serde_json::from_value(value.clone()).map_err(|_| RestartError::Binding)?;
                    if reconstruction.is_some()
                        || !inputs.is_empty()
                        || anchor.schema_version != 1
                        || anchor.config_sha256 != header.config_sha256
                        || header.previous_segment_id.as_deref()
                            != Some(anchor.previous_segment_id.as_str())
                        || !hash(&anchor.previous_config_sha256)
                        || !hash(&anchor.previous_journal_sha256)
                        || anchor.required_labels_sha256 != binding.required_labels_sha256
                        || anchor.committed_labels >= required.len()
                        || !(1..=10000).contains(&anchor.max_reconstruction_ms)
                    {
                        return Err(RestartError::Binding);
                    }
                    reconstruction = Some(anchor);
                }
            }
            _ => return Err(RestartError::Journal),
        }
    }
    if pending.is_some()
        || inputs.len() != binding.committed_labels
        || reconstruction
            .as_ref()
            .is_some_and(|anchor| reconstructed_sources != anchor.committed_labels)
    {
        return Err(RestartError::Incomplete);
    }
    crate::primary::verify::tail::validate(config, bytes, required, &sources, now)
        .map_err(|_| RestartError::Journal)?;
    Ok(JournalScan {
        header,
        inputs,
        terminal_reason,
        last_sequence: sequence,
        position_lots,
        sources,
    })
}

fn canonical(
    event: &Value,
    source_hash: &str,
    label: i64,
    identity: (&str, &str, bool),
    now: i64,
    validated: &ValidatedConfig,
    sources: &BTreeMap<String, AdmittedClosedM1Snapshot>,
) -> Result<Pending, RestartError> {
    let (config, policy, session_end_allowed) = identity;
    let value = &event["receipt"];
    let historical = value["origin"] == "RECOVERY_BACKFILL";
    let (r, digest) = if historical {
        let r: Recovery =
            serde_json::from_value(value.clone()).map_err(|_| RestartError::Source)?;
        if !hash(&r.recovery_plan_sha256)
            || r.cutover_ms < r.canonical.close_ms
            || r.cutover_ms % 600000 != 0
            || r.virtual_callback_at_ms != r.canonical.close_ms
            || r.actual_requested_at_ms > r.actual_received_at_ms
            || Some(r.actual_received_at_ms) != r.canonical.proof_received_at_ms
            || r.actual_received_at_ms > r.canonical.canonical_ready_at_ms
            || r.replay_deadline_ms < r.canonical.canonical_ready_at_ms
        {
            return Err(RestartError::Source);
        }
        let hash = typed_hash(&r)?;
        (r.canonical, hash)
    } else {
        let r: Canonical =
            serde_json::from_value(value.clone()).map_err(|_| RestartError::Source)?;
        let hash = typed_hash(&r)?;
        (r, hash)
    };
    if digest != source_hash
        || r.config_sha256 != config
        || r.policy_sha256 != policy
        || r.label_ms != label
        || r.close_ms != label + 600000
        || r.generation == 0
        || r.subscribed_at_ms < 0
        || r.subscribed_at_ms > r.canonical_ready_at_ms
        || r.canonical_ready_at_ms < r.close_ms
        || r.canonical_ready_at_ms > now
        || r.last_component_received_at_ms < 0
        || r.last_component_received_at_ms > r.canonical_ready_at_ms
    {
        return Err(RestartError::Source);
    }
    let bar = math(&r.actual_m1, label, r.minute_bitmap, &r.canonical_digest)?;
    match r.origin.as_str() {
        "WS_LIVE" => {
            let boundary = r.boundary_received_at_ms.ok_or(RestartError::Source)?;
            if r.reason.is_some()
                || r.minute_bitmap != 1023
                || r.subscribed_at_ms > label
                || boundary < r.close_ms
                || boundary
                    .checked_add(
                        validated
                            .primary_policy()
                            .ok_or(RestartError::Binding)?
                            .close_grace_ms as i64,
                    )
                    .map_or(true, |t| t > r.canonical_ready_at_ms)
                || r.rest_snapshot_sha256.is_some()
                || r.proof_received_at_ms.is_some()
                || r.source_frame_sha256.len() != 10
                || r.source_frame_sha256.iter().any(|s| !hash(s))
            {
                return Err(RestartError::Source);
            }
        }
        "REST_EXCEPTION" | "RECOVERY_BACKFILL" => {
            if (historical && r.reason.is_some())
                || !(historical
                    || matches!(
                        r.reason.as_deref(),
                        Some("StartupPartial" | "SparseUnproven" | "BoundaryUnproven")
                    )
                    || (session_end_allowed && r.reason.as_deref() == Some("SessionEnd")))
                    && !(validated
                        .primary_policy()
                        .is_some_and(|p| p.consistent_sources())
                        && !historical
                        && matches!(
                            r.reason.as_deref(),
                            Some("WsGracePending" | "ProvisionalRevision")
                        ))
            {
                return Err(RestartError::Source);
            }
            r.proof_received_at_ms
                .filter(|t| *t >= r.close_ms && *t <= r.canonical_ready_at_ms)
                .ok_or(RestartError::Source)?;
            let source = if historical {
                sources
                    .get(
                        r.rest_snapshot_sha256
                            .as_deref()
                            .ok_or(RestartError::Source)?,
                    )
                    .ok_or(RestartError::Source)?
            } else {
                // The direct proof is retained inline; see the separate branch below.
                return inline_proof(event, r, bar, source_hash, now, validated);
            };
            if value["cutover_ms"].as_i64() != Some(source.evidence().end.timestamp_millis())
                || value["actual_requested_at_ms"].as_i64()
                    != source
                        .evidence()
                        .parts
                        .iter()
                        .map(|p| p.requested_at.timestamp_millis())
                        .min()
            {
                return Err(RestartError::Source);
            }
            compare(source, &r)?;
        }
        _ => return Err(RestartError::Source),
    }
    Ok(Pending {
        origin: r.origin.clone(),
        reason: r.reason.clone(),
        bar,
        rows: r.actual_m1,
        bitmap: r.minute_bitmap,
        hash: source_hash.into(),
        ready: r.canonical_ready_at_ms,
        historical,
        step: None,
        feedback: None,
        publication: None,
        step_clock: historical.then_some([r.close_ms; 4]),
    })
}
fn inline_proof(
    event: &Value,
    r: Canonical,
    bar: HybridRuntimeBarEvent,
    source_hash: &str,
    now: i64,
    validated: &ValidatedConfig,
) -> Result<Pending, RestartError> {
    let source = admit_rest(
        serde_json::from_value(event["rest_evidence"].clone()).map_err(|_| RestartError::Source)?,
        now,
    )?;
    let part = source
        .evidence()
        .parts
        .first()
        .ok_or(RestartError::Source)?;
    let policy = validated.primary_policy().ok_or(RestartError::Binding)?;
    if r.reason.as_deref() == Some("WsGracePending") {
        let boundary = r.boundary_received_at_ms.ok_or(RestartError::Source)?;
        if !policy.consistent_sources()
            || boundary < r.close_ms
            || boundary > r.canonical_ready_at_ms
            || r.canonical_ready_at_ms
                >= boundary
                    .max(r.close_ms)
                    .checked_add(policy.close_grace_ms as i64)
                    .ok_or(RestartError::Source)?
        {
            return Err(RestartError::Source);
        }
    } else if policy.consistent_sources() && r.boundary_received_at_ms.is_some() {
        return Err(RestartError::Source);
    }
    if part.requested_at.timestamp_millis() < r.close_ms + policy.proof_wait_ms()
        || (part.received_at - part.requested_at).num_milliseconds() > 5000
        || (policy.observed_bo()
            && r.canonical_ready_at_ms - r.close_ms
                + validated.config().clock_uncertainty_ms as i64
                > 2000)
    {
        return Err(RestartError::Source);
    }
    if source.evidence().start.timestamp_millis() != r.label_ms
        || source.evidence().end.timestamp_millis() != r.close_ms
    {
        return Err(RestartError::Source);
    }
    compare(&source, &r)?;
    Ok(Pending {
        origin: r.origin.clone(),
        reason: r.reason.clone(),
        bar,
        rows: r.actual_m1,
        bitmap: r.minute_bitmap,
        hash: source_hash.into(),
        ready: r.canonical_ready_at_ms,
        historical: false,
        step: None,
        feedback: None,
        publication: None,
        step_clock: None,
    })
}
fn compare(source: &AdmittedClosedM1Snapshot, r: &Canonical) -> Result<(), RestartError> {
    let bucket = source
        .bucket(at(r.label_ms)?)
        .map_err(|_| RestartError::Source)?;
    if Some(source.sha256()) != r.rest_snapshot_sha256.as_deref()
        || bucket.actual_m1().len() != r.actual_m1.len()
        || source
            .evidence()
            .parts
            .iter()
            .map(|p| p.received_at.timestamp_millis())
            .max()
            != r.proof_received_at_ms
        || bucket
            .actual_m1()
            .iter()
            .zip(&r.actual_m1)
            .any(|(a, b)| !super::super::wire::same_values(a, b))
    {
        return Err(RestartError::Source);
    }
    Ok(())
}
fn admit_rest(
    e: ClosedM1SnapshotEvidenceV1,
    now: i64,
) -> Result<AdmittedClosedM1Snapshot, RestartError> {
    let plan = ClosedM1RequestPlan::new(e.start, e.end, e.parts.iter().map(|p| p.end).collect())
        .map_err(|_| RestartError::Source)?;
    // A retained pinned response is checked AS OF collection, not resubmitted
    // as a fresh current observation. Do not modify any raw/request/receive
    // times. This private path returns only historical prefix capabilities.
    let collected = e
        .parts
        .iter()
        .map(|p| p.received_at.timestamp_millis())
        .max()
        .ok_or(RestartError::Source)?;
    if collected > now {
        return Err(RestartError::Source);
    }
    AdmittedClosedM1Snapshot::admit(e, &plan, at(collected)?).map_err(|_| RestartError::Source)
}
pub(crate) fn math(
    rows: &[Bar],
    label: i64,
    bitmap: u16,
    digest: &str,
) -> Result<HybridRuntimeBarEvent, RestartError> {
    let instrument = broker_core::InstrumentId {
        symbol: "IMOEXF".into(),
        venue_symbol: Some("IMOEXF@RTSX".into()),
        exchange: broker_core::Exchange::Moex,
        market: broker_core::Market::Futures,
    };
    for row in rows {
        super::super::wire::check_bar(row, label + 600000).map_err(|_| RestartError::Source)?;
    }
    let values = broker_core::canonical_m1::aggregate_m1_window(&instrument, at(label)?, rows)
        .map_err(|_| RestartError::Source)?;
    let bar = super::super::cache::runtime_bar(&values.provisional_bar)
        .map_err(|_| RestartError::Source)?;
    if values.minute_bitmap != bitmap
        || crate::input::canonical_digest(&bar).map_err(|_| RestartError::Source)? != digest
    {
        return Err(RestartError::Source);
    }
    Ok(bar)
}
fn at(ms: i64) -> Result<DateTime<Utc>, RestartError> {
    DateTime::from_timestamp_millis(ms).ok_or(RestartError::Clock)
}
fn integer(value: &Value, key: &str) -> Result<i64, RestartError> {
    value[key].as_i64().ok_or(RestartError::Journal)
}
fn typed_hash(value: &impl Serialize) -> Result<String, RestartError> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|_| RestartError::Source)?,
    ))
}
fn parse(line: &str) -> Result<Value, RestartError> {
    if line.is_empty() || line.len() > 1048576 {
        return Err(RestartError::Bounds);
    }
    unique::parse(line).map_err(|_| RestartError::Journal)
}
