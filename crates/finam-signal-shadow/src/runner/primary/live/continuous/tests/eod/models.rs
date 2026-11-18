//! Full managed actor/physical owner/continuity gate with production model entry.
use super::*;
use crate::primary::models_entry::{self, reader, ModelPlan};
use finam_shadow_models::local::contract::Manifest;
mod r1;
pub(super) async fn answer_pings(peer: &mut Peer) {
    use futures_util::FutureExt;
    while let Some(Some(Ok(message))) = peer.next().now_or_never() {
        if let Message::Ping(payload) = message {
            peer.send(Message::Pong(payload)).await.unwrap();
        }
    }
}

pub(super) async fn advanced(path: &std::path::Path, count: usize) {
    until(|| {
        trace(path)
            .iter()
            .filter(|r| r["event"]["event"] == "primary_models_source_advanced")
            .count()
            == count
    })
    .await;
}
pub(super) fn export_models(
    report: RunReport,
    f: &Fixture,
    artifacts: &crate::primary::entry::artifacts::Artifacts,
    output: &std::path::Path,
    calendar_sha: &str,
    plan: ModelPlan,
    manifest: Manifest,
) -> (RunReport, Artifact) {
    let report_value = serde_json::to_value(Ok::<_, &Cause>(&report)).unwrap();
    let ordinary = artifacts
        .finish(&f.config, calendar_sha, Ok(&report))
        .unwrap();
    assert!(
        !ordinary.semantic_eod_passed,
        "old reader must not grant cohort completion"
    );
    let result =
        models_entry::artifacts::finish(output, &f.config, &plan, &manifest, &report_value, true)
            .unwrap();
    assert_eq!(
        result["status"],
        if report.canonical_commits == 0 {
            "PARTIAL"
        } else if matches!(plan.stop, models_entry::StopMode::ModelEod) {
            "MODEL_EOD"
        } else {
            "BOUNDED_COMPLETE"
        },
        "{result}"
    );
    let verified = reader::verify_directory(&f.config, output).unwrap();
    assert_eq!(verified.source_committed, report.canonical_commits);
    assert!(!verified.observed_clean_session && !verified.broker_execution);
    assert_eq!(verified.states.len(), 2);
    if let Some(root) = std::env::var_os("FINAM_MODELS_ENTRY_FIXTURES_DIR") {
        let name = if report.canonical_commits == 0 {
            "stop-after-sql"
        } else if report.canonical_commits == 101 {
            "eod-full"
        } else if trace(&output.join("journal.jsonl"))
            .iter()
            .any(|r| r["event"]["receipt"]["origin"] == "WS_RECOVERY_BACKFILL")
        {
            "bounded-ws"
        } else {
            "bounded-rest"
        };
        let destination = std::path::PathBuf::from(root).join(name);
        std::fs::create_dir(&destination).unwrap();
        for name in [
            "models-plan.json",
            "models-history.json",
            "models-context-calendar.json",
            "models-manifest.json",
            "models-admission.json",
            "models-post-join-report.json",
            "models-completion.json",
            "models-artifact-manifest.json",
            "journal.jsonl",
            "models.sqlite",
            "entry.json",
            "run-report.json",
        ] {
            std::fs::copy(output.join(name), destination.join(name)).unwrap();
        }
        std::fs::write(
            destination.join("config.json"),
            serde_json::to_vec_pretty(&f.config).unwrap(),
        )
        .unwrap();
    }
    if report.canonical_commits == 0 {
        assert!(!verified.coherent);
        assert_eq!(verified.sql_revision, 1);
        assert_eq!(verified.control_committed, 1);
        assert!(verified.states.iter().all(|s| s["bars_processed"] == 1));
        let journal = std::fs::read(output.join("journal.jsonl")).unwrap();
        let completion = serde_json::to_vec(report.completion.as_ref().unwrap()).unwrap();
        return (
            report,
            Artifact {
                config: f.config.clone(),
                journal,
                completion,
                calendar_sha: calendar_sha.into(),
                now: crate::timing::now_ms(),
            },
        );
    }
    assert!(verified.coherent);
    let db = output.join("models.sqlite");
    let db_before = std::fs::read(&db).unwrap();
    // Read-only verifier must leave the SQLite byte-for-byte unchanged.
    reader::verify_directory(&f.config, output).unwrap();
    assert_eq!(db_before, std::fs::read(&db).unwrap());
    let journal = std::fs::read(output.join("journal.jsonl")).unwrap();
    let rows: Vec<Value> = std::str::from_utf8(&journal)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let origins: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "canonical_source")
        .map(|r| r["event"]["receipt"]["origin"].as_str().unwrap())
        .collect();
    if report.canonical_commits == 5 {
        assert_eq!(origins[0], "WS_LIVE");
        assert!(origins[2].contains("RECOVERY"));
        assert_eq!(origins[3], "WS_LIVE");
        if origins[2] == "WS_RECOVERY_BACKFILL" {
            r1::negatives(f, &plan, &manifest, &rows, &db, &report_value);
        }
    }
    // Semantic negatives, not merely stale outer file digests.
    let mut bad = rows.clone();
    let frontier = bad
        .iter_mut()
        .find(|r| r["event"]["event"] == "primary_models_final_frontier")
        .unwrap();
    frontier["event"]["source_committed"] = json!(report.canonical_commits - 1);
    let encoded = |v: &[Value]| {
        v.iter()
            .flat_map(|r| {
                let mut b = serde_json::to_vec(r).unwrap();
                b.push(b'\n');
                b
            })
            .collect::<Vec<_>>()
    };
    let changed = encoded(&bad);
    let partial = reader::evaluate(
        &f.config,
        &plan,
        &manifest,
        &changed,
        &db,
        &report_value,
        crate::timing::now_ms(),
    )
    .unwrap();
    assert_eq!(partial.status, "PARTIAL");
    assert!(!partial.coherent);
    // Removing a raw retained source, with sequences repaired, must be refused.
    let mut missing = rows.clone();
    missing.retain(|r| r["kind"] != "source_snapshot");
    for (i, row) in missing.iter_mut().enumerate() {
        row["sequence"] = json!(i);
    }
    assert!(reader::evaluate(
        &f.config,
        &plan,
        &manifest,
        &encoded(&missing),
        &db,
        &report_value,
        crate::timing::now_ms()
    )
    .is_err());
    let status=std::process::Command::new("python3").arg("-c")
        .arg("import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('DELETE FROM outcomes WHERE rowid=(SELECT min(rowid) FROM outcomes)'); c.commit()")
        .arg(&db).status().unwrap();
    assert!(status.success());
    assert!(reader::evaluate(
        &f.config,
        &plan,
        &manifest,
        &journal,
        &db,
        &report_value,
        crate::timing::now_ms()
    )
    .is_err());
    std::fs::write(&db, &db_before).unwrap();
    reader::verify_directory(&f.config, output).unwrap();
    let completion = serde_json::to_vec(report.completion.as_ref().unwrap()).unwrap();
    (
        report,
        Artifact {
            config: f.config.clone(),
            journal,
            completion,
            calendar_sha: calendar_sha.into(),
            now: crate::timing::now_ms(),
        },
    )
}

