//! Recompute semantics with matching R1 identity, not stale fingerprint refusal.
use super::*;

pub(super) fn negatives(
    f: &Fixture,
    plan: &ModelPlan,
    manifest: &Manifest,
    rows: &[Value],
    db: &std::path::Path,
    report: &Value,
) {
    let encoded = |rows: &[Value]| {
        rows.iter()
            .flat_map(|r| {
                let mut b = serde_json::to_vec(r).unwrap();
                b.push(b'\n');
                b
            })
            .collect::<Vec<_>>()
    };
    for case in [
        "postjoin_start_after_join",
        "postjoin_elapsed_over_budget",
        "postjoin_foreign_config",
        "postjoin_foreign_calendar",
        "postjoin_wrong_schema",
        "postjoin_wrong_elapsed",
        "barrier_foreign_source",
        "readback_after_advance",
        "barrier_foreign_consumer",
        "barrier_wrong_mode",
        "barrier_duplicate",
        "commit_duplicate",
        "advance_duplicate",
    ] {
        let mut r = report.clone();
        let mut changed = rows.to_vec();
        let done = &mut r["Ok"]["completion"];
        match case {
            "postjoin_start_after_join" => {
                done["stop_started_at_ms"] = json!(done["joined_at_ms"].as_i64().unwrap() + 1)
            }
            "postjoin_elapsed_over_budget" => {
                done["elapsed_stop_ms"] = json!(done["stop_budget_ms"].as_u64().unwrap() + 1)
            }
            "postjoin_foreign_config" => done["config_sha256"] = json!("0".repeat(64)),
            "postjoin_foreign_calendar" => done["calendar_sha256"] = json!("0".repeat(64)),
            "postjoin_wrong_schema" => done["schema_version"] = json!(2),
            "postjoin_wrong_elapsed" => {
                done["elapsed_stop_ms"] = json!(f.config.base.clock_uncertainty_ms + 101)
            }
            _ => {
                let name = if case == "commit_duplicate" || case == "readback_after_advance" {
                    "primary_imoexf_models_commit"
                } else if case == "advance_duplicate" {
                    "primary_models_source_advanced"
                } else {
                    "primary_runtime_cohort_barrier"
                };
                let index = changed
                    .iter()
                    .position(|r| r["event"]["event"] == name)
                    .unwrap();
                if case.ends_with("_duplicate") {
                    changed.insert(index + 1, changed[index].clone());
                    for (i, row) in changed.iter_mut().enumerate() {
                        row["sequence"] = json!(i);
                    }
                } else if case == "readback_after_advance" {
                    let advance = rows
                        .iter()
                        .find(|r| r["event"]["event"] == "primary_models_source_advanced")
                        .unwrap();
                    changed[index]["event"]["readback_completed_at_ms"] =
                        json!(advance["event"]["source_advanced_at_ms"].as_i64().unwrap() + 1);
                } else if case == "barrier_foreign_source" {
                    let other = rows
                        .iter()
                        .filter(|r| r["event"]["event"] == "primary_models_source_advanced")
                        .nth(1)
                        .unwrap();
                    changed[index]["event"]["source_sha256"] =
                        other["event"]["source_sha256"].clone();
                } else if case == "barrier_foreign_consumer" {
                    changed[index]["event"]["consumer_commits"][0]["runtime_index"] = json!(1);
                } else {
                    changed[index]["event"]["recovery"] = json!(true);
                }
            }
        }
        let outcome = reader::evaluate(
            &f.config,
            plan,
            manifest,
            &encoded(&changed),
            db,
            &r,
            crate::timing::now_ms(),
        );
        assert!(
            outcome.is_err(),
            "{case} unexpectedly qualifies: {outcome:?}"
        );
    }
}
