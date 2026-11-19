use super::*;

fn retain(name: &str, value: &Value) {
    if let Ok(root) = std::env::var("FINAM_USD_RI_R1_CASES") {
        let root = Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        std::fs::write(root.join(format!("{name}.json")), value.to_string()).unwrap();
    }
}
#[test]
fn b1_known_nonempty_terminal_and_interior_m10_cannot_be_omitted() {
    with_fixture_clock(f::T, || {
        for terminal in [false, true] {
            let mut d = draft();
            let labels = d["sources"][1]["history"]["required_labels_ms"]
                .as_array_mut()
                .unwrap();
            let n = if terminal { labels.len() - 1 } else { 2 };
            labels.remove(n);
            let err = freeze(d.to_string().as_bytes()).unwrap_err();
            assert!(
                err.to_string()
                    .contains("exact all observed nonempty M10 inventory"),
                "{err}"
            );
            retain(
                if terminal {
                    "b1-omitted-terminal"
                } else {
                    "b1-omitted-interior"
                },
                &d,
            );
        }
    });
}
#[test]
fn b1_real_empty_ri_bucket_is_not_filled_or_treated_as_a_known_omission() {
    with_fixture_clock(f::T, || {
        let mut d = draft();
        let history = &mut d["sources"][1]["history"];
        let label = history["required_labels_ms"]
            .as_array_mut()
            .unwrap()
            .remove(2)
            .as_i64()
            .unwrap();
        let part = &mut history["snapshots"][0]["evidence"]["parts"][0];
        let mut body: Value = serde_json::from_str(part["raw_body"].as_str().unwrap()).unwrap();
        body["bars"].as_array_mut().unwrap().retain(|r| {
            let t = DateTime::parse_from_rfc3339(r["timestamp"].as_str().unwrap())
                .unwrap()
                .timestamp_millis();
            t < label || t >= label + 600000
        });
        let raw = body.to_string();
        part["raw_body"] = json!(raw);
        part["declared_body_bytes"] = json!(raw.len());
        part["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
        let a = admitted(&frozen(&d)).unwrap();
        assert_eq!(a.source.manifest.seeds[2].history.len(), 201);
        assert!(a.source.manifest.seeds[2]
            .history
            .iter()
            .all(|c| c.start_ms != label));
        retain("b1-real-empty-ri-bucket", &d);
    });
}
#[test]
fn b1_calendar_requires_query_coverage_even_when_weekend_has_no_observed_rows() {
    with_fixture_clock(f::T, || {
        let mut d = draft();
        for source in d["sources"].as_array_mut().unwrap() {
            let snapshot = &mut source["calendar"]["snapshots"][0];
            let mut body: Value =
                serde_json::from_str(snapshot["raw_body"].as_str().unwrap()).unwrap();
            let last_closed = body["sessions"].as_array().unwrap().len() - 2;
            let old = body["sessions"][last_closed].clone();
            let sun = f::T - 86400000;
            let interval = |kind: &str, a: &str, b: &str| json!({"type":kind,"interval":{"start_time":a,"end_time":b}});
            let sessions = body["sessions"].as_array_mut().unwrap();
            sessions.remove(last_closed);
            sessions.insert(
                last_closed,
                interval(
                    "CLOSED",
                    old["interval"]["start_time"].as_str().unwrap(),
                    &at(sun + 3 * 3600000).to_rfc3339(),
                ),
            );
            sessions.insert(
                last_closed + 1,
                interval(
                    "CORE_TRADING",
                    &at(sun + 3 * 3600000).to_rfc3339(),
                    &at(sun + 12 * 3600000).to_rfc3339(),
                ),
            );
            sessions.insert(
                last_closed + 2,
                interval(
                    "CLOSED",
                    &at(sun + 12 * 3600000).to_rfc3339(),
                    &at(f::T).to_rfc3339(),
                ),
            );
            let raw = body.to_string();
            snapshot["raw_body"] = json!(raw);
            snapshot["declared_body_bytes"] = json!(raw.len());
            snapshot["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
            source["calendar"]["previous_session_date"] = json!("2026-11-01");
        }
        let error = freeze(d.to_string().as_bytes()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unqueried calendar trading interval"),
            "{error}"
        );
        retain("b1-unqueried-weekend", &d);
        // Complete request coverage is not an assertion of zero-volume bars.
        // A queried, actually empty weekend stays absent from the model seed.
        for source in d["sources"].as_array_mut().unwrap() {
            let snapshot = &mut source["history"]["snapshots"][0]["evidence"];
            let end = at(f::T - 86400000 + 12 * 3600000);
            snapshot["end"] = json!(end);
            snapshot["parts"][0]["end"] = json!(end);
        }
        assert!(admitted(&frozen(&d)).is_ok());
        retain("b1-queried-empty-weekend", &d);
    });
}
#[test]
fn b2_original_prepare_pin_rejects_retimestamped_metadata_before_admission() {
    with_fixture_clock(f::T, || {
        let mut d = draft();
        for source in d["sources"].as_array_mut().unwrap() {
            for key in ["asset", "params"] {
                source[key]["requested_at_ms"] = json!(f::T - 600000);
                source[key]["received_at_ms"] = json!(f::T - 599950);
            }
        }
        let old = frozen(&d);
        let pin = crate::seed::digest(&old);
        assert!(admit(&old, &pin).is_err());
        let mut changed: Value = serde_json::from_slice(&old).unwrap();
        for source in changed["draft"]["sources"].as_array_mut().unwrap() {
            for key in ["asset", "params"] {
                source[key]["requested_at_ms"] = json!(f::T - 1000);
                source[key]["received_at_ms"] = json!(f::T - 950);
            }
        }
        let changed = changed.to_string().into_bytes();
        assert_eq!(
            admit(&changed, &pin).err().unwrap().to_string(),
            "independent exact frozen input pin"
        );
        // Demonstrates why the workflow must NOT self-pin current bytes.
        assert!(admitted(&changed).is_ok());
        assert!(admit(&old, "").is_err());
    });
}
#[test]
fn b4_unknown_nested_m1_fields_never_reach_public_history() {
    with_fixture_clock(f::T, || {
        for decimal in [false, true] {
            let mut d = draft();
            let part = &mut d["sources"][0]["history"]["snapshots"][0]["evidence"]["parts"][0];
            let mut body: Value = serde_json::from_str(part["raw_body"].as_str().unwrap()).unwrap();
            if decimal {
                body["bars"][0]["close"]["review_marker"] = json!(f::ACCOUNT);
            } else {
                body["bars"][0]["review_marker"] = json!(f::ACCOUNT);
            }
            let raw = body.to_string();
            part["raw_body"] = json!(raw);
            part["declared_body_bytes"] = json!(raw.len());
            part["response_sha256"] = json!(crate::seed::digest(raw.as_bytes()));
            assert!(freeze(d.to_string().as_bytes()).is_err());
            retain(
                if decimal {
                    "b4-decimal-extension"
                } else {
                    "b4-bar-extension"
                },
                &d,
            );
        }
    });
}
#[test]
fn b3_same_completion_predicate_requires_two_exact_finalized_joined_lanes() {
    let report = |i| json!({"instrument":i,"result":{"Ok":{"all_workers_drained":true,"session_finalized":true}}});
    let good = json!({"schema":"finam.shadow.usd-ri.managed-finish.v1","reports":[report("usdrubf"),report("riz6")]});
    assert!(artifacts::finalized(&good));
    for n in 0..5 {
        let mut bad = good.clone();
        match n {
            0 => bad["reports"][0]["result"]["Ok"]["session_finalized"] = json!(false),
            1 => bad["reports"][1]["result"]["Ok"]["all_workers_drained"] = json!(false),
            2 => bad["reports"][1]["instrument"] = json!("usdrubf"),
            3 => bad["reports"][0]["result"]["Err"] = json!("synthetic"),
            _ => bad["reports"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
        };
        assert!(!artifacts::finalized(&bad));
    }
}

pub(super) fn matching_reader_completion_cases(original: &Path) {
    for case in ["unfinalized", "held-joined", "false-entry-window"] {
        let dir = Dir::new();
        let out = dir.0.join("copy");
        std::fs::create_dir(&out).unwrap();
        for name in [
            "entry.json",
            "entry-history.json",
            "entry-result.json",
            "managed-control.jsonl",
            "model-manifest.json",
            "models.sqlite",
        ] {
            std::fs::copy(original.join(name), out.join(name)).unwrap();
        }
        let control = out.join("managed-control.jsonl");
        let mut rows = std::fs::read_to_string(&control)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect::<Vec<_>>();
        rows.last_mut().unwrap()["payload"]["data"]["held"] = json!(true);
        if case == "unfinalized" {
            rows.last_mut().unwrap()["payload"]["data"]["reports"][0]["result"]["Ok"]
                ["session_finalized"] = json!(false);
        }
        let last = rows.last_mut().unwrap();
        last["sha256"] =
            json!(crate::primary::additional_models::plan::hash(&last["payload"]).unwrap());
        std::fs::write(
            &control,
            rows.iter()
                .map(|r| r.to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let core = crate::primary::additional_models::managed::reader::inspect(&out).unwrap();
        assert_eq!(core["status"], "HELD");
        if case == "false-entry-window" {
            let path = out.join("entry.json");
            let mut entry: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            entry["entry_window_open_now"] = json!(false);
            std::fs::write(path, entry.to_string()).unwrap();
        }
        let path = out.join("entry-result.json");
        let mut marker: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        marker["status"] = json!("HELD");
        marker["source_result"] = core;
        for name in [
            "entry.json",
            "entry-history.json",
            "managed-control.jsonl",
            "model-manifest.json",
            "models.sqlite",
        ] {
            marker["files_sha256"][name] =
                json!(crate::seed::digest(&std::fs::read(out.join(name)).unwrap()));
        }
        std::fs::write(path, marker.to_string()).unwrap();
        if case == "held-joined" {
            assert_eq!(inspect(&out).unwrap()["status"], "HELD");
        } else {
            assert!(inspect(&out).is_err());
        }
        if let Ok(root) = std::env::var("FINAM_USD_RI_R1_CASES") {
            let retained = Path::new(&root).join(case);
            std::fs::create_dir_all(&retained).unwrap();
            for file in std::fs::read_dir(&out).unwrap() {
                let file = file.unwrap();
                std::fs::copy(file.path(), retained.join(file.file_name())).unwrap();
            }
        }
    }
}
