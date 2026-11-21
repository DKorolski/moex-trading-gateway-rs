//! MGR-1: CURRENT-identity native fixtures, not stale-fingerprint refusals.
use super::*;
use finam_shadow_models::local::contract::{Candle, Construction, Ohlcv, Purpose};

fn name(row: &Value) -> &str {
    if row["payload"]["kind"] == "transport" {
        row["payload"]["data"]["event"]["event"].as_str().unwrap()
    } else {
        row["payload"]["kind"].as_str().unwrap()
    }
}
fn first_advance(rows: &[Value]) -> usize {
    rows.iter()
        .position(|r| {
            name(r) == "primary_models_source_advanced"
                && r["payload"]["data"]["instrument"] == "usdrubf"
        })
        .unwrap()
}
fn seal(path: &Path, rows: &mut [Value]) {
    std::fs::write(path, rehash(rows)).unwrap();
    let mut head = "0".repeat(64);
    for (n, r) in control(path).iter().enumerate() {
        let p = &r["payload"];
        assert_eq!(p["index"], n);
        assert_eq!(p["previous_sha256"], head);
        assert_eq!(r["sha256"], super::super::super::plan::hash(p).unwrap());
        head = r["sha256"].as_str().unwrap().into();
    }
}
fn retain(case: &str, output: &Path, exit: u8, status: &str, error: &str) {
    if let Ok(root) = std::env::var("FINAM_USD_RI_SOURCE_LAST_FIXTURES") {
        let dest = PathBuf::from(root).join(case);
        std::fs::create_dir_all(&dest).unwrap();
        for file in [
            "managed-control.jsonl",
            "models.sqlite",
            "model-manifest.json",
        ] {
            std::fs::copy(output.join(file), dest.join(file)).unwrap();
        }
        std::fs::write(
            dest.join("case.json"),
            json!({"case":case,"exit_code":exit,
            "status":status,"reader_error":error,"same_current_source_identity":true,
            "scope":"synthetic_source_last_only","observed_clean_session":false})
            .to_string(),
        )
        .unwrap();
    }
}

