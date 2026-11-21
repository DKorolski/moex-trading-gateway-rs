//! The original positive source and all later replies are synthetic.
use super::*;

fn prefix_ok(a: &Artifact) -> bool {
    let rows: Vec<Value> = std::str::from_utf8(&a.journal)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let source = rows
        .iter()
        .position(|r| {
            r["kind"] == "canonical_source"
                && r["event"]["receipt"]["origin"] == "RECOVERY_BACKFILL"
        })
        .unwrap();
    let end = source
        + rows[source..]
            .iter()
            .position(|r| r["kind"] == "canonical_commit")
            .unwrap();
    let prefix: Vec<u8> = rows[..=end]
        .iter()
        .flat_map(|r| {
            let mut bytes = serde_json::to_vec(r).unwrap();
            bytes.push(b'\n');
            bytes
        })
        .collect();
    let count = rows[..=end]
        .iter()
        .filter(|r| r["kind"] == "canonical_commit")
        .count();
    let required: Vec<_> = (0..101)
        .map(|i| a.config.base.observation_start_utc_ms + i * 600000)
        .collect();
    let mut successor = a.config.clone();
    successor.base.previous_segment_id = Some(a.config.base.segment_id.clone());
    successor.base.segment_id = "v6-r1-reader-prefix".into();
    let binding = crate::primary::restart::RestartBinding {
        schema_version: 1,
        config_sha256: successor.fingerprint().unwrap(),
        previous_config_sha256: a.config.fingerprint().unwrap(),
        previous_journal_sha256: crate::seed::digest(&prefix),
        previous_segment_id: a.config.base.segment_id.clone(),
        required_labels_sha256: crate::primary::restart::inventory_sha256(&required),
        committed_labels: count,
        max_reconstruction_ms: 10000,
    };
    crate::primary::restart::admit(
        &successor,
        binding,
        &prefix,
        &required,
        required[count] + 60000,
    )
    .is_ok()
}

fn replace_hash(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(s) if s == old => *s = new.into(),
        Value::Object(m) => {
            for v in m.values_mut() {
                replace_hash(v, old, new);
            }
        }
        Value::Array(a) => {
            for v in a {
                replace_hash(v, old, new);
            }
        }
        _ => (),
    }
}

#[test]
fn tail_v6_r1_actual_replay_fences_match_in_prefix_and_full_reader() {
    let mut f = Fixture::historical();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoRestTailRecoveryV6;
    with_fixture_clock(f.open(), || {
        runtime(async {
            let (_, artifact) =
                day_scenario(f, None, false, Some(190), false, None, R1Scenario::RestTail).await;
            assert!(artifact.verify().is_ok());
            assert!(prefix_ok(&artifact));
            for (case, expected) in [
                ("publication_equal", true),
                ("publication_after_replay", false),
                ("publication_after_episode", false),
                ("result_after_replay", false),
                ("declared_expanded_resealed", false),
                ("declared_narrowed_resealed", false),
            ] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    let src = rows
                        .iter()
                        .position(|r| {
                            r["kind"] == "canonical_source"
                                && r["event"]["receipt"]["origin"] == "RECOVERY_BACKFILL"
                        })
                        .unwrap();
                    let end = src
                        + rows[src..]
                            .iter()
                            .position(|r| r["kind"] == "canonical_commit")
                            .unwrap();
                    let deadline = rows[src]["event"]["receipt"]["replay_deadline_ms"]
                        .as_i64()
                        .unwrap();
                    let episode = rows
                        .iter()
                        .find(|r| r["event"]["event"] == "primary_recovery_request_issued")
                        .unwrap()["event"]["scope"]["deadline_ms"]
                        .as_i64()
                        .unwrap();
                    match case {
                        "publication_equal" => {
                            rows[end]["event"]["publication_checked_at_ms"] = json!(deadline)
                        }
                        "publication_after_replay" => {
                            rows[end]["event"]["publication_checked_at_ms"] = json!(deadline + 1)
                        }
                        "publication_after_episode" => {
                            rows[end]["event"]["publication_checked_at_ms"] = json!(episode + 1)
                        }
                        "result_after_replay" => {
                            rows[end]["event"]["model_result_at_ms"] = json!(deadline + 1);
                            rows[end]["event"]["publication_checked_at_ms"] = json!(deadline + 1);
                        }
                        _ => {
                            let old = rows[src]["event"]["source_sha256"]
                                .as_str()
                                .unwrap()
                                .to_owned();
                            rows[src]["event"]["receipt"]["replay_deadline_ms"] = json!(
                                deadline
                                    + if case == "declared_expanded_resealed" {
                                        1
                                    } else {
                                        -1
                                    }
                            );
                            let pin = crate::primary::restart::journal::test_recovery_hash(
                                rows[src]["event"]["receipt"].clone(),
                            )
                            .unwrap();
                            for row in rows {
                                replace_hash(row, &old, &pin);
                            }
                        }
                    }
                });
                assert_eq!(bad.verify().is_ok(), expected, "full {case}");
                assert_eq!(prefix_ok(&bad), expected, "prefix {case}");
                eprintln!("V6_R1_READER_CASE {case}: expected_accept={expected} full/prefix agree");
            }
        })
    });
}
