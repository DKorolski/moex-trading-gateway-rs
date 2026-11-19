use super::super::tests as f;
use super::*;
use crate::{primary::calendar::tests::Dir, timing::with_fixture_clock};
use broker_finam::sparse_m10::*;
use chrono::{DateTime, Utc};

fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap()
}
fn draft() -> Value {
    let m = f::manifest();
    let sources=INSTRUMENTS.map(|instrument|{
        let e=f::evidence(instrument);
        let rows=&m.seeds.iter().find(|s|s.binding.instrument==instrument.symbol()).unwrap().history;
        let labels=rows.iter().map(|c|c.start_ms).collect::<Vec<_>>();
        let start=at(labels[0]);let end=at(labels.last().unwrap()+600000);
        let bars=rows.iter().flat_map(|c|(0..10).map(move|minute|{
            json!({"timestamp":at(c.start_ms+minute*60000).to_rfc3339(),
                "open":{"value":c.data.open},"high":{"value":c.data.high},"low":{"value":c.data.low},
                "close":{"value":c.data.close},"volume":{"value":if minute==0{"1"}else{"0"}}})
        })).collect::<Vec<_>>();
        let raw=json!({"symbol":instrument.venue_symbol(),"bars":bars}).to_string();
        let snapshot=ClosedM1SnapshotEvidenceV1{policy:OBSERVED_M1_POLICY_V1.into(),start,end,parts:vec![ClosedM1RestPartV1{
            method:"GET".into(),endpoint:format!("https://api.finam.ru/v1/instruments/{}/bars",instrument.venue_symbol()),
            symbol:instrument.venue_symbol().into(),timeframe:"TIME_FRAME_M1".into(),start,end,
            requested_at:at(f::T-5000),received_at:at(f::T-4000),status:200,transport_complete:true,
            declared_body_bytes:Some(raw.len() as u64),response_sha256:crate::seed::digest(raw.as_bytes()),raw_body:raw}]};
        json!({"instrument":instrument,"asset":e.asset,"params":e.params,"calendar":e.calendar,
            "bootstrap_reference_start_ms":e.bootstrap_reference_start_ms,
            "history":{"instrument":instrument,"required_labels_ms":labels,
                "snapshots":[{"admitted_at_ms":f::T-3000,"evidence":snapshot}]}})
    });
    json!({"schema":"finam.shadow.usd-ri.input-draft.v1","account_id":f::ACCOUNT,
        "generation":"synthetic-usd-ri-executable","session_date":m.session_date,
        "start_not_before_ms":f::T,"start_before_ms":f::T+540000,"stop_at_ms":f::T+14*600000,"sources":sources})
}
fn frozen(d: &Value) -> Vec<u8> {
    freeze(&serde_json::to_vec(d).unwrap()).unwrap()
}

