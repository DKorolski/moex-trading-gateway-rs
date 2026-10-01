use super::*;
use chrono::TimeZone;
use serde_json::json;

fn rehash(value: &mut Value) {
    value["history_coverage"]["sessions_sha256"] = json!(canonical_value_sha256(
        &value["history_coverage"]["sessions"]
    ));
    value["riskgate_history"]["history_bars_sha256"] =
        json!(canonical_value_sha256(&value["history_bars"]));
    value["riskgate_history"]["session_observations_sha256"] = json!(canonical_value_sha256(
        &value["riskgate_history"]["session_observations"]
    ));
}

fn fixture(hour: u32, minute: u32) -> (Value, DateTime<Utc>) {
    fixture_for_identity(hour, minute, &"1".repeat(64))
}

fn fixture_for_identity(hour: u32, minute: u32, identity: &str) -> (Value, DateTime<Utc>) {
    let (bytes, _, _, _) = tests::fixture_for_binding(identity, "ACC_TEST_0001");
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    let candidate_close = Utc.with_ymd_and_hms(2026, 9, 28, hour, minute, 0).unwrap();
    let now = candidate_close + Duration::seconds(30);
    value["schema_version"] = json!(3);
    value["domain"] = json!(STAGE8B_P1E_FIRST_BOOT_SOURCE_V3_DOMAIN);
    value["runtime_profile_sha256"] = json!(Stage8bP1RuntimeProfileKind::V2.profile_sha256());
    value["captured_at_utc"] = json!(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    value["broker_truth"]["checked_at_utc"] = value["captured_at_utc"].clone();
    let mut history = Vec::new();
    let mut sessions = Vec::new();
    for day in [22, 23, 24, 25, 28] {
        let midnight = Utc
            .with_ymd_and_hms(2026, 9, day, 0, 0, 0)
            .unwrap()
            .timestamp();
        let full = vec![
            json!({"first_close_time_utc":midnight+15000,"last_close_time_utc":midnight+39000}),
            json!({"first_close_time_utc":midnight+40200,"last_close_time_utc":midnight+57000}),
            json!({"first_close_time_utc":midnight+58200,"last_close_time_utc":midnight+75000}),
        ];
        let date = format!("2026-09-{day:02}");
        if day == 28 {
            value["history_coverage"]["candidate_session"] =
                json!({"session_date":date,"windows":full});
        }
        let mut windows = Vec::new();
        for window in full {
            let first = window["first_close_time_utc"].as_i64().unwrap();
            let last = window["last_close_time_utc"]
                .as_i64()
                .unwrap()
                .min(candidate_close.timestamp() - 600);
            if first > last {
                continue;
            }
            windows.push(json!({"first_close_time_utc":first,"last_close_time_utc":last}));
            for close in (first..=last).step_by(600) {
                history.push(json!({
                    "instrument":STAGE8B_P1_VENUE_SYMBOL,"timeframe_sec":600,
                    "close_time_utc":close,"open":"2200","high":"2201","low":"2199",
                    "close":"2200","volume":"20","is_final":true,"origin":"history"
                }));
            }
        }
        if !windows.is_empty() {
            sessions.push(json!({"session_date":date,"windows":windows}));
        }
    }
    value["history_bars"] = json!(history);
    value["history_coverage"]["sessions"] = json!(sessions);
    value["riskgate_history"]["source_mode"] = json!("disabled-bo-only-v1");
    value["riskgate_history"]["state_generation"] = json!("disabled-v1");
    value["riskgate_history"]["session_observations"] = json!([]);
    let close = candidate_close.timestamp_millis();
    let open = close - 600_000;
    let source_m1 = (0..10)
        .map(|index| {
            let open_ts_utc_ms = open + index * 60_000;
            crate::Stage8bP1CanonicalM10SourceM1 {
                redis_id: format!("{}-0", open_ts_utc_ms + 60_000),
                semantic_id_sha256: sha256_hex(format!("short-semantic-{index}").as_bytes()),
                payload_sha256: sha256_hex(format!("short-payload-{index}").as_bytes()),
                open_ts_utc_ms,
                close_ts_utc_ms: open_ts_utc_ms + 60_000,
            }
        })
        .collect::<Vec<_>>();
    let raw = crate::build_stage8b_p1_canonical_m10(crate::Stage8bP1CanonicalM10BuildInput {
        operational_identity_sha256: identity.to_string(),
        open_ts_utc_ms: open,
        close_ts_utc_ms: close,
        open: "2200".into(),
        high: "2201".into(),
        low: "2199".into(),
        close: "2200".into(),
        volume: "20".into(),
        source_m1: source_m1.clone(),
    })
    .unwrap();
    let candidate = crate::parse_stage8b_p1_canonical_m10(&raw, identity).unwrap();
    value["candidate"]["close_time_utc"] = json!(candidate_close.timestamp());
    value["candidate"]["close_ts_utc_ms"] = json!(close);
    value["candidate"]["open_ts_utc_ms"] = json!(open);
    value["candidate"]["source_m1"] = json!(source_m1);
    value["candidate"]["redis_id"] = json!(candidate.redis_id());
    value["candidate"]["semantic_id_sha256"] = json!(candidate.semantic_id_sha256());
    value["candidate"]["payload_sha256"] = json!(candidate.payload_sha256());
    rehash(&mut value);
    (value, now)
}

fn parse(
    value: &Value,
    now: DateTime<Utc>,
) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
    let bytes = serde_json::to_vec(value).unwrap();
    parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        &sha256_hex(&bytes),
        &"1".repeat(64),
        "ACC_TEST_0001",
        now,
    )
}

