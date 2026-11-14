//! Independently check V6 request identity, inventory and publication lineage
//! for BOTH restart-prefix and full-EOD readers. No producer PASS is consumed.
use super::*;
use crate::primary::{RecoveryRequestScope, MAX_REST_READS, REST_BACKOFF_MS};
use broker_core::event::Bar;
use broker_finam::sparse_m10::AdmittedClosedM1Snapshot;
#[cfg(test)]
mod failed_tests;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Active,
    Pending,
    Retired,
    Ready,
    Invalidated,
}
struct Request {
    value: Value,
    scope: RecoveryRequestScope,
    state: State,
    source: Option<String>,
    known: Vec<i64>,
    pending_at: Option<i64>,
}

fn require(ok: bool) -> Result<(), VerifyError> {
    if ok {
        Ok(())
    } else {
        Err(VerifyError::Source)
    }
}
fn ints(v: &Value) -> Result<Vec<i64>, VerifyError> {
    v.as_array()
        .ok_or(VerifyError::Source)?
        .iter()
        .map(|t| t.as_i64().ok_or(VerifyError::Source))
        .collect()
}

pub(crate) fn validate(
    config: &WsPrimaryConfig,
    bytes: &[u8],
    required: &[i64],
    sources: &BTreeMap<String, AdmittedClosedM1Snapshot>,
    now: i64,
) -> Result<(), VerifyError> {
    let enabled = config.primary.rest_tail_recovery();
    let mut requests = BTreeMap::<String, Request>::new();
    let mut episodes = BTreeMap::<u8, (i64, i64, u8)>::new(); // fault, deadline, reads
    let mut active_episode = None;
    let mut active = None::<String>;
    let mut generation = None;
    let mut seen = BTreeSet::new();
    let mut committed = BTreeMap::<i64, Vec<Bar>>::new();
    let mut pending = None::<Vec<Bar>>;
    let mut last_serial = BTreeMap::<String, u64>::new();
    let mut calendar = None::<String>;
    let mut eod = config.base.run_deadline_utc_ms;
    let mut recovery_http_receipt = None;
    for line in std::str::from_utf8(bytes)
        .map_err(|_| VerifyError::Decode)?
        .lines()
    {
        let row = super::super::json::parse(line).map_err(|_| VerifyError::Decode)?;
        let e = &row["event"];
        let name = e["event"].as_str().unwrap_or("");
        if !enabled {
            require(
                !name.starts_with("primary_recovery_request_")
                    && name != "primary_recovery_data_not_ready"
                    && name != "primary_source_readiness",
            )?;
            continue;
        }
        if row["kind"] == "source_snapshot" {
            seen.insert(string(e, "snapshot_sha256")?.to_owned());
        }
        match name {
            "primary_http_completed" if e["role"] == "Recovery" => {
                recovery_http_receipt = Some(e["receipt"].clone());
            }
            "primary_calendar_bound" => {
                calendar = Some(string(&e["evidence"], "calendar_sha256")?.into());
                eod = integer(&e["evidence"]["binding"], "eod_check_at_ms")?;
            }
            "primary_subscription_sent" => {
                generation = Some(number(e, "generation")?);
            }
            "primary_recovery_episode_started" => {
                let a = &e["audit"];
                let id = number(a, "episode")?;
                let fault = integer(a, "fault_observed_at_ms")?;
                let deadline = integer(a, "deadline_ms")?;
                require(
                    (1..=3).contains(&id)
                        && fault >= 0
                        && fault <= now
                        && fault < deadline
                        && deadline <= fault.checked_add(45000).ok_or(VerifyError::Source)?
                        && deadline <= eod
                        && deadline <= config.base.run_deadline_utc_ms,
                )?;
                require(episodes.insert(id as u8, (fault, deadline, 0)).is_none())?;
                active_episode = Some(id as u8);
            }
            "primary_recovery_attempt_prepared" => {
                if let Some(hash) = active.take() {
                    let r = requests.get_mut(&hash).ok_or(VerifyError::Source)?;
                    if r.state != State::Ready {
                        r.state = State::Invalidated;
                    }
                }
            }
            "primary_recovery_request_retired" => {
                let hash = string(e, "ticket_sha256")?;
                require(active.as_deref() == Some(hash))?;
                let r = requests.get_mut(hash).ok_or(VerifyError::Source)?;
                require(r.state == State::Pending && e["reason"] == "Renewal")?;
                let at = integer(e, "at_ms")?;
                let backoff = *REST_BACKOFF_MS
                    .get(usize::from(r.scope.ordinal - 1))
                    .ok_or(VerifyError::Source)?;
                require(
                    at >= r
                        .pending_at
                        .ok_or(VerifyError::Source)?
                        .checked_add(backoff as i64)
                        .ok_or(VerifyError::Source)?,
                )?;
                r.state = State::Retired;
                // The replacement is checked against the next issued ticket.
                r.source = Some(string(e, "replacement_sha256")?.into());
            }
            "primary_recovery_request_issued" => {
                let hash = string(e, "ticket_sha256")?.to_owned();
                let scope: RecoveryRequestScope =
                    serde_json::from_value(e["scope"].clone()).map_err(|_| VerifyError::Source)?;
                let epoch = episodes
                    .get_mut(&scope.episode)
                    .ok_or(VerifyError::Source)?;
                let gen = number(e, "generation")?;
                let issued = integer(e, "issued_at_ms")?;
                let deadline = integer(e, "deadline_ms")?;
                let start = integer(e, "start_ms")?;
                let cutover = integer(e, "cutover_ms")?;
                let frontier = number(e, "frontier_index")? as usize;
                let serial = number(e, "serial")?;
                require(
                    active_episode == Some(scope.episode)
                        && Some(gen) == generation
                        && scope.ordinal == epoch.2 + 1
                        && scope.ordinal <= MAX_REST_READS
                        && scope.deadline_ms == epoch.1
                        && issued >= epoch.0
                        && issued < scope.deadline_ms
                        && issued >= 0
                        && deadline
                            == scope
                                .deadline_ms
                                .min(issued.checked_add(5000).ok_or(VerifyError::Source)?)
                        && issued <= now
                        && e["maximum_reads"] == MAX_REST_READS
                        && frontier == committed.len()
                        && cutover == issued.div_euclid(600000) * 600000
                        && cutover % 600000 == 0
                        && start < cutover,
                )?;
                let startup = required.first().copied() == Some(cutover) && frontier == 0;
                require(
                    e["startup_anchor"] == startup
                        && e["calendar_sha256"].as_str() == calendar.as_deref(),
                )?;
                if !startup {
                    require(
                        required
                            .get(frontier.saturating_sub(config.primary.recovery_overlap_m10))
                            .copied()
                            == Some(start),
                    )?;
                }
                let cfg = string(&row, "config_sha256")?;
                require(serial == last_serial.get(cfg).copied().unwrap_or(0) + 1)?;
                let policy = config
                    .primary
                    .fingerprint()
                    .map_err(|_| VerifyError::Binding)?;
                let mut expected = serde_json::to_vec(&(
                    cfg,
                    policy,
                    gen,
                    start,
                    cutover,
                    frontier,
                    issued,
                    deadline,
                    startup,
                    calendar.as_deref(),
                ))
                .map_err(|_| VerifyError::Source)?;
                expected.extend(
                    serde_json::to_vec(&(Some(scope), serial)).map_err(|_| VerifyError::Source)?,
                );
                require(digest(&expected) == hash)?;
                if let Some(old) = active.as_ref() {
                    let r = requests.get(old).ok_or(VerifyError::Source)?;
                    require(
                        r.state == State::Retired
                            && r.source.as_deref() == Some(hash.as_str())
                            && r.scope.episode == scope.episode
                            && r.value["generation"] == e["generation"]
                            && r.value["frontier_index"] == e["frontier_index"]
                            && r.value["cutover_ms"] == e["cutover_ms"],
                    )?;
                }
                epoch.2 += 1;
                last_serial.insert(cfg.into(), serial);
                require(
                    requests
                        .insert(
                            hash.clone(),
                            Request {
                                value: e.clone(),
                                scope,
                                state: State::Active,
                                source: None,
                                known: vec![],
                                pending_at: None,
                            },
                        )
                        .is_none(),
                )?;
                active = Some(hash);
            }
            "primary_recovery_request_failed" => {
                require(config.primary.data_wait_recovery())?;
                let hash = string(e, "ticket_sha256")?;
                require(active.as_deref() == Some(hash))?;
                let r = requests.get_mut(hash).ok_or(VerifyError::Source)?;
                let receipt = &e["receipt"];
                require(recovery_http_receipt.take().as_ref() == Some(receipt))?;
                let at = integer(e, "at_ms")?;
                let started = integer(receipt, "started_at_ms")?;
                let finished = integer(receipt, "finished_at_ms")?;
                require(
                    r.state == State::Active
                        && e["scope"] == r.value["scope"]
                        && Some(number(e, "generation")?) == generation
                        && matches!(e["error"].as_str(), Some("Unavailable" | "Timeout"))
                        && e["callback"] == false
                        && e["validated_source"] == false
                        && (receipt["executor_completed"].is_null()
                            || receipt["executor_completed"] == true)
                        && receipt["deadline_ms"] == r.value["deadline_ms"]
                        && started >= integer(&r.value, "issued_at_ms")?
                        && finished >= started
                        && finished <= at
                        && at <= now
                        && number(receipt, "get_attempts")? <= 1
                        && (receipt["get_attempts"] == 0
                            || receipt["auth_generation"].as_u64().is_some()),
                )?;
                r.state = State::Pending;
                r.pending_at = Some(at);
            }
            "primary_recovery_data_not_ready" => {
                let hash = string(e, "ticket_sha256")?;
                require(active.as_deref() == Some(hash))?;
                let r = requests.get_mut(hash).ok_or(VerifyError::Source)?;
                require(
                    r.state == State::Active
                        && e["scope"] == r.value["scope"]
                        && Some(number(e, "generation")?) == generation
                        && e["callback"] == false
                        && e["model_input_rewritten"] == false,
                )?;
                let source_hash = string(e, "source_sha256")?;
                require(seen.contains(source_hash))?;
                let source = sources.get(source_hash).ok_or(VerifyError::Source)?;
                let a = &e["assessment"];
                let assessed = integer(a, "assessed_at_ms")?;
                check_source(r, source, &committed, assessed, now, false)?;
                match string(a, "reason")? {
                    "MissingRecentUnpublishedTail" => {
                        require(
                            assessed <= integer(&r.value, "deadline_ms")?
                                && source.evidence().parts[0].received_at.timestamp_millis()
                                    <= integer(&r.value, "deadline_ms")?,
                        )?;
                        let label = integer(a, "label_ms")?;
                        let cutover = integer(&r.value, "cutover_ms")?;
                        let known = ints(&a["known_m1_ms"])?;
                        let missing = ints(&a["missing_known_m1_ms"])?;
                        let frontier = required
                            .get(committed.len())
                            .copied()
                            .ok_or(VerifyError::Source)?;
                        require(
                            label + 600000 == cutover
                                && label >= frontier
                                && known.windows(2).all(|p| p[0] < p[1])
                                && known
                                    .iter()
                                    .all(|t| *t >= frontier && *t < cutover && *t % 60000 == 0),
                        )?;
                        let actual: BTreeSet<_> = source
                            .bars()
                            .map(|b| b.open_ts.timestamp_millis())
                            .collect();
                        let computed: Vec<_> = known
                            .iter()
                            .filter(|t| !actual.contains(t))
                            .copied()
                            .collect();
                        let empty = actual.range(label..cutover).next().is_none();
                        require(
                            computed == missing
                                && a["empty_last_bucket"] == empty
                                && (empty || !missing.is_empty())
                                && missing.iter().all(|t| *t >= label)
                                && missing
                                    .first()
                                    .map_or(true, |t| actual.range(*t..cutover).next().is_none()),
                        )?;
                        for old_label in required
                            .iter()
                            .take_while(|t| **t < label)
                            .skip(committed.len())
                        {
                            require(
                                actual
                                    .range(*old_label..*old_label + 600000)
                                    .next()
                                    .is_some(),
                            )?;
                        }
                        r.known = known;
                    }
                    "RequestExpired" => {
                        require(
                            assessed > integer(&r.value, "deadline_ms")?
                                || source.evidence().parts[0].received_at.timestamp_millis()
                                    > integer(&r.value, "deadline_ms")?,
                        )?;
                    }
                    _ => return Err(VerifyError::Source),
                }
                r.state = State::Pending;
                r.pending_at = Some(assessed);
            }
            "primary_recovery_request_stale_discarded" => {
                let r = requests
                    .get(string(e, "ticket_sha256")?)
                    .ok_or(VerifyError::Source)?;
                require(r.state != State::Active && e["callback"] == false)?;
            }
            "primary_recovery_source_validated" => {
                let hash = string(e, "plan_sha256")?;
                require(active.as_deref() == Some(hash))?;
                let source_hash = string(e, "source_sha256")?;
                require(seen.contains(source_hash))?;
                let source = sources.get(source_hash).ok_or(VerifyError::Source)?;
                let r = requests.get(hash).ok_or(VerifyError::Source)?;
                require(
                    r.state == State::Active
                        && Some(number(e, "generation")?) == generation
                        && e["issued_at_ms"] == r.value["issued_at_ms"]
                        && e["deadline_ms"] == r.value["deadline_ms"],
                )?;
                check_source(
                    r,
                    source,
                    &committed,
                    integer(e, "validated_at_ms")?,
                    now,
                    true,
                )?;
                let actual: BTreeSet<_> = source
                    .bars()
                    .map(|b| b.open_ts.timestamp_millis())
                    .collect();
                for prior in requests
                    .values()
                    .filter(|v| v.scope.episode == r.scope.episode)
                {
                    require(prior.known.iter().all(|t| actual.contains(t)))?;
                }
                let r = requests.get_mut(hash).ok_or(VerifyError::Source)?;
                r.state = State::Ready;
                r.source = Some(source_hash.into());
            }
            _ => (),
        }
        if row["kind"] == "canonical_source" || row["kind"] == "restart_source" {
            let receipt = &e["receipt"];
            if receipt["origin"] == "RECOVERY_BACKFILL" {
                let r = requests
                    .get(string(receipt, "recovery_plan_sha256")?)
                    .ok_or(VerifyError::Source)?;
                require(
                    r.state == State::Ready
                        && r.source.as_deref() == receipt["rest_snapshot_sha256"].as_str()
                        && receipt["generation"] == r.value["generation"]
                        && receipt["cutover_ms"] == r.value["cutover_ms"]
                        && integer(receipt, "canonical_ready_at_ms")?
                            <= integer(&r.value, "deadline_ms")?
                        && integer(receipt, "replay_deadline_ms")?
                            == integer(receipt, "canonical_ready_at_ms")?
                                .checked_add(10000)
                                .ok_or(VerifyError::Source)?
                                .min(config.base.run_deadline_utc_ms)
                                .min(r.scope.deadline_ms),
                )?;
            }
            pending = Some(
                serde_json::from_value(receipt["actual_m1"].clone())
                    .map_err(|_| VerifyError::Source)?,
            );
        }
        if row["kind"] == "canonical_commit" {
            let label = integer(e, "model_label_utc_ms")?;
            require(
                committed
                    .insert(label, pending.take().ok_or(VerifyError::Source)?)
                    .is_none(),
            )?;
        }
    }
    Ok(())
}

