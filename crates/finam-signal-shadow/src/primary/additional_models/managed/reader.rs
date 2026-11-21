//! Bounded file-only readback. JSON is evidence, NEVER a native capability.
use super::super::plan::hash;
use super::*;
use crate::primary::models_entry::reader::post_join::{facts, Report};
use broker_finam::sparse_m10::{ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1};
use finam_shadow_models::local::contract::{Candle, Construction, Manifest};
use std::collections::BTreeSet;

fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink() && m.len() <= max,
        "bounded regular evidence file"
    );
    Ok(std::fs::read(path)?)
}
fn stamp(v: &Value, k: &str) -> Result<i64> {
    v[k].as_i64().context("required evidence clock")
}
fn unique<'a>(rows: &'a [Value], name: &str) -> Result<&'a Value> {
    let mut found = rows.iter().filter(|v| v["event"]["event"] == name);
    let r = &found.next().context("missing managed lane event")?["event"];
    ensure!(found.next().is_none(), "duplicate managed lane event");
    Ok(r)
}
struct Checked {
    candle: Candle,
    at_ms: i64,
}
/// File-only accounting, never a native source or replay capability. SQL
/// readback and actual source advance are deliberately DIFFERENT frontiers.
#[derive(Default)]
pub(super) struct SourceProgress {
    committed: u64,
    pending: Option<Checked>,
    last_advance_ms: Option<i64>,
    closed: bool,
}
impl SourceProgress {
    fn before_intent(&self) -> Result<()> {
        ensure!(!self.closed, "source advance input after final frontier");
        ensure!(
            self.pending.is_none(),
            "source advance missing before next input"
        );
        Ok(())
    }
    pub(super) fn readback(&mut self, candle: Candle, at_ms: i64) -> Result<()> {
        self.before_intent()?;
        self.pending = Some(Checked { candle, at_ms });
        Ok(())
    }
    pub(super) fn advance(
        &mut self,
        event: &Value,
        recorded_at_ms: i64,
        stop_at_ms: i64,
        required: &[i64],
    ) -> Result<()> {
        ensure!(!self.closed, "source advance after final frontier");
        let checked = self
            .pending
            .as_ref()
            .context("source advance without checked readback")?;
        let c = &checked.candle;
        let Construction::DeclaredCanonicalProjectionV1 { provenance: p, .. } = &c.construction
        else {
            anyhow::bail!("source advance without native projection")
        };
        let index = usize::try_from(self.committed).context("source advance index")?;
        let next = serde_json::to_value(required.get(index + 1).copied())?;
        ensure!(
            event["event"] == "primary_models_source_advanced"
                && required.get(index) == Some(&c.start_ms)
                && event["label_ms"] == c.start_ms
                && event["source_sha256"] == p["source_receipt_sha256"]
                && event["source_committed"] == self.committed + 1
                && event.get("next_label_ms") == Some(&next),
            "source advance identity/frontier mismatch"
        );
        let at = stamp(event, "source_advanced_at_ms").context("source advance actual clock")?;
        ensure!(
            at >= checked.at_ms
                && at >= c.durable_ms
                && at <= recorded_at_ms
                && self.last_advance_ms.map_or(true, |previous| at >= previous),
            "source advance readback/record clock"
        );
        ensure!(at < stop_at_ms, "source advance bounded deadline");
        match p["canonical_receipt"]["origin"].as_str() {
            Some("WS_LIVE" | "REST_EXCEPTION") => {
                ensure!(
                    i128::from(at) - i128::from(c.end_ms) + 100 <= 2000,
                    "source advance LIVE deadline"
                );
            }
            Some("RECOVERY_BACKFILL" | "WS_RECOVERY_BACKFILL") => {
                let replay = &p["recovery_receipt"];
                ensure!(
                    at <= stamp(replay, "replay_deadline_ms")?
                        && i128::from(at) - i128::from(c.durable_ms) <= 10000,
                    "source advance historical deadline"
                );
                if p["canonical_receipt"]["origin"] == "WS_RECOVERY_BACKFILL" {
                    ensure!(
                        at < stamp(replay, "episode_deadline_ms")?,
                        "source advance historical episode deadline"
                    );
                }
            }
            _ => anyhow::bail!("source advance unknown origin"),
        }
        self.pending = None;
        self.committed += 1;
        self.last_advance_ms = Some(at);
        Ok(())
    }
    fn final_frontier(
        &mut self,
        event: &Value,
        recorded_at_ms: i64,
        required: &[i64],
    ) -> Result<()> {
        let next = serde_json::to_value(required.get(self.committed as usize).copied())?;
        let at = stamp(event, "at_ms").context("source advance final clock")?;
        ensure!(
            !self.closed
                && event["source_committed"] == self.committed
                && event.get("next_label_ms") == Some(&next)
                && at <= recorded_at_ms
                && self.last_advance_ms.map_or(true, |previous| at >= previous),
            "source advance final frontier mismatch"
        );
        self.closed = true;
        Ok(())
    }
}
fn snapshot(
    i: ShadowInstrument,
    e: ClosedM1SnapshotEvidenceV1,
    at: i64,
    expected: &Value,
) -> Result<AdmittedClosedM1Snapshot> {
    let p = ClosedM1RequestPlan::new(e.start, e.end, e.parts.iter().map(|p| p.end).collect())
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let now = chrono::DateTime::from_timestamp_millis(at).context("HTTP attachment clock")?;
    p.preflight(now)
        .map_err(|e| anyhow::anyhow!("request clock: {e:?}"))?;
    let s = AdmittedClosedM1Snapshot::admit_for(i, e, &p, now)
        .map_err(|e| anyhow::anyhow!("snapshot: {e:?}"))?;
    ensure!(expected == s.sha256(), "exact HTTP snapshot hash");
    Ok(s)
}
pub(super) fn projection(
    c: &Candle,
    i: ShadowInstrument,
    config: &str,
    generation: u64,
    sources: &BTreeMap<String, AdmittedClosedM1Snapshot>,
) -> Result<()> {
    let Construction::DeclaredCanonicalProjectionV1 {
        provenance: p,
        provenance_sha256,
    } = &c.construction
    else {
        anyhow::bail!("native projection required")
    };
    let r = &p["canonical_receipt"];
    let original = p["source_receipt_json"]
        .as_str()
        .context("original native receipt")?;
    let original_value: Value = serde_json::from_str(original)?;
    let recovery = r["origin"] == "RECOVERY_BACKFILL" || r["origin"] == "WS_RECOVERY_BACKFILL";
    ensure!(
        hash(p)? == *provenance_sha256
            && p["schema"] == "finam.shadow.usd-ri.native-projection.v1"
            && p["implementation_sha256"] == super::super::implementation()
            && p["clock_basis"] == "declared_numerical_availability_not_physical_durability"
            && p["observed_clean_session"] == false
            && p["order_emission_allowed"] == false
            && r["config_sha256"] == config
            && r["policy_sha256"]
                == policy()
                    .fingerprint()
                    .map_err(|e| anyhow::anyhow!("{e:?}"))?
            && r["generation"] == generation
            && generation > 0
            && p["source_receipt_sha256"] == crate::seed::digest(original.as_bytes())
            && if recovery {
                original_value == p["recovery_receipt"]
                    && r.as_object().is_some_and(|fields| {
                        fields
                            .iter()
                            .all(|(key, value)| original_value.get(key) == Some(value))
                    })
            } else {
                original_value == *r
            },
        "native receipt identity/original hash/projection"
    );
    ensure!(
        r["label_ms"] == c.start_ms
            && r["close_ms"] == c.end_ms
            && r["canonical_ready_at_ms"] == c.durable_ms
            && c.source_processed_ms == c.durable_ms,
        "canonical numerical clocks"
    );
    if recovery {
        ensure!(
            c.durable_ms <= stamp(&original_value, "replay_deadline_ms")?
                && original_value["virtual_callback_at_ms"] == c.end_ms,
            "historical replay clocks"
        );
    } else {
        ensure!(
            c.durable_ms >= c.end_ms && c.durable_ms - c.end_ms + 100 <= 2000,
            "observed BO latency"
        );
    }
    let bars: Vec<broker_core::event::Bar> = serde_json::from_value(r["actual_m1"].clone())?;
    match r["origin"].as_str().context("origin")? {
        "WS_LIVE" | "WS_RECOVERY_BACKFILL" => {
            ensure!(
                p["rest_evidence"].is_null()
                    && r["rest_snapshot_sha256"].is_null()
                    && r["proof_received_at_ms"].is_null()
                    && r["boundary_received_at_ms"] == c.received_ms,
                "WS receipt"
            );
        }
        "REST_EXCEPTION" | "RECOVERY_BACKFILL" => {
            ensure!(
                r["proof_received_at_ms"] == c.received_ms,
                "REST receive clock"
            );
            let owned;
            let src = if recovery {
                let h = r["rest_snapshot_sha256"].as_str().context("REST hash")?;
                let s = sources.get(h).context("retained recovery HTTP missing")?;
                ensure!(
                    p["rest_evidence"]["storage"] == "native_control_journal_source_snapshot"
                        && p["rest_evidence"]["snapshot_sha256"] == h
                        && p["rest_evidence"]["evidence_sha256"] == hash(s.evidence())?,
                    "recovery evidence pointer"
                );
                s
            } else {
                owned = snapshot(
                    i,
                    serde_json::from_value(p["rest_evidence"].clone())?,
                    c.durable_ms,
                    &r["rest_snapshot_sha256"],
                )?;
                &owned
            };
            ensure!(src.instrument() == i, "REST instrument");
            src.verify_selection(
                chrono::DateTime::from_timestamp_millis(c.start_ms).context("label")?,
                &bars,
            )
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        }
        _ => anyhow::bail!("unsupported native origin"),
    }
    let b = broker_core::canonical_m1::aggregate_m1_window(
        &i.instrument(),
        chrono::DateTime::from_timestamp_millis(c.start_ms).context("label")?,
        &bars,
    )
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let mut runtime = crate::primary::cache::runtime_bar(&b.provisional_bar)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if recovery {
        runtime.origin = broker_core::HybridRuntimeBarOrigin::Replay;
    }
    ensure!(
        r["canonical_digest"]
            == crate::input::canonical_digest(&runtime).map_err(|e| anyhow::anyhow!("{e:?}"))?,
        "canonical digest/origin"
    );
    ensure!(
        r["minute_bitmap"] == b.minute_bitmap
            && c.data.open == b.provisional_bar.open.to_string()
            && c.data.high == b.provisional_bar.high.to_string()
            && c.data.low == b.provisional_bar.low.to_string()
            && c.data.close == b.provisional_bar.close.to_string()
            && c.data.volume == b.provisional_bar.volume.to_string(),
        "exact canonical M1 aggregation"
    );
    Ok(())
}
pub fn inspect(output: &Path) -> Result<Value> {
    let manifest: Manifest = serde_json::from_slice(&read(
        &output.join("model-manifest.json"),
        24 * 1024 * 1024,
    )?)?;
    let bytes = read(&output.join("managed-control.jsonl"), 64 * 1024 * 1024)?;
    ensure!(bytes.ends_with(b"\n"), "partial managed control tail");
    let mut records = Vec::new();
    let mut head = "0".repeat(64);
    for (n, line) in std::str::from_utf8(&bytes)?.lines().enumerate() {
        ensure!(
            n < 32768 && line.len() <= 2 * 1024 * 1024,
            "managed control bound"
        );
        let r: Value = serde_json::from_str(line)?;
        let p = &r["payload"];
        ensure!(
            p["index"] == n && p["previous_sha256"] == head && r["sha256"] == hash(p)?,
            "managed sequence/hash chain"
        );
        head = r["sha256"].as_str().context("record hash")?.into();
        records.push(p.clone());
    }
    let prep = &records.first().context("preparation missing")?["data"];
    ensure!(
        records[0]["kind"] == "managed_preparation"
            && prep["schema"] == "finam.shadow.usd-ri.managed.v1"
            && prep["library_composition"] == true
            && prep["operational_activation"] == false
            && prep["orders"] == false
            && prep["component"]["implementation_sha256"] == super::super::implementation(),
        "managed preparation identity"
    );
    let (plan, configs, calendars) =
        super::super::reader::check_preparation(&prep["component"], &manifest)?;
    read(&output.join("models.sqlite"), 512 * 1024 * 1024)?; // refuse symlink before readonly inspection
    let sql = Journal::inspect(&output.join("models.sqlite"), manifest.clone())?;
    let inputs = sql["inputs"].as_array().context("SQL input list")?;
    let outcomes = sql["results"].as_array().context("SQL outcomes")?;
    let mut matched = 0usize;
    let mut intent: Option<Candle> = None;
    let mut counts: BTreeMap<_, u64> = INSTRUMENTS.map(|i| (i, 0)).into();
    let mut progress: BTreeMap<_, SourceProgress> =
        INSTRUMENTS.map(|i| (i, SourceProgress::default())).into();
    let mut generations: BTreeMap<_, u64> = INSTRUMENTS.map(|i| (i, 0)).into();
    let mut events: BTreeMap<_, Vec<Value>> = INSTRUMENTS.map(|i| (i, vec![])).into();
    let mut snapshots = BTreeMap::new();
    let mut terminal = None;
    let mut clock = plan.start_not_before_ms;
    for (n, r) in records.iter().enumerate().skip(1) {
        ensure!(terminal.is_none(), "record after managed completion");
        let d = &r["data"];
        let kind = r["kind"].as_str().context("kind")?;
        if kind == "managed_finish" {
            ensure!(n + 1 == records.len(), "finish last");
            terminal = Some(d);
            continue;
        }
        let i: ShadowInstrument = serde_json::from_value(d["instrument"].clone())?;
        ensure!(INSTRUMENTS.contains(&i), "foreign source control");
        let at = stamp(
            d,
            if kind == "snapshot" {
                "recorded_at_ms"
            } else {
                "at_ms"
            },
        )?;
        ensure!(at >= clock, "global serialized event clocks");
        clock = at;
        match kind {
            "transport" => {
                let e = &d["event"];
                let name = e["event"].as_str().context("transport event")?;
                ensure!(
                    name.starts_with("primary_") || name == "usd_ri_lane_finalized",
                    "foreign transport event"
                );
                if name == "primary_subscription_sent" {
                    let generation = e["generation"]
                        .as_u64()
                        .context("subscription generation")?;
                    ensure!(
                        generation == generations[&i] + 1
                            && e["auth_generation"].as_u64().is_some_and(|g| g > 0)
                            && stamp(e, "sent_at_ms")? <= at,
                        "physical subscription generation/auth/clock"
                    );
                    if generation == 1 {
                        ensure!(
                            at >= plan.start_not_before_ms && at < plan.start_before_ms,
                            "first subscription entry window"
                        );
                    }
                    generations.insert(i, generation);
                }
                if name == "primary_models_source_advanced" {
                    progress.get_mut(&i).unwrap().advance(
                        e,
                        at,
                        plan.stop_at_ms,
                        calendars[&i].required(),
                    )?;
                }
                if name == "primary_models_final_frontier" {
                    progress.get_mut(&i).unwrap().final_frontier(
                        e,
                        at,
                        calendars[&i].required(),
                    )?;
                }
                events.get_mut(&i).unwrap().push(json!({"event":e}));
            }
            "snapshot" => {
                ensure!(at < plan.stop_at_ms, "HTTP attachment after deadline");
                let source = snapshot(
                    i,
                    serde_json::from_value(d["evidence"].clone())?,
                    at,
                    &d["sha256"],
                )?;
                ensure!(
                    source.evidence().start.timestamp_millis()
                        >= calendars[&i].reference_start_ms(),
                    "reference outside independent calendar"
                );
                snapshots.insert(source.sha256().into(), source);
            }
            "publication_intent" => {
                progress[&i].before_intent()?;
                ensure!(
                    intent.is_none() && generations[&i] > 0 && at < plan.stop_at_ms,
                    "serialized publication/bound source/window"
                );
                let c: Candle = serde_json::from_value(d["candle"].clone())?;
                ensure!(
                    c.instrument == i.symbol()
                        && c.start_ms
                            == manifest.freeze_at_ms + progress[&i].committed as i64 * 600000
                        && c.durable_ms <= at,
                    "independent contiguous source prefix"
                );
                projection(&c, i, &configs[&i], generations[&i], &snapshots)?;
                intent = Some(c);
            }
            "checked_readback" => {
                let c = intent.take().context("readback without intent")?;
                let row = inputs.get(matched).context("SQL readback prefix absent")?;
                let mut expected: Vec<_> = outcomes
                    .iter()
                    .filter(|o| o["input_sha256"] == c.sha256().unwrap())
                    .map(|o| o["result_id"].clone())
                    .collect();
                let mut ids = d["result_ids"].as_array().context("result IDs")?.clone();
                expected.sort_by_key(Value::to_string);
                ids.sort_by_key(Value::to_string);
                ensure!(
                    c.instrument == i.symbol()
                        && serde_json::from_value::<Candle>(row["candle"].clone())? == c
                        && d["input_sha256"] == c.sha256()?
                        && d["source_sequence"] == c.source_sequence
                        && d["revision"] == row["revision"]
                        && expected.len() == 2
                        && expected == ids
                        && at < plan.stop_at_ms,
                    "exact serialized SQL checked readback"
                );
                let p = match &c.construction {
                    Construction::DeclaredCanonicalProjectionV1 { provenance, .. } => provenance,
                    _ => unreachable!(),
                };
                if p.get("recovery_receipt").is_some() {
                    ensure!(
                        at <= stamp(&p["recovery_receipt"], "replay_deadline_ms")?,
                        "readback after replay deadline"
                    );
                } else {
                    ensure!(at - c.end_ms + 100 <= 2000, "late live readback");
                }
                matched += 1;
                *counts.get_mut(&i).unwrap() += 1;
                progress.get_mut(&i).unwrap().readback(c, at)?;
            }
            _ => anyhow::bail!("unsupported managed control kind"),
        }
    }
    let done = terminal.context("INCOMPLETE: no managed post-join completion")?;
    ensure!(
        done["schema"] == "finam.shadow.usd-ri.managed-finish.v1"
            && done["scope"] == "source_only_library"
            && done["observed_clean_session"] == false
            && done["native_systemd_qualified"] == false
            && done["full_session_qualified"] == false
            && done["real_orders"] == false
            && stamp(done, "joined_at_ms")? >= clock,
        "truthful managed completion scope/clock"
    );
    let held = done["held"].as_bool().context("held")?;
    for i in INSTRUMENTS {
        ensure!(
            counts[&i] == progress[&i].committed + u64::from(progress[&i].pending.is_some()),
            "source advance/readback count mismatch"
        );
        ensure!(
            held || progress[&i].pending.is_none(),
            "source advance missing at completion"
        );
    }
    ensure!(
        matched <= inputs.len() && inputs.len() <= matched + usize::from(intent.is_some()),
        "SQL/control prefix"
    );
    if inputs.len() > matched {
        ensure!(
            serde_json::from_value::<Candle>(inputs[matched]["candle"].clone())?
                == *intent.as_ref().context("ahead SQL intent")?,
            "ahead SQL must match intent"
        );
    }
    let reports = done["reports"].as_array().context("two reports")?;
    ensure!(reports.len() == 2, "two source reports");
    let mut seen = BTreeSet::new();
    let mut coherent = !held && intent.is_none() && inputs.len() == matched;
    for entry in reports {
        let i: ShadowInstrument = serde_json::from_value(entry["instrument"].clone())?;
        ensure!(INSTRUMENTS.contains(&i) && seen.insert(i), "unique reports");
        if !entry["result"]["Err"].is_null() {
            ensure!(held, "error result not held");
            ensure!(
                counts[&i] == 0
                    && progress[&i].committed == 0
                    && !intent.as_ref().is_some_and(|c| c.instrument == i.symbol()),
                "source advance unaccounted error report"
            );
            coherent = false;
            continue;
        }
        let r: Report = serde_json::from_value(entry["result"]["Ok"].clone())?;
        let e = &events[&i];
        let frontier = unique(e, "primary_models_final_frontier")?;
        let n = frontier["source_committed"]
            .as_u64()
            .context("final source count")?;
        ensure!(
            n == progress[&i].committed
                && n <= counts[&i]
                && counts[&i] <= n + u64::from(held)
                && r.canonical_commits == n,
            "source advance versus checked SQL"
        );
        let next = if n == 101 {
            Value::Null
        } else {
            json!(manifest.freeze_at_ms + n as i64 * 600000)
        };
        ensure!(
            frontier["next_label_ms"] == next,
            "actual independent final frontier"
        );
        let d = &r.completion;
        let calendar = unique(e, "primary_calendar_bound")?;
        ensure!(
            calendar["evidence"] == calendars[&i].evidence()
                && d.schema_version == 1
                && d.config_sha256 == configs[&i]
                && d.calendar_sha256 == calendars[&i].sha256()
                && d.eod_check_at_ms == calendars[&i].eod_check_at_ms()
                && d.stop_budget_ms == 1000
                && d.elapsed_stop_ms <= 1000
                && d.stop_started_at_ms >= stamp(calendar, "bound_at_ms")?
                && d.joined_at_ms >= d.stop_started_at_ms
                && d.joined_at_ms <= stamp(done, "joined_at_ms")?
                && !d.network_qualified
                && !d.cgroup_qualified
                && !d.model_prepared
                && !d.semantic_eod_passed
                && d.journal.is_none()
                && !r.full_session_qualified,
            "completion config/calendar/stop/join binding"
        );
        let elapsed = i128::from(d.joined_at_ms) - i128::from(d.stop_started_at_ms);
        ensure!(
            (elapsed - i128::from(d.elapsed_stop_ms)).unsigned_abs() <= 100,
            "UTC/monotonic join consistency"
        );
        let shutdown = unique(e, "primary_async_shutdown")?;
        ensure!(
            shutdown["cause"] == r.cause
                && shutdown["stop_started_at_ms"] == d.stop_started_at_ms
                && shutdown["requested_calendar_eod"] == d.calendar_eod_trigger
                && shutdown["continuity_lost"] == !d.continuity_resolved
                && shutdown["full_session_qualified"] == false
                && shutdown["http"] == r.http
                && shutdown["recovery"] == r.recovery
                && shutdown["socket_drained"] == r.socket_drained
                && shutdown["http_drained"] == r.http_drained
                && r.socket_drained == d.socket_drained
                && r.http_drained == d.http_drained
                && r.writer_drained == d.writer_drained
                && r.all_workers_drained
                    == (r.socket_drained && r.http_drained && r.writer_drained)
                && shutdown["unresolved_transport_fault"] == r.unresolved_transport_fault,
            "actual worker shutdown/report linkage"
        );
        let accounted = facts::validate_policy(&policy(), e, shutdown, d)?;
        coherent &= accounted
            && r.all_workers_drained
            && r.session_finalized
            && d.clock_consistent
            && d.fault_free
            && d.continuity_resolved
            && d.recovery_resolved
            && r.unresolved_transport_fault.is_null()
            && frontier["held"] == false
            && matches!(r.cause.as_str(), Some("RequestedStop" | "RunDeadline"));
        ensure!(
            unique(e, "usd_ri_lane_finalized")?["reason"] == "INCOMPLETE_primary_bounded_stop",
            "bounded lane finish reason"
        );
    }
    Ok(
        json!({"schema":"finam.shadow.usd-ri.managed-inspection.v1","status":if coherent{"MANAGED_PREFIX_VERIFIED"}else{"HELD"},
        "inputs":inputs.len(),"checked_inputs":matched,"control_head_sha256":head,
        "source_counts":progress.iter().map(|(i,p)|(*i,p.committed)).collect::<BTreeMap<_,_>>(),
        "checked_source_counts":counts,"models":sql["states"],
        "scope":"source_only_library","observed_clean_session":false,"full_session_qualified":false,"native_systemd_qualified":false,"real_orders":false}),
    )
}
