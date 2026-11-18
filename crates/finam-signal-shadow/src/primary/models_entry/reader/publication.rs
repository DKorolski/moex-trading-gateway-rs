//! Source/consumer identity and same-clock chronology, not merely line order.
use super::*;

fn integer(v: &Value, key: &str) -> Result<i64> {
    v[key].as_i64().context("cohort publication clock")
}
fn marker<'a>(rows: &'a [Value], name: &str, label: i64) -> Result<(usize, &'a Value)> {
    let mut matches = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r["event"]["event"] == name && r["event"]["label_ms"] == label);
    let (index, row) = matches.next().context("cohort marker missing")?;
    ensure!(
        matches.next().is_none() && row["kind"] == "transport_event",
        "ambiguous cohort marker"
    );
    Ok((index, &row["event"]))
}
pub(super) fn validate(
    rows: &[Value],
    required: &[i64],
    inputs: usize,
    advances: usize,
    now: i64,
) -> Result<()> {
    for (name, count) in [
        ("primary_imoexf_models_commit", inputs),
        ("primary_runtime_cohort_barrier", inputs),
        ("primary_models_source_advanced", advances),
    ] {
        ensure!(
            rows.iter().filter(|r| r["event"]["event"] == name).count() == count,
            "cohort marker count"
        );
    }
    for (i, &label) in required.iter().take(inputs).enumerate() {
        let (ci, commit) = marker(rows, "primary_imoexf_models_commit", label)?;
        let (bi, barrier) = marker(rows, "primary_runtime_cohort_barrier", label)?;
        let mut natives = rows.iter().enumerate().filter(|(_, r)| {
            r["kind"] == "canonical_commit" && r["event"]["model_label_utc_ms"] == label
        });
        let (ni, native) = natives.next().context("native commit missing")?;
        ensure!(natives.next().is_none(), "ambiguous native commit");
        let native = &native["event"];
        let consumers = barrier["consumer_commits"]
            .as_array()
            .context("cohort consumers")?;
        ensure!(
            ni < ci
                && ci < bi
                && consumers.len() == 1
                && consumers[0]["runtime_index"] == 0
                && barrier["runtime_count"] == 3
                && barrier["additional_sqlite_model_arms"] == 2
                && barrier["shared_frontier_not_yet_advanced"] == true
                && commit["source_frontier_not_yet_advanced"] == true
                && commit["commit"]["status"] == "committed"
                && barrier["source_sha256"] == commit["source_sha256"]
                && native["source_sha256"] == commit["source_sha256"],
            "cohort source/consumer identity"
        );
        let readback = integer(commit, "readback_completed_at_ms")?;
        let consumer = integer(&consumers[0], "committed_at_ms")?;
        ensure!(
            integer(native, "publication_checked_at_ms")? <= consumer
                && consumer <= readback
                && readback <= now,
            "consumer/readback chronology"
        );
        // The matching native source determines LIVE vs WS/REST historical,
        // not a mutable barrier boolean.
        let source = rows
            .iter()
            .find(|r| {
                r["kind"] == "canonical_source"
                    && r["event"]["source_sha256"] == commit["source_sha256"]
            })
            .context("cohort source")?;
        let receipt = &source["event"]["receipt"];
        let historical = matches!(
            receipt["origin"].as_str(),
            Some("RECOVERY_BACKFILL" | "WS_RECOVERY_BACKFILL")
        );
        ensure!(
            barrier["recovery"] == historical
                && integer(receipt, "canonical_ready_at_ms")? <= consumer,
            "cohort publication mode"
        );
        if i < advances {
            let (ai, advance) = marker(rows, "primary_models_source_advanced", label)?;
            ensure!(
                bi < ai && readback <= integer(advance, "source_advanced_at_ms")?,
                "readback must precede source advancement"
            );
        }
    }
    Ok(())
}
