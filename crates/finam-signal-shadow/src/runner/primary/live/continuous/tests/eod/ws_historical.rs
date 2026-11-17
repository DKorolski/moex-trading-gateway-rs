//! Synthetic full day, same live codec/writer/feedback, strict independent EOD.
use super::*;
#[test]
fn v8_historical_ws_runtime_journal_eod_and_resealed_reader_negatives() {
    let mut f = Fixture::historical();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8;
    let mut restart_fixture = f.clone();
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (report, artifact) =
                day_scenario(f, None, false, None, false, None, R1Scenario::WsHistorical).await;
            assert_eq!(report.canonical_commits, 101, "{report:?}");
            assert!(report.all_workers_drained, "{report:?}");
            let verified = artifact.verify().unwrap();
            assert_eq!(verified.historical_labels, 1);
            assert!(verified.semantic_eod_passed);
            let delivery = verified.delivery_evidence.as_ref().unwrap();
            assert!(delivery.complete_model_inventory);
            assert_eq!(delivery.historical_ws_inputs, 1);
            assert_eq!(delivery.historical_model_inputs, 1);
            assert!(!delivery.all_inputs_published_live);
            assert!(!verified.network_qualified && !verified.cgroup_qualified);
            let records: Vec<Value> = std::str::from_utf8(&artifact.journal)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            // Actually reconstruct a NEW owner/model from a prefix containing the
            // late WS input. No imported book/socket state and no provider calls.
            let prefix_end = records
                .iter()
                .enumerate()
                .filter(|(_, r)| r["kind"] == "canonical_commit")
                .nth(2)
                .unwrap()
                .0;
            let prefix: Vec<u8> = records[..=prefix_end]
                .iter()
                .flat_map(|r| {
                    let mut b = serde_json::to_vec(r).unwrap();
                    b.push(b'\n');
                    b
                })
                .collect();
            restart_fixture.config = artifact.config.clone();
            restart_fixture.config.base.previous_segment_id =
                Some(artifact.config.base.segment_id.clone());
            restart_fixture.config.base.segment_id = "v8-reconstructed-successor".into();
            let next_label = restart_fixture.open() + 1800000;
            restart_fixture.binding.next_label_ms = next_label;
            restart_fixture.binding.start_not_before_ms = next_label;
            restart_fixture.binding.start_before_ms = next_label + 540000;
            let restart_binding = crate::primary::restart::RestartBinding {
                schema_version: 1,
                config_sha256: restart_fixture.config.fingerprint().unwrap(),
                previous_config_sha256: artifact.config.fingerprint().unwrap(),
                previous_journal_sha256: crate::seed::digest(&prefix),
                previous_segment_id: artifact.config.base.segment_id.clone(),
                required_labels_sha256: crate::primary::restart::inventory_sha256(
                    &restart_fixture.model.required_m10_labels_ms,
                ),
                committed_labels: 3,
                max_reconstruction_ms: 10000,
            };
            let resumed_journal = with_fixture_clock(next_label + 5000, || {
                use crate::primary::entry::{tests, Startup};
                let d = Dir::new();
                let plan = tests::plan(
                    &restart_fixture,
                    Startup::CommittedPrefix {
                        binding: restart_binding.clone(),
                    },
                );
                let prepared = tests::prepared(&restart_fixture, plan, Some(&prefix)).unwrap();
                let output = d.0.join("reconstructed");
                let opened = prepared.open(&output, StopHandle::default()).unwrap();
                assert_eq!(opened.owner.processor().next_label_ms(), Some(next_label));
                assert_eq!(opened.owner.processor().counters().restart_committed, 3);
                assert!(!opened.owner.processor().subscription_data_ready());
                let b = std::fs::read(output.join("journal.jsonl")).unwrap();
                let resumed: Vec<Value> = std::str::from_utf8(&b)
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect();
                let original = records
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .nth(2)
                    .unwrap();
                let replayed = resumed
                    .iter()
                    .filter(|r| r["kind"] == "model_step")
                    .nth(2)
                    .unwrap();
                assert_eq!(original["event"]["after"], replayed["event"]["after"]);
                assert_eq!(original["event"]["decision"], replayed["event"]["decision"]);
                b
            });
            assert_eq!(
                records
                    .iter()
                    .filter(|r| r["event"]["event"] == "primary_subscription_sent")
                    .count(),
                1
            );
            for field in [
                "generation",
                "boundary_received_at_ms",
                "canonical_ready_at_ms",
                "continuity_revision",
                "origin",
                "timeliness",
                "broker_execution",
                "boundary_frame_sha256",
                "versions",
            ] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    let event = &mut rows
                        .iter_mut()
                        .find(|r| {
                            r["kind"] == "canonical_source"
                                && r["event"]["receipt"]["origin"] == "WS_RECOVERY_BACKFILL"
                        })
                        .unwrap()["event"];
                    let old = event["source_sha256"].as_str().unwrap().to_owned();
                    let r = &mut event["receipt"];
                    r[field] = match field {
                        "generation" | "continuity_revision" => json!(999),
                        "boundary_received_at_ms" => json!(r["close_ms"].as_i64().unwrap() + 1000),
                        "canonical_ready_at_ms" => {
                            json!(r["boundary_received_at_ms"].as_i64().unwrap())
                        }
                        "origin" => json!("WS_LIVE"),
                        "timeliness" => json!("LIVE"),
                        "broker_execution" => json!(true),
                        "boundary_frame_sha256" => json!("1".repeat(64)),
                        "versions" => {
                            let mut versions = r["versions"].clone();
                            versions[0]["frame_sha256"] = json!("1".repeat(64));
                            r["source_frame_sha256"][0] = json!("1".repeat(64));
                            versions
                        }
                        _ => unreachable!(),
                    };
                    let hash = crate::primary::restart::journal::test_ws_hash(r.clone());
                    for row in rows {
                        for key in ["source_sha256", "raw_provenance_sha256"] {
                            if row["event"][key] == old {
                                row["event"][key] = json!(hash);
                            }
                        }
                    }
                });
                assert!(bad.verify().is_err(), "resealed {field}");
            }
            let mut duplicate = artifact.clone();
            duplicate.change_journal(|rows| {
                let at = rows
                    .iter()
                    .position(|r| {
                        r["kind"] == "model_step" && r["event"]["mode"] == "BACKFILL_REPLAY"
                    })
                    .unwrap();
                rows.insert(at + 1, rows[at].clone());
                for (sequence, row) in rows.iter_mut().enumerate() {
                    row["sequence"] = json!(sequence);
                }
            });
            assert!(duplicate.verify().is_err(), "resealed duplicate callback");
            if let Some(path) = std::env::var_os("FINAM_V8_REVIEW_EVIDENCE_DIR") {
                let d = std::path::PathBuf::from(path).join("synthetic-ws-historical-eod");
                std::fs::create_dir_all(&d).unwrap();
                for (name, body) in [
                    ("config.json", serde_json::to_vec(&artifact.config).unwrap()),
                    ("journal.jsonl", artifact.journal.clone()),
                    ("completion.json", artifact.completion.clone()),
                    ("committed-prefix.jsonl", prefix.clone()),
                    (
                        "restart-config.json",
                        serde_json::to_vec(&restart_fixture.config).unwrap(),
                    ),
                    (
                        "restart-binding.json",
                        serde_json::to_vec(&restart_binding).unwrap(),
                    ),
                    ("reconstructed-journal.jsonl", resumed_journal.clone()),
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
