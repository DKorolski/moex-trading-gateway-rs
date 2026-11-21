//! Two combined state transitions requested by independent V5 R0 review.
//! All transport peers, prices/account IDs and clocks are synthetic.
use super::*;

fn rows(a: &Artifact) -> Vec<Value> {
    std::str::from_utf8(&a.journal)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn replace_hash(value: &mut Value, old: &str, new: &str) {
    match value {
        Value::String(s) if s == old => *s = new.into(),
        Value::Array(a) => a.iter_mut().for_each(|v| replace_hash(v, old, new)),
        Value::Object(o) => o.values_mut().for_each(|v| replace_hash(v, old, new)),
        _ => (),
    }
}
fn changed_reference_inventory(a: &Artifact, extra: bool) -> Artifact {
    use broker_finam::sparse_m10::{
        AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1SnapshotEvidenceV1,
    };
    let original = rows(a);
    let old = original
        .iter()
        .find(|r| {
            r["event"]["event"] == "primary_reference_installed"
                && r["event"]["role"] == "StartupAnchor"
        })
        .unwrap()["event"]["source_sha256"]
        .as_str()
        .unwrap();
    let snapshot = &original
        .iter()
        .find(|r| r["kind"] == "source_snapshot" && r["event"]["snapshot_sha256"] == old)
        .unwrap()["event"];
    let mut evidence: ClosedM1SnapshotEvidenceV1 =
        serde_json::from_value(snapshot["evidence"].clone()).unwrap();
    let plan = ClosedM1RequestPlan::new(
        evidence.start,
        evidence.end,
        evidence.parts.iter().map(|p| p.end).collect(),
    )
    .unwrap();
    let p = &mut evidence.parts[0];
    let mut raw: Value = serde_json::from_str(&p.raw_body).unwrap();
    let mut extra_row = raw["bars"][0].clone();
    extra_row["timestamp"] = json!((evidence.end - chrono::Duration::minutes(1)).to_rfc3339());
    if !extra {
        raw["bars"] = json!([]);
    }
    raw["bars"].as_array_mut().unwrap().push(extra_row);
    p.raw_body = raw.to_string();
    p.declared_body_bytes = Some(p.raw_body.len() as u64);
    p.response_sha256 = crate::seed::digest(p.raw_body.as_bytes());
    let collected = p.received_at;
    // A genuine admitted replacement cap: no malformed source/hash shortcuts.
    let source = AdmittedClosedM1Snapshot::admit(evidence, &plan, collected).unwrap();
    let mut bad = a.clone();
    bad.change_journal(|rows| {
        let row = rows
            .iter_mut()
            .find(|r| r["kind"] == "source_snapshot" && r["event"]["snapshot_sha256"] == old)
            .unwrap();
        row["event"]["evidence"] = serde_json::to_value(source.evidence()).unwrap();
        row["event"]["accounting"] = serde_json::to_value(source.accounting()).unwrap();
        if !extra {
            rows.retain(|r| r["event"]["change"]["context"] != "REST_REFERENCE_VERSIONED");
        }
        for (i, row) in rows.iter_mut().enumerate() {
            replace_hash(row, old, source.sha256());
            row["sequence"] = json!(i);
        }
    });
    let canonical = |rows: Vec<Value>| {
        rows.into_iter()
            .filter(|r| {
                matches!(
                    r["kind"].as_str(),
                    Some("canonical_source" | "model_step" | "canonical_commit")
                )
            })
            .map(|r| r["event"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(canonical(original), canonical(rows(&bad)));
    bad
}
fn fixture() -> Fixture {
    let mut f = Fixture::historical();
    f.config.primary.delivery = crate::primary::DeliveryPolicy::ObservedBoConsistentSourcesV5;
    f.config.primary.audit_every_commits = Some(6);
    f
}
fn save(a: &Artifact, report: &RunReport, name: &str) {
    let result = a.verify();
    eprintln!(
        "R1_REPRO {name}: commits={} recovery={} reader={result:?}",
        report.canonical_commits,
        report.recovery.len()
    );
    let Some(root) = std::env::var_os("FINAM_V5_REVIEW_EVIDENCE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(root).join(name);
    std::fs::create_dir(dir.clone()).unwrap();
    let (verified, refusal) = match result {
        Ok(v) => (serde_json::to_value(v).unwrap(), Value::Null),
        Err(e) => (Value::Null, json!(format!("{e:?}"))),
    };
    for (name,bytes) in [
        ("config.json",serde_json::to_vec(&a.config).unwrap()),
        ("evidence-binding.json",serde_json::to_vec(&a.binding()).unwrap()),
        ("journal.jsonl",a.journal.clone()),("completion.json",a.completion.clone()),
        ("synthetic-result.json",serde_json::to_vec(&json!({"synthetic_only":true,"provider_calls":false,"verified":verified,"reader_refusal":refusal,"commits":report.canonical_commits,"recovery_episodes":report.recovery.len(),"audit_every_commits":6,"parity_accepted":false})).unwrap()),
    ] {std::fs::write(dir.join(name),bytes).unwrap();}
}

#[test]
fn consistent_v5_r1_reference_install_before_stale_frame_survives_recovery_and_eod() {
    let f = fixture();
    let open = f.open();
    with_fixture_clock(open, || {
        runtime(async {
            let (report, artifact) = day_scenario(
                f,
                None,
                false,
                None,
                false,
                None,
                R1Scenario::StaleReference,
            )
            .await;
            save(&artifact, &report, "reference-stale-recovery");
            assert_eq!(report.canonical_commits, 101);
            assert_eq!(report.recovery.len(), 1);
            artifact.verify().unwrap();
            let rows = rows(&artifact);
            let bootstrap = &rows
                .iter()
                .find(|r| r["event"]["event"] == "primary_bootstrap_proof_required")
                .unwrap()["event"];
            assert_eq!(bootstrap["original_received_at_ms"], open + 100);
            assert!(!rows
                .iter()
                .any(|r| r["event"]["event"] == "primary_bootstrap_proof_applied"));
            let installed: Vec<_> = rows
                .iter()
                .filter(|r| r["event"]["event"] == "primary_reference_installed")
                .collect();
            assert_eq!(installed.len(), 2);
            assert!(
                installed[0]["sequence"].as_u64().unwrap()
                    < rows
                        .iter()
                        .find(|r| r["event"]["change"]["context"] == "REST_REFERENCE_VERSIONED")
                        .unwrap()["sequence"]
                        .as_u64()
                        .unwrap()
            );
            assert_eq!(installed[0]["event"]["role"], "Bootstrap");
            assert_eq!(installed[1]["event"]["role"], "StartupAnchor");
            for case in [
                "missing-A",
                "missing-B",
                "unlinked-source",
                "wrong-plan",
                "snapshot-only",
            ] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    let position = rows
                        .iter()
                        .position(|r| {
                            r["event"]["event"] == "primary_reference_installed"
                                && r["event"]["role"]
                                    == if case == "missing-B" {
                                        "StartupAnchor"
                                    } else {
                                        "Bootstrap"
                                    }
                        })
                        .unwrap();
                    match case {
                        "missing-A" | "missing-B" => {
                            rows.remove(position);
                        }
                        "unlinked-source" => {
                            rows[position]["event"]["source_sha256"] = json!("f".repeat(64))
                        }
                        "wrong-plan" => {
                            rows[position]["event"]["plan_sha256"] = json!("f".repeat(64))
                        }
                        _ => rows[position]["event"]["event"] = json!("optional_snapshot_seen"),
                    }
                    for (i, row) in rows.iter_mut().enumerate() {
                        row["sequence"] = json!(i);
                    }
                });
                assert!(
                    matches!(bad.verify(), Err(verify::VerifyError::Source)),
                    "{case}"
                );
            }
            for extra in [false, true] {
                assert!(matches!(
                    changed_reference_inventory(&artifact, extra).verify(),
                    Err(verify::VerifyError::Source)
                ));
            }
        })
    });
}

#[test]
fn consistent_v5_r1_retained_aba_identity_survives_recovery_eviction_and_eod() {
    let f = fixture();
    let open = f.open();
    with_fixture_clock(open, || {
        runtime(async {
            let (report, artifact) = day_scenario(
                f,
                None,
                false,
                Some(10),
                false,
                None,
                R1Scenario::RepeatedWitness,
            )
            .await;
            save(&artifact, &report, "retained-witness-aba");
            assert_eq!(report.canonical_commits, 101);
            assert_eq!(report.recovery.len(), 1);
            artifact.verify().unwrap();
            let rows = rows(&artifact);
            let pending = &rows
                .iter()
                .find(|r| r["event"]["change"]["context"] == "RECOVERY_PENDING")
                .unwrap()["event"]["change"];
            let discard = &rows
                .iter()
                .find(|r| r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED")
                .unwrap()["event"]["change"];
            assert_eq!(
                pending["expected_ws_version"],
                discard["superseded_ws_version"]
            );
            assert_eq!(
                pending["expected_ws_version"]["last_changed_at_ms"],
                open + 100
            );
            let source = &rows
                .iter()
                .find(|r| r["kind"] == "canonical_source")
                .unwrap()["event"];
            assert_eq!(source["receipt"]["origin"], "RECOVERY_BACKFILL");
            assert_eq!(source["receipt"]["actual_m1"][0]["high"], "2200.5");
            assert_eq!(
                discard["original_canonical_source_sha256"],
                source["source_sha256"]
            );
            assert_eq!(
                rows.iter()
                    .filter(
                        |r| r["kind"] == "model_step" && r["event"]["model_label_utc_ms"] == open
                    )
                    .count(),
                1
            );
            for case in ["witness", "canonical-source", "unknown-price"] {
                let mut bad = artifact.clone();
                bad.change_journal(|rows| {
                    let row = rows
                        .iter_mut()
                        .find(|r| {
                            r["event"]["change"]["context"] == "WS_SUPERSEDED_REPEAT_DISCARDED"
                        })
                        .unwrap();
                    let c = &mut row["event"]["change"];
                    match case {
                        "witness" => {
                            c["superseded_ws_version"]["last_changed_at_ms"] = json!(open + 300)
                        }
                        "canonical-source" => {
                            c["original_canonical_source_sha256"] = json!("f".repeat(64))
                        }
                        _ => c["observed"]["close"] = json!("2200"),
                    }
                });
                assert!(
                    matches!(bad.verify(), Err(verify::VerifyError::Source)),
                    "{case}"
                );
            }
        })
    });
}