#[test]
fn no_riskgate_short_history_accepts_open_intraday_and_explicit_clearing_prefix() {
    for (hour, minute, sessions) in [(4, 10, 4), (10, 0, 5), (11, 10, 5)] {
        let (value, now) = fixture(hour, minute);
        let source = parse(&value, now).unwrap();
        assert!(source.runtime_profile().no_riskgate());
        assert!(source.riskgate_observations().is_empty());
        assert_eq!(
            value["history_coverage"]["sessions"]
                .as_array()
                .unwrap()
                .len(),
            sessions
        );
    }
}

#[test]
fn no_riskgate_short_history_rejects_rehashed_missing_prefix_and_anchor() {
    let (original, now) = fixture(10, 0);
    for mutation in 0..7 {
        let mut value = original.clone();
        match mutation {
            0 => {
                value["history_bars"].as_array_mut().unwrap().remove(0);
            }
            1 => {
                value["history_coverage"]["sessions"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
                let midnight = Utc
                    .with_ymd_and_hms(2026, 9, 28, 0, 0, 0)
                    .unwrap()
                    .timestamp();
                value["history_bars"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|bar| bar["close_time_utc"].as_i64().unwrap() < midnight);
            }
            2 => {
                value["history_coverage"]
                    .as_object_mut()
                    .unwrap()
                    .remove("candidate_session");
            }
            3 => {
                value["riskgate_history"]["source_mode"] =
                    json!("source-compatible-high180-shadow-history-v1");
            }
            4 => {
                value["riskgate_history"]["session_observations"] = json!([{"session_date":"2026-09-25","shadow_pnl_points":"0.0","shadow_trade_count":0}]);
            }
            5 => {
                value["history_coverage"]["sessions"][0]["windows"][0]["first_close_time_utc"] =
                    json!(0);
            }
            _ => {
                value["history_bars"][0]["close"] = json!("NaN");
            }
        }
        rehash(&mut value);
        assert!(parse(&value, now).is_err(), "mutation {mutation}");
    }
}

#[test]
fn no_riskgate_wire_and_profile_cannot_be_crossed_with_legacy() {
    let (original, now) = fixture(10, 0);
    for legacy_piece in ["profile", "wire", "age"] {
        let mut value = original.clone();
        match legacy_piece {
            "profile" => {
                value["runtime_profile_sha256"] = json!(STAGE8B_P1E_RUNTIME_PROFILE_SHA256)
            }
            "wire" => {
                value["schema_version"] = json!(2);
                value["domain"] = json!(STAGE8B_P1E_FIRST_BOOT_SOURCE_DOMAIN);
            }
            _ => {
                value["captured_at_utc"] = json!((now + Duration::seconds(901))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
                value["broker_truth"]["checked_at_utc"] = value["captured_at_utc"].clone();
            }
        }
        let checked_now = if legacy_piece == "age" {
            now + Duration::seconds(901)
        } else {
            now
        };
        assert!(parse(&value, checked_now).is_err());
    }
}

#[test]
fn no_riskgate_source_plan_hash_is_exact() {
    assert_eq!(
        sha256_hex(include_bytes!(
            "../../../docs/stage-8/stage8b-p1e-first-boot-source-plan-v3.json"
        )),
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V3_SHA256
    );
}

#[test]
fn no_riskgate_candidate_freshness_uses_admission_clock_not_capture_clock() {
    let (mut value, now) = fixture(10, 0);
    let captured = now + Duration::seconds(820);
    value["captured_at_utc"] = json!(captured.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    value["broker_truth"]["checked_at_utc"] = value["captured_at_utc"].clone();
    assert!(parse(&value, captured).is_ok());
    let late = captured + Duration::seconds(100);
    assert!(matches!(
        parse(&value, late),
        Err(Stage8bP1eFirstBootSourceError::InvalidHistory)
    ));
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        parse_stage8b_p1e_first_boot_source_with_policy_v1(
            &bytes,
            &sha256_hex(&bytes),
            &"1".repeat(64),
            "ACC_TEST_0001",
            late,
            FirstBootTruthPolicy::HistoricalRecovery
        )
        .is_ok(),
        "authenticated recovery preserves historical capture, never re-admits freshness"
    );
}

#[test]
fn no_riskgate_complete_first_boot_export_restore_and_wrong_profile_rejection() {
    use strategy_runtime_core as core;
    let parent = tests::temp_directory("no-riskgate-source");
    let profile = Stage8bP1RuntimeProfileKind::V2;
    let (runtime, fingerprint) = profile.build_hybrid_runtime().unwrap();
    let bootstrap = crate::validate_stage8b_p1_bootstrap_config(tests::bootstrap_config(
        parent.clone(),
        fingerprint,
    ))
    .unwrap();
    let identity = bootstrap.operational_identity_sha256().to_string();
    let (value, now) = fixture_for_identity(10, 0, &identity);
    let bytes = serde_json::to_vec(&value).unwrap();
    let source = parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        &sha256_hex(&bytes),
        &identity,
        "ACC_TEST_0001",
        now,
    )
    .unwrap();
    let prepared = prepare_stage8b_p1_first_boot_source_v1(bootstrap, runtime, source).unwrap();
    let (_, ready, export, fresh, provenance) = prepared.into_parts();
    assert_eq!(
        provenance.runtime_profile_sha256(),
        profile.profile_sha256()
    );
    assert_eq!(
        provenance.source_plan_sha256(),
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V3_SHA256
    );
    assert_eq!(export.riskgate.materialized_state.ledger_rows_count, 0);
    assert_eq!(
        export
            .riskgate
            .materialized_state
            .current_shadow_session_date,
        None
    );
    let key = core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9b; 32]).unwrap();
    let package = core::export_stage5g_clean_restart(
        core::Stage5gCleanRestartSource::P1BootstrapReady(ready),
        export,
        &key,
    )
    .unwrap();
    let restored = core::restore_stage5g_clean_restart(&package, &key, fresh).unwrap();
    assert_eq!(restored.summary().stage5c_callback_count, 1);
    let (fresh_again, _) = profile.build_hybrid_runtime().unwrap();
    let again = core::restore_stage5g_clean_restart(&package, &key, fresh_again).unwrap();
    assert_eq!(
        again.reconstructed_runtime_state_fingerprint_sha256(),
        restored.reconstructed_runtime_state_fingerprint_sha256()
    );
    let (legacy, _) = Stage8bP1RuntimeProfileKind::V1
        .build_hybrid_runtime()
        .unwrap();
    assert!(core::restore_stage5g_clean_restart(&package, &key, legacy).is_err());
    assert_eq!(
        std::fs::read_dir(&parent).unwrap().count(),
        0,
        "pure prepare must not create durable root"
    );
    std::fs::remove_dir(parent).unwrap();
}

