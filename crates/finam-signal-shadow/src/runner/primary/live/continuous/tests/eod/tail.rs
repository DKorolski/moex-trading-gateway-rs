//! All positive completion bodies here are SYNTHETIC, not later FINAM replies.
use super::*;
mod r1;

#[test]
fn tail_v6_full_eod_pending_renewed_ticket_historical_then_live_and_reader_negatives() {
    // The exported file fixture must also verify at actual UTC now, not
    // merely at a synthetic future test clock. No captured source is retimed.
    let mut f = Fixture::historical();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) =
                day_scenario(f, None, false, Some(190), false, None, R1Scenario::RestTail).await;
            assert_eq!(report.canonical_commits, 101);
            assert!(report.all_workers_drained);
            let verified = artifact.verify().unwrap();
            assert_eq!(verified.historical_labels, 1);
            assert!(verify::verify(
                &artifact.config,
                &artifact.binding(),
                &artifact.journal,
                &artifact.completion,
                Utc::now().timestamp_millis(),
            )
            .is_ok());
            let trace: Vec<Value> = std::str::from_utf8(&artifact.journal)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            assert_eq!(
                trace
                    .iter()
                    .filter(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
                    .count(),
                1
            );
            let issued: Vec<_> = trace
                .iter()
                .filter(|r| r["event"]["event"] == "primary_recovery_request_issued")
                .collect();
            assert_eq!(issued.len(), 2);
            assert_eq!(
                issued[0]["event"]["generation"],
                issued[1]["event"]["generation"]
            );
            assert!(
                issued[1]["event"]["issued_at_ms"].as_i64().unwrap()
                    > issued[0]["event"]["deadline_ms"].as_i64().unwrap()
            );
            let subscriptions: Vec<_> = trace
                .iter()
                .filter(|r| r["event"]["event"] == "primary_subscription_sent")
                .collect();
            assert_eq!(subscriptions.len(), 2);
            for case in 0..6 {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| match case {
                    0 => {
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_data_not_ready")
                            .unwrap();
                        r["event"]["assessment"]["missing_known_m1_ms"] = json!([]);
                    }
                    1 => {
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_request_retired")
                            .unwrap();
                        r["event"]["replacement_sha256"] = json!("0".repeat(64));
                    }
                    2 => {
                        let r = rows
                            .iter_mut()
                            .filter(|r| r["event"]["event"] == "primary_recovery_request_issued")
                            .nth(1)
                            .unwrap();
                        r["event"]["scope"]["deadline_ms"] = json!(i64::MAX);
                    }
                    3 => {
                        let old = rows
                            .iter()
                            .find(|r| r["event"]["event"] == "primary_recovery_request_issued")
                            .unwrap()["event"]["ticket_sha256"]
                            .clone();
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_source_validated")
                            .unwrap();
                        r["event"]["plan_sha256"] = old;
                    }
                    4 => {
                        let r = rows
                            .iter_mut()
                            .find(|r| {
                                r["kind"] == "model_step" && r["event"]["mode"] == "BACKFILL_REPLAY"
                            })
                            .unwrap();
                        r["event"]["mode"] = json!("LIVE_OBSERVED");
                    }
                    _ => {
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_source_validated")
                            .unwrap();
                        r["event"]["validated_at_ms"] = json!(i64::MAX);
                    }
                });
                assert!(bad.verify().is_err(), "case {case}");
            }
            if let Some(path) = std::env::var_os("FINAM_V6_REVIEW_EVIDENCE_DIR") {
                let d = std::path::PathBuf::from(path).join("pending-renewal-eod");
                std::fs::create_dir_all(&d).unwrap();
                for (name, body) in [
                    ("config.json", serde_json::to_vec(&artifact.config).unwrap()),
                    ("journal.jsonl", artifact.journal.clone()),
                    ("completion.json", artifact.completion.clone()),
                    (
                        "evidence-binding.json",
                        serde_json::to_vec(&artifact.binding()).unwrap(),
                    ),
                ] {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(d.join(name))
                        .unwrap();
                    file.write_all(&body).unwrap();
                    file.sync_all().unwrap();
                }
            }
        })
    });
}
