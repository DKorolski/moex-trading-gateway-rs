//! Recompute V5 diagnostic links. None of these records can replace the
//! original canonical source, which the restart reader checks independently.
use super::*;
use crate::primary::{revisions::same_slot, wire::same_values};
use broker_core::event::Bar;
type Installation = (u64, String, String, String); // generation, role, plan, source

#[derive(Default)]
pub(super) struct Diagnostics {
    seen_sources: BTreeSet<String>,
    superseded: Vec<(Bar, Bar, String, Value)>,
    overlaps: BTreeSet<(String, i64)>,
    canonical_rest: BTreeMap<String, String>,
    reference: BTreeMap<i64, Bar>,
    reference_ranges: Vec<(i64, i64)>,
    quarantined: BTreeSet<i64>,
    current_generation: Option<u64>,
    bootstrap_requests: BTreeMap<(u64, String), Value>,
    installations: BTreeSet<Installation>,
    pending_anchors: BTreeSet<Installation>,
}

#[cfg(test)]
mod tests;
impl Diagnostics {
    pub fn overlap(&self, source: &str, minute: i64) -> bool {
        self.overlaps.contains(&(source.into(), minute))
    }
    pub fn inspect(
        &mut self,
        row: &Value,
        scan: &super::super::restart::journal::JournalScan,
        committed: usize,
        now: i64,
    ) -> Result<(), VerifyError> {
        let event = &row["event"];
        if event["event"] == "primary_subscription_sent" {
            self.current_generation = Some(number(event, "generation")?);
        }
        if event["event"] == "primary_bootstrap_proof_required" {
            let generation = number(event, "generation")?;
            if self.current_generation != Some(generation)
                || self
                    .bootstrap_requests
                    .insert(
                        (generation, string(event, "plan_sha256")?.into()),
                        event.clone(),
                    )
                    .is_some()
            {
                return Err(VerifyError::Source);
            }
        }
        if row["kind"] == "canonical_source" {
            let r = &event["receipt"];
            if r["reason"] == "ProvisionalRevision"
                && self
                    .quarantined
                    .range(integer(r, "label_ms")?..integer(r, "close_ms")?)
                    .next()
                    .is_none()
            {
                return Err(VerifyError::Source);
            }
            if let Some(hash) = event["receipt"]["rest_snapshot_sha256"].as_str() {
                self.canonical_rest
                    .insert(string(event, "source_sha256")?.into(), hash.into());
            }
        }
        if row["kind"] == "source_snapshot" {
            self.seen_sources
                .insert(string(event, "snapshot_sha256")?.into());
        }
        if event["event"] == "primary_reference_installed" {
            let role = string(event, "role")?;
            let key = installation(event, role)?;
            if !matches!(role, "Bootstrap" | "StartupAnchor")
                || self.current_generation != Some(key.0)
                || event["config_sha256"] != scan.header.config_sha256
                || event["history_proves_data_readiness"] != false
                || !self.seen_sources.contains(&key.3)
                || self.installations.contains(&key)
            {
                return Err(VerifyError::Source);
            }
            let source = scan.sources.get(&key.3).ok_or(VerifyError::Source)?;
            let e = source.evidence();
            let at = integer(event, "installed_at_ms")?;
            if e.parts.len() != 1
                || integer(event, "start_ms")? != e.start.timestamp_millis()
                || integer(event, "end_ms")? != e.end.timestamp_millis()
                || at < e.parts[0].received_at.timestamp_millis()
                || at > now
            {
                return Err(VerifyError::Source);
            }
            if role == "Bootstrap" {
                let request = self
                    .bootstrap_requests
                    .get(&(key.0, key.2.clone()))
                    .ok_or(VerifyError::Source)?;
                if parse_time(request, "start")? != e.start.timestamp_millis()
                    || parse_time(request, "end")? != e.end.timestamp_millis()
                    || e.parts[0].requested_at.timestamp_millis()
                        < integer(request, "original_received_at_ms")?
                    || e.parts[0].received_at.timestamp_millis() > integer(request, "deadline_ms")?
                {
                    return Err(VerifyError::Source);
                }
            } else {
                self.pending_anchors.insert(key.clone());
            }
            self.bind_reference(source)?;
            self.installations.insert(key);
        }
        let role = if event["event"] == "primary_bootstrap_proof_applied" {
            Some("Bootstrap")
        } else if event["event"] == "primary_recovery_source_validated"
            && event["scope"] == "STARTUP_ANCHOR"
        {
            Some("StartupAnchor")
        } else {
            None
        };
        if let Some(role) = role {
            let key = installation(event, role)?;
            if !self.installations.contains(&key)
                || (role == "StartupAnchor" && !self.pending_anchors.remove(&key))
            {
                return Err(VerifyError::Source);
            }
        }
        if event["event"] != "primary_source_change" {
            return Ok(());
        }
        let c = &event["change"];
        let context = c["context"].as_str().ok_or(VerifyError::Source)?;
        if !matches!(
            context,
            "REST_PROOF"
                | "RECOVERY_PENDING"
                | "WS_SUPERSEDED_REPEAT_DISCARDED"
                | "RECOVERY_OVERLAP_REVISION"
                | "REST_REFERENCE_VERSIONED"
                | "WS_PROVISIONAL_QUARANTINED"
        ) {
            return Ok(());
        }
        let a: Bar =
            serde_json::from_value(c["expected"].clone()).map_err(|_| VerifyError::Source)?;
        let b: Bar =
            serde_json::from_value(c["observed"].clone()).map_err(|_| VerifyError::Source)?;
        let minute = integer(c, "minute_ms")?;
        if c["model_input_rewritten"] != false
            || !same_slot(&a, &b)
            || minute != a.open_ts.timestamp_millis()
        {
            return Err(VerifyError::Source);
        }
        for bar in [&a, &b] {
            super::super::wire::check_bar(bar, integer(event, "recorded_at_ms")?)
                .map_err(|_| VerifyError::Source)?;
        }
        if context == "WS_PROVISIONAL_QUARANTINED" {
            if scan.inputs[..committed]
                .iter()
                .any(|i| i.actual_m1.iter().any(|r| r.open_ts == a.open_ts))
                || !(a.open != b.open
                    || b.high < a.high
                    || b.low > a.low
                    || (b.volume < a.volume && !crate::primary::revisions::same_prices(&a, &b)))
            {
                return Err(VerifyError::Source);
            }
            self.quarantined.insert(minute);
            return Ok(());
        }
        if context == "WS_SUPERSEDED_REPEAT_DISCARDED" || context == "RECOVERY_OVERLAP_REVISION" {
            let input = scan.inputs[..committed]
                .iter()
                .find(|i| i.actual_m1.iter().any(|r| same_values(r, &a)))
                .ok_or(VerifyError::Source)?;
            if c["original_canonical_source_sha256"] != input.source_sha256 {
                return Err(VerifyError::Source);
            }
            if context == "WS_SUPERSEDED_REPEAT_DISCARDED" {
                if !matches!(
                    input.origin.as_str(),
                    "REST_EXCEPTION" | "RECOVERY_BACKFILL"
                ) || !self.superseded.iter().any(|(old, rest, hash, w)| {
                    same_values(old, &b)
                        && same_values(rest, &a)
                        && input.actual_m1.iter().any(|r| same_values(r, rest))
                        && c["superseded_ws_version"] == *w
                        && self.canonical_rest.get(&input.source_sha256) == Some(hash)
                        && scan
                            .sources
                            .get(hash)
                            .is_some_and(|s| s.bars().any(|r| same_values(r, &a)))
                }) {
                    return Err(VerifyError::Source);
                }
                return Ok(());
            }
        }
        let hash = string(c, "observed_source_sha256")?;
        let source = scan.sources.get(hash).ok_or(VerifyError::Source)?;
        if !source.bars().any(|r| same_values(r, &b)) {
            return Err(VerifyError::Source);
        }
        if context == "RECOVERY_OVERLAP_REVISION" {
            if !self.seen_sources.contains(hash) {
                return Err(VerifyError::Source);
            }
            self.overlaps.insert((hash.into(), minute));
        } else if context == "REST_REFERENCE_VERSIONED" {
            if !self.seen_sources.contains(hash)
                || !self
                    .reference
                    .get(&minute)
                    .is_some_and(|r| same_values(r, &a))
            {
                return Err(VerifyError::Source);
            }
        } else {
            let witness = &c["expected_ws_version"];
            let generation = number(witness, "generation")?;
            let first = integer(witness, "first_received_at_ms")?;
            let last = integer(witness, "last_changed_at_ms")?;
            if generation > number(event, "generation")?
                || generation == 0
                || first > last
                || last > integer(event, "recorded_at_ms")?
                || string(witness, "frame_sha256")?.len() != 64
                || !string(witness, "frame_sha256")?
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit())
            {
                return Err(VerifyError::Source);
            }
            self.superseded.push((a, b, hash.into(), witness.clone()));
        }
        Ok(())
    }

    fn bind_reference(
        &mut self,
        source: &broker_finam::sparse_m10::AdmittedClosedM1Snapshot,
    ) -> Result<(), VerifyError> {
        let start = source.evidence().start.timestamp_millis();
        let end = source.evidence().end.timestamp_millis();
        let incoming: BTreeMap<_, _> = source
            .bars()
            .map(|b| (b.open_ts.timestamp_millis(), b.clone()))
            .collect();
        if self
            .reference
            .range(start..end)
            .any(|(k, _)| !incoming.contains_key(k))
            || incoming.keys().any(|k| {
                self.reference_ranges.iter().any(|(a, b)| a <= k && k < b)
                    && !self.reference.contains_key(k)
            })
        {
            return Err(VerifyError::Source);
        }
        for (key, bar) in incoming {
            if self
                .reference
                .get(&key)
                .is_some_and(|old| !same_slot(old, &bar))
            {
                return Err(VerifyError::Source);
            }
            self.reference.insert(key, bar);
        }
        if !self.reference_ranges.contains(&(start, end)) {
            self.reference_ranges.push((start, end));
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), VerifyError> {
        if self.pending_anchors.is_empty() {
            Ok(())
        } else {
            Err(VerifyError::Source)
        }
    }
}

fn installation(event: &Value, role: &str) -> Result<Installation, VerifyError> {
    let plan = string(event, "plan_sha256")?;
    let source = string(event, "source_sha256")?;
    if [plan, source]
        .iter()
        .any(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(VerifyError::Source);
    }
    Ok((
        number(event, "generation")?,
        role.into(),
        plan.into(),
        source.into(),
    ))
}
fn parse_time(event: &Value, key: &str) -> Result<i64, VerifyError> {
    chrono::DateTime::parse_from_rfc3339(string(event, key)?)
        .map(|t| t.timestamp_millis())
        .map_err(|_| VerifyError::Source)
}
