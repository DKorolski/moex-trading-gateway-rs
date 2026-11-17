//! Native managed two-episode composition; all provider responses synthetic.
use super::*;

fn prefix(artifact: &Artifact, records: &[Value]) -> [(&'static str, Vec<u8>); 3] {
    let end = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r["kind"] == "canonical_commit")
        .nth(3)
        .unwrap()
        .0;
    let bytes: Vec<u8> = records[..=end]
        .iter()
        .flat_map(|r| {
            let mut b = serde_json::to_vec(r).unwrap();
            b.push(b'\n');
            b
        })
        .collect();
    let mut successor = artifact.config.clone();
    successor.base.previous_segment_id = Some(artifact.config.base.segment_id.clone());
    successor.base.segment_id = "v8-r1-two-episode-prefix".into();
    let required: Vec<_> = (0..101)
        .map(|n| artifact.config.base.observation_start_utc_ms + n * 600000)
        .collect();
    let binding = crate::primary::restart::RestartBinding {
        schema_version: 1,
        config_sha256: successor.fingerprint().unwrap(),
        previous_config_sha256: artifact.config.fingerprint().unwrap(),
        previous_journal_sha256: crate::seed::digest(&bytes),
        previous_segment_id: artifact.config.base.segment_id.clone(),
        required_labels_sha256: crate::primary::restart::inventory_sha256(&required),
        committed_labels: 4,
        max_reconstruction_ms: 10000,
    };
    let admitted = crate::primary::restart::admit(
        &successor,
        binding.clone(),
        &bytes,
        &required,
        required[4] + 5000,
    )
    .unwrap();
    assert_eq!(admitted.committed_labels(), 4);
    [
        (
            "prefix-config.json",
            serde_json::to_vec(&successor).unwrap(),
        ),
        ("prefix-binding.json", serde_json::to_vec(&binding).unwrap()),
        ("committed-prefix.jsonl", bytes),
    ]
}
async fn composition(scenario: R1Scenario) {
    let mut f = Fixture::historical();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoWsHistoricalRecoveryV8;
    let (report, artifact) = day_scenario(f, None, false, None, false, None, scenario).await;
    assert_eq!(report.canonical_commits, 101, "{report:?}");
    assert!(report.all_workers_drained);
    assert_eq!(report.recovery.len(), 2);
    assert!(report
        .recovery
        .iter()
        .all(|r| r.generation == 1 && !r.canonical_unresolved && !r.transport_unresolved));
    let verified = artifact.verify().unwrap();
    assert_eq!(verified.historical_labels, 2);
    let expected_ws = match scenario {
        R1Scenario::RestThenRest => 0,
        R1Scenario::RestThenWs => 1,
        _ => 2,
    };
    assert_eq!(
        verified.delivery_evidence.unwrap().historical_ws_inputs,
        expected_ws
    );
    let records: Vec<Value> = std::str::from_utf8(&artifact.journal)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        records
            .iter()
            .filter(|r| r["event"]["event"] == "primary_subscription_sent")
            .count(),
        1
    );
    let prefix_files = prefix(&artifact, &records);
    // Full verifier must reject unrelated retirement and new authority before
    // the preceding episode Ready, even with recomputed outer seals.
    if scenario == R1Scenario::RestThenWs {
        for case in 0..6 {
            let mut bad = artifact.clone();
            bad.change_journal(|rows| {
                match case {
                    0..=2 => {
                        let r = &mut rows
                            .iter_mut()
                            .find(|r| {
                                r["kind"] == "canonical_source"
                                    && r["event"]["receipt"]["origin"] == "RECOVERY_BACKFILL"
                            })
                            .unwrap()["event"]["receipt"];
                        match case {
                            0 => r["generation"] = json!(2),
                            1 => r["label_ms"] = json!(r["label_ms"].as_i64().unwrap() + 600000),
                            _ => r["recovery_plan_sha256"] = json!("0".repeat(64)),
                        }
                    }
                    3 => {
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_ws_data_gap_authorized")
                            .unwrap();
                        r["event"]["continuity_revision"] = json!(999);
                    }
                    4 => {
                        let old = rows
                            .iter()
                            .find(|r| r["event"]["event"] == "primary_ws_data_gap_authorized")
                            .unwrap()
                            .clone();
                        let at = rows
                            .iter()
                            .position(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                            .unwrap();
                        rows.insert(at, old);
                    }
                    _ => {
                        let r = rows
                            .iter_mut()
                            .find(|r| r["event"]["event"] == "primary_recovery_episode_ready")
                            .unwrap();
                        r["event"]["audit"]["episode"] = json!(2);
                    }
                }
                for (n, row) in rows.iter_mut().enumerate() {
                    row["sequence"] = json!(n);
                }
            });
            assert!(bad.verify().is_err(), "resealed composition case {case}");
        }
    }
    if let Some(root) = std::env::var_os("FINAM_V8_R1_REVIEW_EVIDENCE_DIR") {
        let name = match scenario {
            R1Scenario::RestThenWs => "rest-then-ws",
            R1Scenario::RestThenRest => "rest-then-rest",
            _ => "ws-then-ws",
        };
        let d = std::path::PathBuf::from(root).join(name);
        std::fs::create_dir_all(&d).unwrap();
        let mut files = vec![
            ("config.json", serde_json::to_vec(&artifact.config).unwrap()),
            ("journal.jsonl", artifact.journal.clone()),
            ("completion.json", artifact.completion.clone()),
            (
                "evidence-binding.json",
                serde_json::to_vec(&artifact.binding()).unwrap(),
            ),
        ];
        files.extend(prefix_files);
        for (name, body) in files {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(d.join(name))
                .unwrap();
            file.write_all(&body).unwrap();
            file.sync_all().unwrap();
        }
    }
}
#[test]
fn v8_r1_two_episode_rest_then_ws_eod_and_second_prefix() {
    let f = Fixture::historical();
    with_fixture_clock(f.open(), || runtime(composition(R1Scenario::RestThenWs)));
}
#[test]
fn v8_r1_two_episode_rest_then_rest_eod_and_second_prefix() {
    let f = Fixture::historical();
    with_fixture_clock(f.open(), || runtime(composition(R1Scenario::RestThenRest)));
}
#[test]
fn v8_r1_two_episode_ws_then_ws_eod_and_second_prefix() {
    let f = Fixture::historical();
    with_fixture_clock(f.open(), || runtime(composition(R1Scenario::WsThenWs)));
}
