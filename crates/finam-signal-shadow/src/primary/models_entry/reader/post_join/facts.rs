//! Final facts, not summary booleans or equality of two producer copies.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
enum RecoveryStage {
    Fault,
    Backoff,
    Auth,
    Connect,
    Subscribe,
    Bootstrap,
    FirstData,
    Resync,
    TransportReady,
    Ready,
    Failed,
    Cancelled,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalFailure {
    #[serde(rename = "generation")]
    _generation: u64,
    #[serde(rename = "kind")]
    _kind: Value,
    #[serde(rename = "liveness")]
    _liveness: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    #[serde(rename = "episode")]
    _episode: u8,
    #[serde(rename = "attempt")]
    _attempt: u8,
    #[serde(rename = "generation")]
    _generation: u64,
    #[serde(rename = "stage")]
    _stage: RecoveryStage,
    #[serde(rename = "original")]
    _original: OriginalFailure,
    #[serde(rename = "fault_observed_at_ms")]
    _fault_observed_at_ms: i64,
    #[serde(rename = "deadline_ms")]
    _deadline_ms: i64,
    transport_unresolved: bool,
    canonical_unresolved: bool,
    #[serde(rename = "last_error")]
    _last_error: Value,
    terminal: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Counts {
    scheduled: u64,
    completed: u64,
    cancelled_or_unaccounted: u64,
    known_get_send_attempts: u64,
    #[serde(rename = "complete_response_body_bytes")]
    _complete_response_body_bytes: u64,
    #[serde(rename = "completed_command_elapsed_ms")]
    _completed_command_elapsed_ms: u64,
}
const ROLES: [(&str, &str); 5] = [
    ("session_end", "SessionEnd"),
    ("recovery", "Recovery"),
    ("bootstrap", "Bootstrap"),
    ("proof", "Proof"),
    ("audit", "Audit"),
];
fn incomplete(event: &Value) -> Result<bool> {
    ensure!(event["receipt"].is_object(), "final HTTP receipt shape");
    // Runtime omits executor_completed when true; false is explicit.
    match event["receipt"].get("executor_completed") {
        None => Ok(false),
        Some(v) => Ok(!v.as_bool().context("final HTTP executor flag")?),
    }
}

/// Returns false (not a verification error) for correctly accounted but
/// genuinely unfinished HTTP work. Such a run can remain PARTIAL, never PASS.
pub(super) fn validate(
    config: &WsPrimaryConfig,
    rows: &[Value],
    shutdown: &Value,
    done: &crate::primary::eod::Completion,
) -> Result<bool> {
    validate_policy(&config.primary, rows, shutdown, done)
}
pub(crate) fn validate_policy(
    policy: &crate::primary::PrimaryPolicy,
    rows: &[Value],
    shutdown: &Value,
    done: &crate::primary::eod::Completion,
) -> Result<bool> {
    let recovery: Vec<Recovery> = serde_json::from_value(shutdown["recovery"].clone())
        .context("final recovery snapshot schema")?;
    let resolved = recovery
        .iter()
        .all(|r| !r.transport_unresolved && !r.canonical_unresolved && r.terminal.is_null());
    ensure!(
        done.recovery_resolved == resolved,
        "final recovery facts contradict completion"
    );
    if policy.data_wait_recovery() {
        let gap = shutdown["unresolved_canonical_gap"]
            .as_bool()
            .context("final V8 gap flag")?;
        let lost = shutdown["continuity_lost"]
            .as_bool()
            .context("final continuity flag")?;
        ensure!(
            gap == lost && gap != done.continuity_resolved,
            "final gap facts contradict completion"
        );
    }
    let totals: BTreeMap<String, Counts> =
        serde_json::from_value(shutdown["http"].clone()).context("final HTTP accounting schema")?;
    ensure!(
        totals.len() == ROLES.len() && ROLES.iter().all(|(r, _)| totals.contains_key(*r)),
        "final HTTP accounting roles"
    );
    let drained = shutdown["drained_http"]
        .as_array()
        .context("final drained HTTP inventory")?;
    let completed: Vec<_> = rows
        .iter()
        .filter(|r| r["event"]["event"] == "primary_http_completed")
        .map(|r| &r["event"])
        .chain(drained.iter())
        .collect();
    let mut unknown = BTreeMap::<&str, u64>::new();
    for event in completed {
        let role = event["role"].as_str().context("final HTTP role")?;
        ensure!(
            ROLES.iter().any(|(_, r)| *r == role),
            "unknown final HTTP role"
        );
        if incomplete(event)? {
            *unknown.entry(role).or_default() += 1;
        }
    }
    let mut accounted = true;
    for (role, wire_role) in ROLES {
        let c = &totals[role];
        ensure!(
            c.completed.checked_add(c.cancelled_or_unaccounted) == Some(c.scheduled),
            "final HTTP accounting arithmetic"
        );
        let optional = policy.rest_nonblocking() && matches!(role, "proof" | "audit");
        let expected = if optional {
            unknown.get(wire_role).copied().unwrap_or(0)
        } else {
            0
        };
        accounted &=
            c.cancelled_or_unaccounted == expected && c.known_get_send_attempts <= c.completed;
    }
    // Same accepted drain outcome policy as the strict native EOD reader.
    // No requirement that optional comparisons succeeded; no session-end
    // GET/full-grid/flat requirement is imported into bounded completion.
    for event in drained {
        let granted = event["receipt"]["auth_generation"]
            .as_u64()
            .is_some_and(|g| g > 0);
        let optional =
            policy.rest_nonblocking() && matches!(event["role"].as_str(), Some("Proof" | "Audit"));
        let class = event["class"].as_str();
        accounted &= (optional
            && (granted || matches!(class, Some("Timeout" | "Unavailable" | "Cancelled"))))
            || matches!(class, Some("Timeout" | "Unavailable"))
            || event["class"].is_null()
            || (class == Some("Cancelled") && granted);
    }
    Ok(accounted)
}

#[cfg(test)]
mod tests;
