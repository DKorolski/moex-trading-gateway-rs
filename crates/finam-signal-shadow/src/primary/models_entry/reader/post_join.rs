//! Common bounded/EOD post-join validation. No flat/full-grid condition here.
use super::*;
use crate::primary::calendar::{self, CalendarBinding, CalendarInputs};
pub(crate) mod facts;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub completion: crate::primary::eod::Completion,
    pub recovery: Value,
    pub unresolved_transport_fault: Value,
    pub cause: Value,
    pub canonical_commits: u64,
    pub http: Value,
    pub socket_drained: bool,
    pub http_drained: bool,
    pub writer_drained: bool,
    pub all_workers_drained: bool,
    pub session_finalized: bool,
    pub socket: Value,
    pub full_session_qualified: bool,
}

fn unique<'a>(rows: &'a [Value], name: &str) -> Result<&'a Value> {
    let mut found = rows.iter().filter(|r| r["event"]["event"] == name);
    let row = found.next().context("missing post-join control event")?;
    ensure!(found.next().is_none(), "ambiguous post-join control event");
    Ok(&row["event"])
}
fn stamp(v: &Value, name: &str) -> Result<i64> {
    v[name].as_i64().context("post-join control clock")
}
pub(super) fn validate(
    config: &WsPrimaryConfig,
    rows: &[Value],
    value: &Value,
    now: i64,
) -> Result<(Report, bool)> {
    ensure!(
        value.as_object().is_some_and(|o| o.len() == 1),
        "post-join result schema"
    );
    let r: Report =
        serde_json::from_value(value.get("Ok").context("post-join report missing")?.clone())?;
    let d = &r.completion;
    let event = unique(rows, "primary_calendar_bound")?;
    let ev = &event["evidence"];
    let binding: CalendarBinding = serde_json::from_value(ev["binding"].clone())?;
    let raw =
        |key: &str| -> Result<&str> { ev[key].as_str().context("post-join calendar raw input") };
    let cal = calendar::admit(
        config,
        binding,
        CalendarInputs {
            model_day: raw("model_day_raw_json")?.as_bytes(),
            raw_calendar: raw("raw_calendar_raw_json")?.as_bytes(),
            mapping: raw("mapping_raw_json")?.as_bytes(),
        },
        stamp(ev, "admitted_at_ms")?,
    )
    .map_err(|_| anyhow::anyhow!("post-join calendar admission"))?;
    let bound_at = stamp(event, "bound_at_ms")?;
    cal.validate_entry(bound_at)
        .map_err(|_| anyhow::anyhow!("post-join calendar entry"))?;
    ensure!(
        d.schema_version == 1
            && d.config_sha256
                == config
                    .fingerprint()
                    .map_err(|_| anyhow::anyhow!("config"))?
            && d.calendar_sha256 == cal.sha256()
            && ev["calendar_sha256"] == cal.sha256()
            && ev["reference_start_ms"] == cal.reference_start_ms()
            && d.eod_check_at_ms == cal.eod_check_at_ms()
            && bound_at <= now
            && !d.network_qualified
            && !d.cgroup_qualified
            && !r.full_session_qualified,
        "post-join config/calendar/schema identity"
    );
    ensure!(
        d.stop_budget_ms == config.base.stop_timeout_ms.min(15000)
            && (1..=15000).contains(&d.stop_budget_ms)
            && d.elapsed_stop_ms <= d.stop_budget_ms
            && d.stop_started_at_ms >= bound_at
            && d.joined_at_ms >= d.stop_started_at_ms
            && d.joined_at_ms <= now,
        "post-join STOP/join budget or chronology"
    );
    // The monotonic budget begins at calendar EOD for EOD, at root STOP
    // otherwise. Compare UTC to that same origin, allowing only accepted U.
    let origin = if d.calendar_eod_trigger {
        d.eod_check_at_ms
    } else {
        d.stop_started_at_ms
    };
    let elapsed = i128::from(d.joined_at_ms) - i128::from(origin);
    ensure!(
        elapsed >= 0
            && elapsed
                <= i128::from(d.stop_budget_ms) + i128::from(config.base.clock_uncertainty_ms)
            && (elapsed - i128::from(d.elapsed_stop_ms)).unsigned_abs()
                <= u128::from(config.base.clock_uncertainty_ms)
            && (!d.calendar_eod_trigger || d.stop_started_at_ms >= d.eod_check_at_ms),
        "post-join UTC/elapsed inconsistency"
    );
    let shutdown = unique(rows, "primary_async_shutdown")?;
    ensure!(
        shutdown["cause"] == r.cause
            && stamp(shutdown, "stop_started_at_ms")? == d.stop_started_at_ms
            && shutdown["requested_calendar_eod"] == d.calendar_eod_trigger
            && shutdown["socket_drained"] == r.socket_drained
            && shutdown["http_drained"] == r.http_drained
            && shutdown["continuity_lost"] == !d.continuity_resolved
            && shutdown["unresolved_transport_fault"] == r.unresolved_transport_fault
            && shutdown["recovery"] == r.recovery
            && shutdown["http"] == r.http
            && shutdown["full_session_qualified"] == false
            && r.socket_drained == d.socket_drained
            && r.http_drained == d.http_drained
            && r.writer_drained == d.writer_drained
            && r.all_workers_drained == (r.socket_drained && r.http_drained && r.writer_drained)
            && d.semantic_eod_passed == d.qualifies(),
        "post-join/control shutdown linkage"
    );
    // Socket report is diagnostic, never an independent join witness.
    let _ = &r.socket;
    let accounted = facts::validate(config, rows, shutdown, d)?;
    Ok((r, accounted))
}
impl Report {
    pub(super) fn drained(&self, source: u64) -> bool {
        let d = &self.completion;
        self.all_workers_drained
            && self.session_finalized
            && d.clock_consistent
            && d.fault_free
            && d.continuity_resolved
            && d.recovery_resolved
            && self.canonical_commits == source
            && self.unresolved_transport_fault.is_null()
    }
}