#[test]
fn current_identity_source_last_mutants_are_semantic_refusals() {
    let d = observed_pair(true);
    let output = d.0.join("managed");
    let path = output.join("managed-control.jsonl");
    let original = control(&path);
    assert_eq!(
        reader::inspect(&output).unwrap()["status"],
        "MANAGED_PREFIX_VERIFIED"
    );
    retain("positive", &output, 0, "MANAGED_PREFIX_VERIFIED", "");
    let db = std::fs::read(output.join("models.sqlite")).unwrap();
    let manifest = std::fs::read(output.join("model-manifest.json")).unwrap();
    let cases = [
        "omitted_advance",
        "early_advance",
        "foreign_advance_sha",
        "duplicate_advance",
        "foreign_instrument",
        "skipped_count",
        "wrong_label",
        "wrong_next",
        "missing_actual_clock",
        "clock_before_readback",
        "clock_after_record",
        "live_deadline",
        "omitted_last_advance",
        "extra_after_final",
        "missing_next_label",
        "omitted_last_with_held",
    ];
    for case in cases {
        let mut rows = original.clone();
        let first = first_advance(&rows);
        match case {
            "omitted_advance" => {
                rows.remove(first);
            }
            "early_advance" => {
                let e = rows.remove(first);
                let before = rows
                    .iter()
                    .position(|r| {
                        name(r) == "checked_readback"
                            && r["payload"]["data"]["instrument"] == "usdrubf"
                    })
                    .unwrap();
                rows.insert(before, e);
            }
            "foreign_advance_sha" => {
                rows[first]["payload"]["data"]["event"]["source_sha256"] = json!("0".repeat(64));
            }
            "duplicate_advance" => {
                rows.insert(first + 1, rows[first].clone());
            }
            "foreign_instrument" => {
                rows[first]["payload"]["data"]["instrument"] = json!("riz6");
            }
            "skipped_count" => {
                rows[first]["payload"]["data"]["event"]["source_committed"] = json!(2);
            }
            "wrong_label" => {
                rows[first]["payload"]["data"]["event"]["label_ms"] = json!(f::T + 600000);
            }
            "wrong_next" => {
                rows[first]["payload"]["data"]["event"]["next_label_ms"] = json!(f::T + 1200000);
            }
            "missing_actual_clock" => {
                rows[first]["payload"]["data"]["event"]
                    .as_object_mut()
                    .unwrap()
                    .remove("source_advanced_at_ms");
            }
            "clock_before_readback" => {
                let at = rows
                    .iter()
                    .find(|r| {
                        name(r) == "checked_readback"
                            && r["payload"]["data"]["instrument"] == "usdrubf"
                    })
                    .unwrap()["payload"]["data"]["at_ms"]
                    .as_i64()
                    .unwrap();
                rows[first]["payload"]["data"]["event"]["source_advanced_at_ms"] = json!(at - 1);
            }
            "clock_after_record" => {
                let at = rows[first]["payload"]["data"]["at_ms"].as_i64().unwrap();
                rows[first]["payload"]["data"]["event"]["source_advanced_at_ms"] = json!(at + 1);
            }
            "live_deadline" => {
                let at = f::T + 600000 + 1901;
                rows[first]["payload"]["data"]["at_ms"] = json!(at);
                rows[first]["payload"]["data"]["event"]["source_advanced_at_ms"] = json!(at);
            }
            "omitted_last_advance" | "omitted_last_with_held" => {
                let last = rows
                    .iter()
                    .rposition(|r| {
                        name(r) == "primary_models_source_advanced"
                            && r["payload"]["data"]["instrument"] == "usdrubf"
                    })
                    .unwrap();
                rows.remove(last);
                if case == "omitted_last_with_held" {
                    rows.last_mut().unwrap()["payload"]["data"]["held"] = json!(true);
                }
            }
            "extra_after_final" => {
                let mut extra = rows[first].clone();
                let at = rows.last().unwrap()["payload"]["data"]["joined_at_ms"]
                    .as_i64()
                    .unwrap();
                extra["payload"]["data"]["at_ms"] = json!(at);
                extra["payload"]["data"]["event"]["source_advanced_at_ms"] = json!(at);
                rows.insert(rows.len() - 1, extra);
            }
            "missing_next_label" => {
                rows[first]["payload"]["data"]["event"]
                    .as_object_mut()
                    .unwrap()
                    .remove("next_label_ms");
            }
            _ => unreachable!(),
        }
        seal(&path, &mut rows);
        assert_eq!(
            rows[0], original[0],
            "current preparation identity must not change"
        );
        let error = reader::inspect(&output).unwrap_err().to_string();
        assert!(
            error.contains("source advance"),
            "{case}: unexpected refusal: {error}"
        );
        assert_eq!(std::fs::read(output.join("models.sqlite")).unwrap(), db);
        assert_eq!(
            std::fs::read(output.join("model-manifest.json")).unwrap(),
            manifest
        );
        retain(&format!("mutants/{case}"), &output, 1, "REFUSED", &error);
    }
    seal(&path, &mut original.clone());
}

#[test]
fn advances_remain_independent_when_both_readbacks_precede_their_logging() {
    let d = observed_pair(false);
    let output = d.0.join("managed");
    let path = output.join("managed-control.jsonl");
    let mut rows = control(&path);
    let advances: Vec<_> = rows
        .iter()
        .filter(|r| name(r) == "primary_models_source_advanced")
        .cloned()
        .collect();
    rows.retain(|r| name(r) != "primary_models_source_advanced");
    let after = rows
        .iter()
        .rposition(|r| name(r) == "checked_readback")
        .unwrap()
        + 1;
    for e in advances.into_iter().rev() {
        rows.insert(after, e);
    }
    seal(&path, &mut rows);
    assert_eq!(
        reader::inspect(&output).unwrap()["status"],
        "MANAGED_PREFIX_VERIFIED"
    );
    retain("interleaved", &output, 0, "MANAGED_PREFIX_VERIFIED", "");
}