fn check_source(
    r: &Request,
    s: &AdmittedClosedM1Snapshot,
    committed: &BTreeMap<i64, Vec<Bar>>,
    assessed: i64,
    now: i64,
    ready: bool,
) -> Result<(), VerifyError> {
    let ev = s.evidence();
    require(
        ev.parts.len() == 1
            && ev.start.timestamp_millis() == integer(&r.value, "start_ms")?
            && ev.end.timestamp_millis() == integer(&r.value, "cutover_ms")?
            && assessed <= now,
    )?;
    let p = &ev.parts[0];
    for label in (ev.start.timestamp_millis()..ev.end.timestamp_millis()).step_by(600000) {
        if s.bars()
            .any(|b| (label..label + 600000).contains(&b.open_ts.timestamp_millis()))
        {
            s.bucket(chrono::DateTime::from_timestamp_millis(label).ok_or(VerifyError::Source)?)
                .map_err(|_| VerifyError::Source)?;
        }
    }
    require(
        p.requested_at.timestamp_millis() >= integer(&r.value, "issued_at_ms")?
            && p.received_at.timestamp_millis() <= assessed,
    )?;
    if ready {
        require(assessed <= integer(&r.value, "deadline_ms")?)?;
    }
    for (label, known) in committed.range(ev.start.timestamp_millis()..ev.end.timestamp_millis()) {
        let actual: Vec<_> = s
            .bars()
            .filter(|b| (*label..*label + 600000).contains(&b.open_ts.timestamp_millis()))
            .collect();
        require(
            known.len() == actual.len()
                && known
                    .iter()
                    .zip(actual)
                    .all(|(a, b)| crate::primary::revisions::same_slot(a, b)),
        )?;
    }
    Ok(())
}
