//! Read-only journal + post-join receipt verifier. No IO, model-state import,
//! network, activation or runtime capability is created by a successful check.
//! It shares pure schema/Decimal inspection with restart, not mutable writer
//! state or a producer PASS boolean. Strategy parity remains a separate oracle.
use super::{
    calendar::{self, CalendarBinding, CalendarInputs},
    eod::Completion,
    WsPrimaryConfig,
};
use crate::seed::digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
mod versions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceBinding {
    pub schema_version: u32,
    pub config_sha256: String,
    pub calendar_sha256: String,
    pub journal_sha256: String,
    pub completion_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    Binding,
    Bounds,
    Decode,
    Journal,
    Calendar,
    Completion,
    Unresolved,
    Source,
    Model,
    Terminal,
}
#[derive(Debug, Serialize)]
pub struct VerifiedEod {
    pub journal_sha256: String,
    pub canonical_labels: usize,
    pub ws_live_labels: usize,
    pub rest_exception_labels: usize,
    pub historical_labels: usize,
    pub semantic_eod_passed: bool,
    pub network_qualified: bool,
    pub cgroup_qualified: bool,
    pub parity_accepted: bool,
}

pub fn verify(
    config: &WsPrimaryConfig,
    binding: &EvidenceBinding,
    journal: &[u8],
    completion: &[u8],
    now: i64,
) -> Result<VerifiedEod, VerifyError> {
    let config_hash = config.fingerprint().map_err(|_| VerifyError::Binding)?;
    if binding.schema_version != 1
        || binding.config_sha256 != config_hash
        || digest(journal) != binding.journal_sha256
        || digest(completion) != binding.completion_sha256
    {
        return Err(VerifyError::Binding);
    }
    if journal.len() as u64 > config.base.max_output_bytes.min(64 * 1024 * 1024)
        || completion.len() > 16384
    {
        return Err(VerifyError::Bounds);
    }
    let done: Completion =
        serde_json::from_value(json(completion)?).map_err(|_| VerifyError::Decode)?;
    if done.config_sha256 != config_hash
        || done.calendar_sha256 != binding.calendar_sha256
        || !done.semantic_eod_passed
        || !done.qualifies()
        || done.joined_at_ms > now
        || done.stop_budget_ms != config.base.stop_timeout_ms.min(15000)
        || done.cgroup_qualified
        || done.network_qualified
    {
        return Err(VerifyError::Completion);
    }
    let utc_elapsed = done
        .joined_at_ms
        .checked_sub(done.eod_check_at_ms)
        .ok_or(VerifyError::Completion)?;
    if utc_elapsed < 0
        || utc_elapsed as u64 > done.stop_budget_ms + config.base.clock_uncertainty_ms
        || (i128::from(utc_elapsed) - i128::from(done.elapsed_stop_ms)).unsigned_abs()
            > u128::from(config.base.clock_uncertainty_ms)
    {
        return Err(VerifyError::Completion);
    }
    let text = std::str::from_utf8(journal).map_err(|_| VerifyError::Decode)?;
    let rows: Vec<Value> = text
        .lines()
        .map(|line| {
            if line.len() > 1048576 {
                return Err(VerifyError::Bounds);
            }
            super::json::parse(line).map_err(|_| VerifyError::Decode)
        })
        .collect::<Result<_, _>>()?;
    if rows.len() > 16385 {
        return Err(VerifyError::Bounds);
    }
    let calendar_rows: Vec<_> = rows
        .iter()
        .filter(|r| r["event"]["event"] == "primary_calendar_bound")
        .collect();
    if calendar_rows.len() != 1 {
        return Err(VerifyError::Calendar);
    }
    let calendar_row = calendar_rows[0];
    let ev = &calendar_row["event"]["evidence"];
    let cal_binding: CalendarBinding =
        serde_json::from_value(ev["binding"].clone()).map_err(|_| VerifyError::Calendar)?;
    let admitted = integer(ev, "admitted_at_ms")?;
    let cal = calendar::admit(
        config,
        cal_binding,
        CalendarInputs {
            model_day: string(ev, "model_day_raw_json")?.as_bytes(),
            raw_calendar: string(ev, "raw_calendar_raw_json")?.as_bytes(),
            mapping: string(ev, "mapping_raw_json")?.as_bytes(),
        },
        admitted,
    )
    .map_err(|_| VerifyError::Calendar)?;
    let bound_at = integer(&calendar_row["event"], "bound_at_ms")?;
    cal.validate_entry(bound_at)
        .map_err(|_| VerifyError::Calendar)?;
    if cal.sha256() != binding.calendar_sha256
        || ev["calendar_sha256"] != cal.sha256()
        || cal.eod_check_at_ms() != done.eod_check_at_ms
        || ev["reference_start_ms"].as_i64() != Some(cal.reference_start_ms())
        || bound_at > now
    {
        return Err(VerifyError::Calendar);
    }
    let scan = super::restart::journal::complete(config, journal, cal.required(), now)
        .map_err(|_| VerifyError::Journal)?;
    let seal = done.journal.as_ref().ok_or(VerifyError::Completion)?;
    if seal.sha256 != binding.journal_sha256
        || seal.bytes != journal.len() as u64
        || seal.last_sequence != scan.last_sequence
        || scan.header.config_sha256 != config_hash
        || scan.inputs.len() != 101
        || scan.position_lots != 0
        || scan.terminal_reason.as_deref() != Some(super::eod::PREPARED_REASON)
    {
        return Err(VerifyError::Terminal);
    }
    let last = scan.inputs.last().ok_or(VerifyError::Terminal)?;
    if last.origin != "REST_EXCEPTION" || last.reason.as_deref() != Some("SessionEnd") {
        return Err(VerifyError::Terminal);
    }
    let mut generations = BTreeMap::new();
    let mut current_generation = None;
    let mut starts = BTreeMap::new();
    let mut ready = BTreeMap::new();
    let mut shutdown = None;
    let mut prepared = None;
    let mut end_sources = 0;
    let mut audit_slots = BTreeMap::new();
    let mut any_model_step = false;
    let mut committed_so_far = 0usize;
    let mut validated_sources = BTreeMap::<u64, String>::new();
    let mut canonical_resolved = BTreeSet::new();
    let mut proof_jobs = BTreeMap::new();
    let mut proof_failures = BTreeSet::new();
    let mut canonical_origins = BTreeMap::new();
    let nonblocking = config.primary.rest_nonblocking();
    let mut soft_failures = BTreeSet::new();
    let mut versions = versions::Diagnostics::default();
    for row in &rows {
        let event = &row["event"];
        if config.primary.consistent_sources() {
            versions.inspect(row, &scan, committed_so_far)?;
        }
        let name = event["event"].as_str();
        match name {
            Some("primary_subscription_sent") => {
                let generation = number(event, "generation")?;
                let sent = integer(event, "sent_at_ms")?;
                if generation == 0
                    || number(event, "auth_generation")? == 0
                    || sent > now
                    || event["provider_ack"] != false
                    || current_generation.is_some_and(|old| generation <= old)
                    || sent < bound_at
                {
                    return Err(VerifyError::Source);
                }
                if current_generation.is_none() {
                    cal.validate_entry(sent)
                        .map_err(|_| VerifyError::Calendar)?;
                }
                if row["sequence"].as_u64() <= calendar_row["sequence"].as_u64() {
                    return Err(VerifyError::Calendar);
                }
                current_generation = Some(generation);
                generations.insert(generation, sent);
            }
            Some("primary_recovery_episode_started") => {
                let audit = &event["audit"];
                let episode = number(audit, "episode")?;
                if !(1..=3).contains(&episode) || starts.insert(episode, audit.clone()).is_some() {
                    return Err(VerifyError::Unresolved);
                }
            }
            Some("primary_recovery_episode_ready") => {
                let audit = &event["audit"];
                let episode = number(audit, "episode")?;
                if starts
                    .get(&episode)
                    .map_or(true, |s| s["original"] != audit["original"])
                    || !resolved(audit)
                    || !canonical_resolved.contains(&number(audit, "generation")?)
                    || ready.insert(episode, audit.clone()).is_some()
                {
                    return Err(VerifyError::Unresolved);
                }
            }
            Some("primary_rest_proof_scheduled") => {
                let label = integer(event, "label_ms")?;
                let issued = integer(event, "issued_at_ms")?;
                let deadline = integer(event, "deadline_ms")?;
                if !cal.required().contains(&label)
                    || issued < label + 600000 + config.primary.proof_wait_ms()
                    || deadline <= issued
                    || deadline > issued + 5000
                    || proof_jobs
                        .insert(label, current_generation.ok_or(VerifyError::Source)?)
                        .is_some()
                {
                    return Err(VerifyError::Source);
                }
            }
            Some("primary_rest_proof_failed") => {
                let label = integer(event, "label_ms")?;
                if !matches!(event["class"].as_str(), Some("Unavailable" | "Timeout"))
                    || !proof_jobs.contains_key(&label)
                    || !proof_failures.insert(label)
                {
                    return Err(VerifyError::Unresolved);
                }
            }
            Some("primary_rest_nonblocking_failed") => {
                let label = integer(event, "label_ms")?;
                let role = string(event, "role")?;
                if !nonblocking
                    || event["nonblocking"] != true
                    || event["model_input_rewritten"] != false
                    || !rest_failure(&event["class"])
                    || !(role == "Proof" && proof_jobs.contains_key(&label)
                        || role == "Audit" && audit_slots.contains_key(&label))
                    || !soft_failures.insert((role.to_owned(), label))
                {
                    return Err(VerifyError::Source);
                }
            }
            Some("primary_rest_observation") => {
                let label = integer(event, "label_ms")?;
                let source = scan
                    .sources
                    .get(string(event, "source_sha256")?)
                    .ok_or(VerifyError::Source)?;
                let a = &event["assessment"];
                // A future input in the complete scan cannot establish that
                // the label was already committed when this event was written.
                let committed = scan.inputs[..committed_so_far]
                    .iter()
                    .find(|i| i.step.model_label_utc_ms == label)
                    .map(|i| i.actual_m1.as_slice());
                verify_proof_assessment(label, a, source.bars().cloned().collect(), committed)?;
                if !nonblocking
                    || event["nonblocking"] != true
                    || event["role"] != "Proof"
                    || !proof_jobs.contains_key(&label)
                    || source.evidence().start.timestamp_millis() != label
                    || source.evidence().end.timestamp_millis() != label + 600000
                {
                    return Err(VerifyError::Source);
                }
                // Provisional WS assessments are diagnostic, never proof of
                // canonical authority or of parity. Admission is checked by
                // the independent canonical source/restart reader above.
            }
            Some("primary_recovery_source_validated") if event["scope"] == "STARTUP_ANCHOR" => {
                let source = scan
                    .sources
                    .get(string(event, "source_sha256")?)
                    .ok_or(VerifyError::Source)?;
                let issued = integer(event, "issued_at_ms")?;
                let deadline = integer(event, "deadline_ms")?;
                if any_model_step
                    || Some(number(event, "generation")?) != current_generation
                    || event["overlap_labels"] != 0
                    || event["replayed_labels"] != 0
                    || event["history_proves_live_readiness"] != false
                    || integer(event, "start_ms")? != cal.reference_start_ms()
                    || integer(event, "cutover_ms")? != cal.required()[0]
                    || source.evidence().start.timestamp_millis() != cal.reference_start_ms()
                    || source.evidence().end.timestamp_millis() != cal.required()[0]
                    || source.evidence().parts.len() != 1
                    || source.bars().next().is_none()
                    || source.bars().count() > config.primary.max_cache_m1
                    || issued < cal.required()[0]
                    || issued >= cal.required()[0] + 600000
                    || deadline <= issued
                    || deadline > issued + 5000
                    || deadline > cal.eod_check_at_ms()
                {
                    return Err(VerifyError::Source);
                }
                let part = &source.evidence().parts[0];
                if part.requested_at.timestamp_millis() < issued
                    || part.received_at.timestamp_millis() > deadline
                    || integer(event, "requested_at_ms")? != part.requested_at.timestamp_millis()
                    || integer(event, "received_at_ms")? != part.received_at.timestamp_millis()
                {
                    return Err(VerifyError::Source);
                }
            }
            Some("primary_recovery_source_validated") => {
                if (!any_model_step && integer(event, "cutover_ms")? == cal.required()[0])
                    || (!event["scope"].is_null() && event["scope"] != "MODEL_GAP")
                {
                    return Err(VerifyError::Source);
                }
                let source = scan
                    .sources
                    .get(string(event, "source_sha256")?)
                    .ok_or(VerifyError::Source)?;
                let cutover = integer(event, "cutover_ms")?;
                let end = cal.required().partition_point(|l| *l < cutover);
                let start = committed_so_far.saturating_sub(config.primary.recovery_overlap_m10);
                if end < committed_so_far
                    || end == 0
                    || source.evidence().start.timestamp_millis() != cal.required()[start]
                    || cutover != cal.required().get(end).copied().unwrap_or(cal.close_ms())
                    || event["overlap_labels"].as_u64() != Some((committed_so_far - start) as u64)
                    || event["replayed_labels"].as_u64() != Some((end - committed_so_far) as u64)
                {
                    return Err(VerifyError::Source);
                }
                for (offset, input) in scan.inputs[start..end].iter().enumerate() {
                    let committed_overlap = start + offset < committed_so_far;
                    let label =
                        chrono::DateTime::from_timestamp_millis(input.step.model_label_utc_ms)
                            .ok_or(VerifyError::Source)?;
                    let bucket = source.bucket(label).map_err(|_| VerifyError::Source)?;
                    if input.actual_m1.len() != bucket.actual_m1().len()
                        || input
                            .actual_m1
                            .iter()
                            .zip(bucket.actual_m1())
                            .any(|(a, b)| {
                                !(config.primary.consistent_sources()
                                    && committed_overlap
                                    && super::revisions::same_slot(a, b)
                                    && (super::wire::same_values(a, b)
                                        || versions.overlap(
                                            source.sha256(),
                                            a.open_ts.timestamp_millis(),
                                        )))
                                    && !super::revisions::compatible(
                                        a,
                                        b,
                                        config.primary.observed_bo()
                                            && input.step.model_label_utc_ms
                                                < cal
                                                    .required()
                                                    .get(committed_so_far)
                                                    .copied()
                                                    .unwrap_or(i64::MAX),
                                    )
                            })
                    {
                        return Err(VerifyError::Source);
                    }
                }
            }
            Some("primary_canonical_recovery_resolved") => {
                let generation = number(event, "generation")?;
                let hash = validated_sources
                    .get(&generation)
                    .ok_or(VerifyError::Unresolved)?;
                let source = scan.sources.get(hash).ok_or(VerifyError::Source)?;
                if Some(generation) != current_generation
                    || event["source_sha256"] != hash.as_str()
                    || integer(event, "cutover_ms")? != source.evidence().end.timestamp_millis()
                    || !canonical_resolved.insert(generation)
                {
                    return Err(VerifyError::Source);
                }
            }
            Some("primary_audit_scheduled")
                if audit_slots
                    .insert(integer(event, "label_ms")?, integer(event, "at_ms")?)
                    .is_some() =>
            {
                return Err(VerifyError::Source);
            }
            Some("primary_audit_result") => {
                let label = integer(event, "label_ms")?;
                let requested = *audit_slots.get(&label).ok_or(VerifyError::Source)?;
                match event["status"].as_str() {
                    Some("EvidenceUnavailable") if nonblocking => {
                        if !event["source_sha256"].is_null()
                            || event["nonblocking"] != true
                            || !rest_failure(&event["class"])
                            || !soft_failures.contains(&("Audit".to_owned(), label))
                        {
                            return Err(VerifyError::Source);
                        }
                    }
                    Some("EvidenceOnly") if nonblocking => {
                        let source = scan
                            .sources
                            .get(string(event, "source_sha256")?)
                            .ok_or(VerifyError::Source)?;
                        let input = scan
                            .inputs
                            .iter()
                            .find(|i| i.step.model_label_utc_ms == label)
                            .ok_or(VerifyError::Source)?;
                        let assessed = super::rest_observation::assess(
                            label,
                            "COMMITTED_MODEL_INPUT",
                            input.actual_m1.clone(),
                            source.bars().cloned().collect(),
                        );
                        if event["nonblocking"] != true
                            || source.evidence().start.timestamp_millis() != label
                            || source.evidence().end.timestamp_millis() != label + 600000
                            || source.evidence().parts.len() != 1
                            || source.evidence().parts.iter().any(|p| {
                                p.requested_at.timestamp_millis() < requested
                                    || p.received_at.timestamp_millis()
                                        > integer(event, "recorded_at_ms").unwrap_or(i64::MIN)
                            })
                            || serde_json::to_value(assessed).map_err(|_| VerifyError::Source)?
                                != event["assessment"]
                        {
                            return Err(VerifyError::Source);
                        }
                    }
                    Some("Unavailable" | "CancelledForStop")
                        if event["source_sha256"].is_null() => {}
                    Some("Match" | "VolumeRevision") if !nonblocking => {
                        let source = scan
                            .sources
                            .get(string(event, "source_sha256")?)
                            .ok_or(VerifyError::Source)?;
                        let input = scan
                            .inputs
                            .iter()
                            .find(|i| i.step.model_label_utc_ms == label)
                            .ok_or(VerifyError::Source)?;
                        let actual: Vec<_> = source.bars().collect();
                        let volume_revision = input
                            .actual_m1
                            .iter()
                            .zip(&actual)
                            .any(|(a, b)| a.volume != b.volume);
                        if source.evidence().start.timestamp_millis() != label
                            || source.evidence().end.timestamp_millis() != label + 600000
                            || source.evidence().parts.len() != 1
                            || source.evidence().parts.iter().any(|p| {
                                p.requested_at.timestamp_millis() < requested
                                    || p.received_at.timestamp_millis() > requested + 5000
                            })
                            || input.actual_m1.len() != actual.len()
                            || input.actual_m1.iter().zip(actual).any(|(a, b)| {
                                !super::revisions::compatible(a, b, config.primary.observed_bo())
                            })
                            || (event["status"] == "VolumeRevision") != volume_revision
                            || (volume_revision && !config.primary.observed_bo())
                        {
                            return Err(VerifyError::Source);
                        }
                    }
                    _ => return Err(VerifyError::Source),
                }
            }
            Some("primary_async_shutdown") if shutdown.replace(event).is_some() => {
                return Err(VerifyError::Terminal);
            }
            Some("primary_eod_prepared") if prepared.replace(event).is_some() => {
                return Err(VerifyError::Terminal);
            }
            _ => (),
        }
        if name == Some("primary_recovery_source_validated") {
            let generation = match event.get("generation") {
                Some(value) => value.as_u64().ok_or(VerifyError::Decode)?,
                None => current_generation.ok_or(VerifyError::Source)?,
            };
            let hash = string(event, "source_sha256")?;
            let source = scan.sources.get(hash).ok_or(VerifyError::Source)?;
            if Some(generation) != current_generation
                || source.evidence().end.timestamp_millis() != integer(event, "cutover_ms")?
                || validated_sources.insert(generation, hash.into()).is_some()
            {
                return Err(VerifyError::Source);
            }
        }
        if row["kind"] == "model_step" {
            any_model_step = true;
        }
        if row["kind"] == "canonical_commit" {
            committed_so_far += 1;
        }
        if row["kind"] == "canonical_source" {
            let receipt = &event["receipt"];
            let generation = number(receipt, "generation")?;
            if canonical_origins
                .insert(
                    integer(receipt, "label_ms")?,
                    (string(receipt, "origin")?.to_owned(), generation),
                )
                .is_some()
            {
                return Err(VerifyError::Source);
            }
            if Some(generation) != current_generation
                || generations.get(&generation).copied()
                    != Some(integer(receipt, "subscribed_at_ms")?)
                || integer(receipt, "last_component_received_at_ms")?
                    < *generations.get(&generation).ok_or(VerifyError::Source)?
            {
                return Err(VerifyError::Source);
            }
            if receipt["reason"] == "SessionEnd" {
                end_sources += 1;
                if integer(receipt, "label_ms")? != cal.last_label_ms() {
                    return Err(VerifyError::Source);
                }
            }
        }
        if row["kind"] == "canonical_commit"
            && integer(event, "publication_checked_at_ms")? > cal.eod_check_at_ms()
        {
            return Err(VerifyError::Source);
        }
    }
    if end_sources != 1 || starts.len() != ready.len() {
        return Err(VerifyError::Unresolved);
    }
    for label in proof_failures {
        let (origin, generation) = canonical_origins
            .get(&label)
            .ok_or(VerifyError::Unresolved)?;
        let scheduled_generation = proof_jobs.get(&label).ok_or(VerifyError::Unresolved)?;
        // A comparison-only outage can be discharged by the same-generation WS
        // commit, or by validated and resolved source-bound historical repair.
        if !(origin == "WS_LIVE" && generation == scheduled_generation
            || origin == "RECOVERY_BACKFILL"
                && generation > scheduled_generation
                && canonical_resolved.contains(generation)
                && ready
                    .values()
                    .any(|a| a["generation"].as_u64() == Some(*generation)))
        {
            return Err(VerifyError::Unresolved);
        }
    }
    let shutdown = shutdown.ok_or(VerifyError::Terminal)?;
    let prepared = prepared.ok_or(VerifyError::Terminal)?;
    if shutdown["cause"] != "EndOfSession"
        || shutdown["requested_calendar_eod"] != true
        || shutdown["continuity_lost"] != false
        || !shutdown["unresolved_transport_fault"].is_null()
        || shutdown["socket_drained"] != true
        || shutdown["http_drained"] != true
        || shutdown["full_session_qualified"] != false
        || integer(shutdown, "stop_started_at_ms")? != done.stop_started_at_ms
    {
        return Err(VerifyError::Unresolved);
    }
    let histories = shutdown["recovery"]
        .as_array()
        .ok_or(VerifyError::Unresolved)?;
    if histories.len() != ready.len()
        || histories.iter().any(|r| {
            !resolved(r)
                || r.get("episode")
                    .and_then(Value::as_u64)
                    .and_then(|id| ready.get(&id))
                    != Some(r)
        })
    {
        return Err(VerifyError::Unresolved);
    }
    for role in ["session_end", "recovery", "bootstrap", "proof", "audit"] {
        let counts = &shutdown["http"][role];
        let scheduled = number(counts, "scheduled")?;
        let completed = number(counts, "completed")?;
        let unaccounted = number(counts, "cancelled_or_unaccounted")?;
        let optional = nonblocking && matches!(role, "proof" | "audit");
        let role_name = if role == "proof" { "Proof" } else { "Audit" };
        let unknown = if optional {
            rows.iter()
                .filter(|r| {
                    r["event"]["event"] == "primary_http_completed"
                        && r["event"]["role"] == role_name
                        && r["event"]["receipt"]["executor_completed"] == false
                })
                .count() as u64
                + shutdown["drained_http"]
                    .as_array()
                    .ok_or(VerifyError::Completion)?
                    .iter()
                    .filter(|r| {
                        r["role"] == role_name && r["receipt"]["executor_completed"] == false
                    })
                    .count() as u64
        } else {
            0
        };
        if unaccounted != unknown
            || scheduled != completed + unaccounted
            || number(counts, "known_get_send_attempts")? > completed
        {
            return Err(VerifyError::Completion);
        }
        if role == "session_end"
            && (scheduled != 1 || number(counts, "known_get_send_attempts")? != 1)
        {
            return Err(VerifyError::Source);
        }
    }
    for drained in shutdown["drained_http"]
        .as_array()
        .ok_or(VerifyError::Completion)?
    {
        if nonblocking
            && matches!(drained["role"].as_str(), Some("Proof" | "Audit"))
            && (drained["receipt"]["auth_generation"]
                .as_u64()
                .is_some_and(|g| g > 0)
                || matches!(
                    drained["class"].as_str(),
                    Some("Timeout" | "Unavailable" | "Cancelled")
                ))
        {
            continue;
        }
        match drained["class"].as_str() {
            Some("Unavailable" | "Timeout") => (),
            None if drained["class"].is_null() => (),
            Some("Cancelled")
                if drained["receipt"]["auth_generation"]
                    .as_u64()
                    .is_some_and(|g| g > 0) => {}
            _ => return Err(VerifyError::Completion),
        }
    }
    if prepared["prepared"] != true
        || prepared["writer_join_attested"] != false
        || prepared["calendar_sha256"] != cal.sha256()
        || prepared["canonical_labels"] != 101
        || prepared["model"]["position_lots"] != 0
        || prepared["model"]["pending"] != false
        || prepared["model"]["held"] != false
        || integer(prepared, "at_ms")? < done.stop_started_at_ms
        || integer(prepared, "at_ms")? > done.joined_at_ms
    {
        return Err(VerifyError::Model);
    }
    let ws_live_labels = scan.inputs.iter().filter(|s| s.origin == "WS_LIVE").count();
    let rest_exception_labels = scan
        .inputs
        .iter()
        .filter(|s| s.origin == "REST_EXCEPTION")
        .count();
    Ok(VerifiedEod {
        journal_sha256: binding.journal_sha256.clone(),
        canonical_labels: scan.inputs.len(),
        ws_live_labels,
        rest_exception_labels,
        historical_labels: scan.inputs.len() - ws_live_labels - rest_exception_labels,
        semantic_eod_passed: true,
        network_qualified: false,
        cgroup_qualified: false,
        parity_accepted: false,
    })
}
// File-only diagnostic attestation. This is not called by the runtime owner
// and never changes nonblocking REST, model inputs or admission policy.
fn verify_proof_assessment(
    label: i64,
    assessment: &Value,
    actual: Vec<broker_core::event::Bar>,
    committed: Option<&[broker_core::event::Bar]>,
) -> Result<(), VerifyError> {
    let (basis, expected) = match assessment["known_basis"].as_str() {
        Some("COMMITTED_MODEL_INPUT") => (
            "COMMITTED_MODEL_INPUT",
            committed.ok_or(VerifyError::Source)?.to_vec(),
        ),
        Some("PROVISIONAL_WS") if committed.is_none() => (
            "PROVISIONAL_WS",
            serde_json::from_value(assessment["expected_m1"].clone())
                .map_err(|_| VerifyError::Source)?,
        ),
        _ => return Err(VerifyError::Source),
    };
    let recomputed = super::rest_observation::assess(label, basis, expected, actual);
    if serde_json::to_value(recomputed).map_err(|_| VerifyError::Source)? != *assessment {
        return Err(VerifyError::Source);
    }
    Ok(())
}