#[test]
fn exact_raw_seed_rebuild_then_fresh_admission_has_four_ready_arms_and_no_account_in_receipt() {
    with_fixture_clock(f::T, || {
        let raw = draft();
        let bytes = frozen(&raw);
        let a = admit(&bytes).unwrap();
        let states =
            finam_shadow_models::local::Journal::initial_states(&a.source.manifest).unwrap();
        assert_eq!(states.iter().filter(|s| s["ready"] == true).count(), 4);
        assert_eq!(a.account_id(), f::ACCOUNT);
        let r = a.receipt().unwrap();
        assert_eq!(r["entry_window_open_now"], true);
        assert_eq!(r["operational_authority"], false);
        assert!(!r.to_string().contains(f::ACCOUNT));
        if let Ok(root) = std::env::var("FINAM_USD_RI_EXECUTABLE_INPUT_FIXTURE") {
            let path = Path::new(&root);
            std::fs::create_dir(path).unwrap();
            std::fs::write(path.join("draft.json"), raw.to_string()).unwrap();
            std::fs::write(path.join("frozen.json"), &bytes).unwrap();
            std::fs::write(path.join("admission.json"), r.to_string()).unwrap();
        }
    });
}
#[test]
fn sparse_seed_keeps_nominal_m10_but_empty_m10_and_overlapping_snapshots_refuse() {
    with_fixture_clock(f::T, || {
        let mutate = |value: &mut Value, empty: bool| {
            let part = &mut value["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0];
            let mut body: Value = serde_json::from_str(part["raw_body"].as_str().unwrap()).unwrap();
            let rows = body["bars"].as_array_mut().unwrap();
            if empty {
                rows.drain(..10);
            } else {
                rows.remove(8);
                rows.remove(1);
            }
            let raw = body.to_string();
            part["declared_body_bytes"] = json!(raw.len());
            part["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
            part["raw_body"] = json!(raw);
        };
        let good = draft();
        let mut sparse = good.clone();
        mutate(&mut sparse, false);
        let accepted = admit(&frozen(&sparse)).unwrap();
        assert_eq!(accepted.source.manifest.seeds[1].history.len(), 202);
        assert_eq!(
            accepted.source.manifest.seeds[1].history[0].start_ms,
            good["sources"][0]["history"]["required_labels_ms"][0]
                .as_i64()
                .unwrap()
        );
        let mut empty = good.clone();
        mutate(&mut empty, true);
        assert!(freeze(empty.to_string().as_bytes()).is_err());
        let mut overlap = good;
        let snapshot = overlap["sources"][0]["history"]["snapshots"][0].clone();
        overlap["sources"][0]["history"]["snapshots"]
            .as_array_mut()
            .unwrap()
            .push(snapshot);
        assert!(freeze(overlap.to_string().as_bytes()).is_err());
    });
}
#[test]
fn draft_and_frozen_mutations_fail_closed_without_network_or_clock_relabeling() {
    with_fixture_clock(f::T, || {
        let good = draft();
        for n in 0..11 {
            let mut bad = good.clone();
            match n {
                0 => bad["account_id"] = json!(" ACCOUNT "),
                1 => bad["sources"]
                    .as_array_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap(),
                2 => bad["sources"][1]["instrument"] = json!("usdrubf"),
                3 => bad["sources"][0]["history"]["instrument"] = json!("riz6"),
                4 => {
                    bad["sources"][0]["history"]["required_labels_ms"][0] = json!(i64::MAX - 600000)
                }
                5 => {
                    bad["sources"][0]["history"]["snapshots"][0]["admitted_at_ms"] = json!(f::T + 1)
                }
                6 => {
                    bad["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0]
                        ["transport_complete"] = json!(false)
                }
                7 => {
                    bad["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0]
                        ["response_sha256"] = json!("0".repeat(64))
                }
                8 => {
                    bad["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0]["symbol"] =
                        json!("IMOEXF@RTSX")
                }
                9 => {
                    bad["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0]
                        ["received_at"] = json!(at(f::T + 1))
                }
                _ => bad["unexpected"] = json!(true),
            };
            assert!(freeze(bad.to_string().as_bytes()).is_err(), "mutation {n}");
        }
        let bytes = frozen(&good);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        for n in 0..6 {
            let mut bad = value.clone();
            match n {
                0 => bad["plan"]["manifest_sha256"] = json!("0".repeat(64)),
                1 => bad["manifest"]["seeds"][1]["history"][0]["data"]["close"] = json!("99"),
                2 => bad["draft"]["sources"][0]["params"]["raw_body"] = json!("{}"),
                3 => {
                    bad["draft"]["sources"][1]["calendar"]["snapshots"][0]["transport_complete"] =
                        json!(false)
                }
                4 => bad["draft"]["account_id"] = json!("OTHER_ACCOUNT"),
                _ => bad["clock_override"] = json!(f::T),
            };
            assert!(
                admit(bad.to_string().as_bytes()).is_err(),
                "frozen mutation {n}"
            );
        }
        assert!(freeze(br#"{"schema":1,"schema":2}"#).is_err());
        f::go(f::T + 540000);
        assert!(admit(&bytes).is_err(), "no replay of expired fresh entry");
    });
}
#[test]
fn file_creation_is_private_create_new_and_refuses_symlink_and_oversize() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let input = d.0.join("draft");
        let output = d.0.join("frozen");
        std::fs::write(&input, draft().to_string()).unwrap();
        assert_eq!(
            freeze_file(&input, &output).unwrap()["status"],
            "INPUTS_FROZEN_ONLY"
        );
        let before = std::fs::read(&output).unwrap();
        assert!(freeze_file(&input, &output).is_err());
        assert_eq!(before, std::fs::read(&output).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let link = d.0.join("link");
            std::os::unix::fs::symlink(&input, &link).unwrap();
            assert!(freeze_file(&link, &d.0.join("no")).is_err());
        }
        assert!(freeze(&vec![b' '; MAX_INPUT_BYTES + 1]).is_err());
    });
}
#[test]
fn opened_executable_drives_both_real_v8_fixtures_reconnects_and_closes_matching_export() {
    with_fixture_clock(f::T, || {
        let bytes = frozen(&draft());
        let admitted = admit(&bytes).unwrap();
        let d = Dir::new();
        let output = d.0.join("managed");
        let opened = admitted.open(&output, StopHandle::default()).unwrap();
        let (_d, finished) = super::super::managed::tests::executable_fixture(opened.managed, d);
        let report = artifacts::finish(&output, Ok(finished)).unwrap();
        assert_eq!(report["status"], "MANAGED_PREFIX_VERIFIED");
        assert_eq!(inspect(&output).unwrap(), report);
        assert_eq!(report["source_result"]["inputs"], 4);
        assert_eq!(report["operational_authority"], false);
        let before = std::fs::read(output.join("entry-result.json")).unwrap();
        assert!(artifacts::finish(
            &output,
            Ok(super::super::managed::Finished {
                output: output.clone(),
                evidence: json!({"reports":[
                {"result":{"Ok":{"all_workers_drained":true,"session_finalized":true}}},
                {"result":{"Ok":{"all_workers_drained":true,"session_finalized":true}}}]})
            })
        )
        .is_err());
        assert_eq!(
            before,
            std::fs::read(output.join("entry-result.json")).unwrap()
        );
        let path = output.join("entry-history.json");
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"{}").unwrap();
        assert!(inspect(&output).is_err());
        std::fs::write(&path, &original).unwrap();
        let marker = output.join("entry-result.json");
        std::fs::remove_file(&marker).unwrap();
        assert!(inspect(&output).is_err());
        std::fs::write(&marker, &before).unwrap();
        if let Ok(root) = std::env::var("FINAM_USD_RI_EXECUTABLE_FIXTURE") {
            let p = Path::new(&root);
            std::fs::create_dir(p).unwrap();
            for name in [
                "entry.json",
                "entry-history.json",
                "managed-control.jsonl",
                "model-manifest.json",
                "models.sqlite",
                "entry-result.json",
            ] {
                let contents = std::fs::read(output.join(name)).unwrap();
                if name != "entry-history.json" {
                    assert!(!String::from_utf8_lossy(&contents).contains(f::ACCOUNT));
                }
                std::fs::copy(output.join(name), p.join(name)).unwrap();
            }
        }
    });
}
#[test]
fn cancelled_export_is_incomplete_and_wrong_auth_never_opens_or_calls_network() {
    with_fixture_clock(f::T, || {
        let d = Dir::new();
        let output = d.0.join("incomplete");
        std::fs::create_dir(&output).unwrap();
        assert_eq!(
            artifacts::finish(&output, Err(anyhow::anyhow!("synthetic"))).unwrap()["status"],
            "INCOMPLETE"
        );
        assert!(!output.join("entry-result.json").exists());
        assert!(inspect(&output).is_err());
        let auth = AuthLifecycle::new(
            broker_finam::SecretToken::new("SYNTHETIC_NON_CREDENTIAL"),
            "OTHER_ACCOUNT".into(),
        )
        .unwrap();
        let a = admit(&frozen(&draft())).unwrap();
        let path = d.0.join("wrong-account");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(rt
            .block_on(run(a, auth, &path, StopHandle::default()))
            .is_err());
        assert!(!path.exists());
    });
}