#[test]
fn model_cohort_managed_live_ws_historical_live_and_matching_reader() {
    let f = models_entry::tests::fixture();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, _) =
                day_scenario(f, None, true, None, false, None, R1Scenario::ModelsWs).await;
            assert_eq!(report.canonical_commits, 5);
            assert!(report.all_workers_drained);
        })
    });
}
#[test]
fn model_cohort_managed_live_rest_historical_live_and_matching_reader() {
    let f = models_entry::tests::fixture();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, _) =
                day_scenario(f, None, true, None, false, None, R1Scenario::ModelsRest).await;
            assert_eq!(report.canonical_commits, 5);
            assert!(report.all_workers_drained);
        })
    });
}
#[test]
fn model_cohort_managed_full_grid_eod_both_arms_and_old_reader_stays_closed() {
    let f = models_entry::tests::fixture();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) =
                day_scenario(f, None, false, None, false, None, R1Scenario::ModelsEod).await;
            assert_eq!(report.canonical_commits, 101);
            assert!(report.completion.as_ref().unwrap().semantic_eod_passed);
            assert!(artifact.verify().is_err());
        })
    });
}
#[test]
fn model_cohort_managed_stop_after_sql_keeps_partial_and_source_frontier() {
    let f = models_entry::tests::fixture();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, _) =
                day_scenario(f, None, true, None, false, None, R1Scenario::ModelsStop).await;
            assert_eq!(report.canonical_commits, 0);
            assert!(report.all_workers_drained);
        })
    });
}