fn rest_failure(value: &Value) -> bool {
    matches!(
        value.as_str(),
        Some("Timeout" | "Unavailable" | "Cancelled" | "Integrity" | "Authentication" | "Unknown")
    )
}
fn resolved(audit: &Value) -> bool {
    audit["stage"] == "Ready"
        && audit["transport_unresolved"] == false
        && audit["canonical_unresolved"] == false
        && audit["terminal"].is_null()
}
fn json(bytes: &[u8]) -> Result<Value, VerifyError> {
    super::json::parse(std::str::from_utf8(bytes).map_err(|_| VerifyError::Decode)?)
        .map_err(|_| VerifyError::Decode)
}
fn number(value: &Value, key: &str) -> Result<u64, VerifyError> {
    value[key].as_u64().ok_or(VerifyError::Decode)
}
fn integer(value: &Value, key: &str) -> Result<i64, VerifyError> {
    value[key].as_i64().ok_or(VerifyError::Decode)
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, VerifyError> {
    value[key].as_str().ok_or(VerifyError::Decode)
}

#[cfg(test)]
mod proof_basis_tests {
    use super::*;
    use broker_core::event::Bar;
    use serde_json::json;

    fn bar() -> Bar {
        serde_json::from_value(json!({
            "instrument":{"exchange":"Moex","market":"Futures","symbol":"IMOEXF","venue_symbol":"IMOEXF@RTSX"},
            "timeframe_sec":60,"open_ts":"2026-11-10T04:00:00Z","close_ts":"2026-11-10T04:01:00Z",
            "open":"2469","high":"2470.5","low":"2461.5","close":"2464.5","volume":"5592",
            "is_final":false,"source_kind":"LiveStream"
        })).unwrap()
    }

    fn assessment(basis: &'static str, expected: Vec<Bar>, actual: Vec<Bar>) -> Value {
        serde_json::to_value(super::super::rest_observation::assess(
            1794283200000,
            basis,
            expected,
            actual,
        ))
        .unwrap()
    }

    #[test]
    fn committed_basis_uses_real_input_not_a_self_consistent_forged_expected() {
        let original = vec![bar()];
        let mut forged = original.clone();
        forged[0].volume += rust_decimal::Decimal::ONE;
        let a = assessment("COMMITTED_MODEL_INPUT", forged, original.clone());
        assert_eq!(
            verify_proof_assessment(1794283200000, &a, original.clone(), Some(&original)),
            Err(VerifyError::Source)
        );
    }

    #[test]
    fn committed_basis_requires_a_preceding_commit_of_the_same_label() {
        let bars = vec![bar()];
        let a = assessment("COMMITTED_MODEL_INPUT", bars.clone(), bars.clone());
        assert_eq!(
            verify_proof_assessment(1794283200000, &a, bars.clone(), None),
            Err(VerifyError::Source)
        );
        assert_eq!(
            verify_proof_assessment(1794283800000, &a, bars.clone(), Some(&bars)),
            Err(VerifyError::Source)
        );
    }

    #[test]
    fn committed_match_and_volume_revision_attest_without_rewriting_inputs() {
        let bars = vec![bar()];
        for revision in [false, true] {
            let mut actual = bars.clone();
            if revision {
                actual[0].volume += rust_decimal::Decimal::ONE;
            }
            let a = assessment("COMMITTED_MODEL_INPUT", bars.clone(), actual.clone());
            assert_eq!(a["model_input_rewritten"], false);
            assert_eq!(
                verify_proof_assessment(1794283200000, &a, actual, Some(&bars)),
                Ok(())
            );
        }
        assert_eq!(bars[0].volume, rust_decimal::Decimal::from(5592));
    }

    #[test]
    fn provisional_is_diagnostic_only_and_cannot_claim_an_already_committed_label() {
        let bars = vec![bar()];
        let a = assessment("PROVISIONAL_WS", bars.clone(), bars.clone());
        assert_eq!(
            verify_proof_assessment(1794283200000, &a, bars.clone(), None),
            Ok(())
        );
        assert_eq!(
            verify_proof_assessment(1794283200000, &a, bars.clone(), Some(&bars)),
            Err(VerifyError::Source)
        );
    }
}