#[test]
fn sql_ahead_with_no_native_advance_stays_held_and_missing_completion_is_incomplete() {
    let d = observed_pair_case(false, true);
    let output = d.0.join("managed");
    let path = output.join("managed-control.jsonl");
    let mut rows = control(&path);
    assert_eq!(reader::inspect(&output).unwrap()["status"], "HELD");
    retain("held-before-readback", &output, 2, "HELD", "");
    // Add an independently checked synthetic readback, NOT an advance. The
    // native producer really stopped after durable SQL with source count zero.
    let manifest =
        serde_json::from_slice(&std::fs::read(output.join("model-manifest.json")).unwrap())
            .unwrap();
    let sql = Journal::inspect(&output.join("models.sqlite"), manifest).unwrap();
    let c: Candle = serde_json::from_value(sql["inputs"][0]["candle"].clone()).unwrap();
    let i = if c.instrument == "USDRUBF" {
        ShadowInstrument::Usdrubf
    } else {
        ShadowInstrument::Riz6
    };
    let input_sha = c.sha256().unwrap();
    let ids: Vec<_> = sql["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["input_sha256"] == input_sha)
        .map(|r| r["result_id"].clone())
        .collect();
    let before = rows
        .iter()
        .position(|r| name(r) == "primary_models_final_frontier")
        .unwrap();
    let at = rows[before]["payload"]["data"]["at_ms"].as_i64().unwrap();
    rows.insert(before,json!({"payload":{"kind":"checked_readback","data":{"instrument":i,
        "input_sha256":input_sha,"source_sequence":c.source_sequence,"revision":1,"result_ids":ids,"at_ms":at}}}));
    seal(&path, &mut rows);
    let report = reader::inspect(&output).unwrap();
    assert_eq!(report["status"], "HELD");
    assert_eq!(report["checked_inputs"], 1);
    assert_eq!(report["source_counts"]["usdrubf"], 0);
    assert_eq!(report["source_counts"]["riz6"], 0);
    retain("held-after-readback", &output, 2, "HELD", "");
    rows.pop();
    seal(&path, &mut rows);
    let error = reader::inspect(&output).unwrap_err().to_string();
    assert!(error.contains("INCOMPLETE"), "{error}");
    retain("missing-completion", &output, 1, "REFUSED", &error);
}

#[test]
fn actual_advance_clocks_enforce_live_replay_episode_and_stop_bounds() {
    // Only the pure file-accounting clock rule is exercised here. These DTOs
    // are NOT admitted M10/recovery capabilities or observed runtime evidence.
    let start = f::T;
    let close = start + 600000;
    for origin in [
        "WS_LIVE",
        "REST_EXCEPTION",
        "RECOVERY_BACKFILL",
        "WS_RECOVERY_BACKFILL",
    ] {
        let replay = origin.ends_with("BACKFILL");
        let ready = if replay { close + 3000 } else { close + 1100 };
        let p = json!({"canonical_receipt":{"origin":origin},"source_receipt_sha256":"a".repeat(64),
            "recovery_receipt":{"replay_deadline_ms":ready+10000,"episode_deadline_ms":ready+45000}});
        let c = Candle {
            instrument: "USDRUBF".into(),
            provider_id: "USDRUBF@RTSX".into(),
            binding_sha256: "b".repeat(64),
            source_sequence: 203,
            start_ms: start,
            end_ms: close,
            received_ms: ready,
            durable_ms: ready,
            source_processed_ms: ready,
            purpose: Purpose::Model,
            data: Ohlcv {
                open: "100".into(),
                high: "100".into(),
                low: "100".into(),
                close: "100".into(),
                volume: "1".into(),
            },
            construction: Construction::DeclaredCanonicalProjectionV1 {
                provenance_sha256: super::super::super::plan::hash(&p).unwrap(),
                provenance: p,
            },
        };
        let limit = if replay { ready + 10000 } else { close + 1900 };
        let event = |at| {
            json!({"event":"primary_models_source_advanced","label_ms":start,"source_sha256":"a".repeat(64),
            "source_committed":1,"next_label_ms":null,"source_advanced_at_ms":at})
        };
        for at in [limit, limit + 1] {
            let mut cursor = reader::SourceProgress::default();
            cursor.readback(c.clone(), ready).unwrap();
            let result = cursor.advance(&event(at), at, ready + 50000, &[start]);
            assert_eq!(result.is_ok(), at == limit, "{origin} at={at}: {result:?}");
        }
        let mut cursor = reader::SourceProgress::default();
        cursor.readback(c.clone(), ready).unwrap();
        assert!(cursor
            .advance(&event(ready), ready, ready, &[start])
            .unwrap_err()
            .to_string()
            .contains("bounded deadline"));
        let mut missing = event(ready);
        missing.as_object_mut().unwrap().remove("next_label_ms");
        let mut cursor = reader::SourceProgress::default();
        cursor.readback(c.clone(), ready).unwrap();
        assert!(
            cursor
                .advance(&missing, ready, ready + 50000, &[start])
                .is_err(),
            "explicit null next frontier is required"
        );
        if origin == "WS_RECOVERY_BACKFILL" {
            let mut c = c.clone();
            let Construction::DeclaredCanonicalProjectionV1 { provenance, .. } =
                &mut c.construction
            else {
                unreachable!()
            };
            provenance["recovery_receipt"]["episode_deadline_ms"] = json!(ready + 50);
            let mut cursor = reader::SourceProgress::default();
            cursor.readback(c, ready).unwrap();
            assert!(cursor
                .advance(&event(ready + 50), ready + 50, ready + 50000, &[start])
                .unwrap_err()
                .to_string()
                .contains("episode deadline"));
        }
    }
}