#[test]
fn no_riskgate_durable_transaction_ordinary_restart_keeps_exact_profile_and_plan() {
    let parent = tests::temp_directory("no-riskgate-durable");
    let profile = Stage8bP1RuntimeProfileKind::V2;
    let (runtime, fingerprint) = profile.build_hybrid_runtime().unwrap();
    let config = || {
        crate::validate_stage8b_p1_bootstrap_config(tests::bootstrap_config(
            parent.clone(),
            fingerprint.clone(),
        ))
        .unwrap()
    };
    let bootstrap = config();
    let identity = bootstrap.operational_identity_sha256().to_string();
    let (value, now) = fixture_for_identity(10, 0, &identity);
    let bytes = serde_json::to_vec(&value).unwrap();
    let source = parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        &sha256_hex(&bytes),
        &identity,
        "ACC_TEST_0001",
        now,
    )
    .unwrap();
    let admin = crate::authorize_stage8b_p1_first_boot(
        &bootstrap,
        crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
    )
    .unwrap();
    let prepared = prepare_stage8b_p1_first_boot_source_v1(bootstrap, runtime, source).unwrap();
    let key = strategy_runtime_core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9c; 32])
        .unwrap();
    let outcome = crate::first_boot_stage8b_p1e_transaction_v5(prepared, admin, 1, &key).unwrap();
    assert!(outcome.owner().recovery_ready());
    drop(outcome);
    for _ in 0..2 {
        let (fresh, _) = profile.build_hybrid_runtime().unwrap();
        let restarted = crate::admit_stage8b_p1e_ordinary_run_v1(config(), &key, fresh).unwrap();
        assert!(restarted.recovery_ready());
        drop(restarted);
    }
    let (legacy, legacy_fingerprint) = Stage8bP1RuntimeProfileKind::V1
        .build_hybrid_runtime()
        .unwrap();
    let wrong_config = crate::validate_stage8b_p1_bootstrap_config(tests::bootstrap_config(
        parent.clone(),
        legacy_fingerprint,
    ))
    .unwrap();
    assert!(crate::admit_stage8b_p1e_ordinary_run_v1(wrong_config, &key, legacy).is_err());
    std::fs::remove_dir_all(parent).unwrap();
}
