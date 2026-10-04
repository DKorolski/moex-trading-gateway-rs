use super::*;
use chrono::TimeZone;
use serde_json::json;

pub(crate) fn prepare_fixture(
    bootstrap: Stage8bP1ValidatedBootstrapConfig,
    runtime: strategy_runtime_core::HybridIntradayRuntimeStrategy,
    source: Stage8bP1eValidatedFirstBootSourceV1,
) -> Stage8bP1ePreparedFirstBootV1 {
    prepare_stage8b_p1_first_boot_source_v1(bootstrap, runtime, source).unwrap()
}

pub(crate) fn fixture(identity: &str) -> (Value, DateTime<Utc>) {
    // Synthetic control only. Real raw-response/History linkage is exercised
    // by the gateway fixture tests, not claimed by these generated minutes.
    let (mut value, now) = super::super::no_riskgate_tests::fixture_for_identity(10, 0, identity);
    let instrument = crate::stage8b_p1_semantic::p1_instrument();
    let mut bars = Vec::new();
    for bar in value["history_bars"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&value["candidate"]))
    {
        let close = bar["close_time_utc"].as_i64().unwrap();
        // Missing first and last minute of every bucket.
        for minute in 1..9 {
            let open_ts = Utc.timestamp_opt(close - 600 + minute * 60, 0).unwrap();
            bars.push(broker_core::event::Bar {
                instrument: instrument.clone(),
                timeframe_sec: 60,
                open_ts,
                close_ts: open_ts + Duration::seconds(60),
                open: Decimal::from(2200),
                high: Decimal::from(2201),
                low: Decimal::from(2199),
                close: Decimal::from(2200),
                volume: Decimal::ONE,
                is_final: true,
                source_kind: broker_core::MarketDataSourceKind::HistoricalPoll,
            });
        }
    }
    let start = Utc
        .timestamp_opt(
            value["history_bars"][0]["close_time_utc"].as_i64().unwrap() - 600,
            0,
        )
        .unwrap();
    let end = Utc
        .timestamp_opt(value["candidate"]["close_time_utc"].as_i64().unwrap(), 0)
        .unwrap();
    let receipt =
        ObservedM1Receipt::from_admitted_source("ab".repeat(32), start, end, now, instrument, bars)
            .unwrap();
    value["schema_version"] = json!(4);
    value["domain"] = json!(STAGE8B_P1E_FIRST_BOOT_SOURCE_V4_DOMAIN);
    value["history_provenance"]["source_mode"] = json!(OBSERVED_M1_POLICY_V1);
    value["history_provenance"]["gap_absence_proven"] = json!(false);
    for bar in value["history_bars"].as_array_mut().unwrap() {
        bar["volume"] = json!("8");
    }
    value["candidate"]["volume"] = json!("8");
    let canonical = crate::build_stage8b_p1_observed_canonical_m10(
        identity,
        end.timestamp_millis() - 600_000,
        &receipt,
    )
    .unwrap();
    let canonical: Value = serde_json::from_slice(&canonical).unwrap();
    value["candidate"]["source_m1"] = canonical["payload"]["source_m1"].clone();
    value["candidate"]["semantic_id_sha256"] = canonical["m10_semantic_id_sha256"].clone();
    value["candidate"]["payload_sha256"] = canonical["m10_payload_sha256"].clone();
    value["riskgate_history"]["history_bars_sha256"] =
        json!(canonical_value_sha256(&value["history_bars"]));
    value["observed_source"] = json!({
        "policy":OBSERVED_M1_POLICY_V1, "source_plan_sha256":STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256,
        "receipt_sha256":receipt.sha256(), "receipt":std::str::from_utf8(receipt.retained_bytes()).unwrap()
    });
    (value, now)
}

fn parse(
    value: &Value,
    now: DateTime<Utc>,
) -> Result<Stage8bP1eValidatedFirstBootSourceV1, Stage8bP1eFirstBootSourceError> {
    let bytes = serde_json::to_vec(value).unwrap();
    validate_stage8b_p1e_observed_first_boot_source_v4(
        &bytes,
        &sha256_hex(&bytes),
        &"1".repeat(64),
        "ACC_TEST_0001",
        now,
    )
}

#[test]
fn observed_v4_source_is_explicit_and_legacy_parser_stays_strict() {
    assert_eq!(
        sha256_hex(STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4.as_bytes()),
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
    );
    let (value, now) = fixture(&"1".repeat(64));
    let source = parse(&value, now).unwrap();
    assert_eq!(
        source.source_plan_sha256(),
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
    );
    assert!(source.observed_receipt().is_some());
    assert_eq!(source.candidate().volume, Decimal::from(8));
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        &sha256_hex(&bytes),
        &"1".repeat(64),
        "ACC_TEST_0001",
        now
    )
    .is_err());
    let (mut legacy, _) =
        super::super::no_riskgate_tests::fixture_for_identity(10, 0, &"1".repeat(64));
    assert!(parse(&legacy, now).is_err());
    legacy["observed_source"] = Value::Null;
    let bytes = serde_json::to_vec(&legacy).unwrap();
    assert!(parse_stage8b_p1e_first_boot_source_v1(
        &bytes,
        &sha256_hex(&bytes),
        &"1".repeat(64),
        "ACC_TEST_0001",
        now
    )
    .is_err());
}

#[test]
fn observed_v4_rehashed_invalid_projections_do_not_gain_source_admission() {
    let (original, now) = fixture(&"1".repeat(64));
    for mutation in 0..14 {
        let mut v = original.clone();
        match mutation {
            0 => v["history_provenance"]["gap_absence_proven"] = json!(true),
            1 => v["history_provenance"]["source_mode"] = json!("finam_derived_m1_to_m10"),
            2 => v["history_bars"][0]["volume"] = json!("7"),
            3 => v["candidate"]["close"] = json!("2200.5"),
            4 => {
                v["candidate"]["source_m1"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            5 => v["observed_source"]["policy"] = json!("other"),
            6 => {
                v["observed_source"]["source_plan_sha256"] =
                    json!(STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V3_SHA256)
            }
            7 => v["observed_source"]["receipt_sha256"] = json!("cd".repeat(32)),
            8 => {
                v["history_bars"].as_array_mut().unwrap().remove(0);
            }
            9 => v["schema_version"] = json!(3),
            10 => {
                v["runtime_profile_sha256"] =
                    json!(Stage8bP1RuntimeProfileKind::V1.profile_sha256())
            }
            11 => v["history_provenance"]["aggregation_complete"] = json!(false),
            12 => v["candidate"]["payload_sha256"] = json!("de".repeat(32)),
            _ => v["observed_source"] = Value::Null,
        }
        v["riskgate_history"]["history_bars_sha256"] =
            json!(canonical_value_sha256(&v["history_bars"]));
        assert!(parse(&v, now).is_err(), "mutation {mutation}");
    }
    let old_bytes = serde_json::to_vec(&original).unwrap();
    let mut tampered = original.clone();
    tampered["observed_source"]["receipt"] = json!("{}");
    let bytes = serde_json::to_vec(&tampered).unwrap();
    assert!(matches!(
        validate_stage8b_p1e_observed_first_boot_source_v4(
            &bytes,
            &sha256_hex(&old_bytes),
            &"1".repeat(64),
            "ACC_TEST_0001",
            now
        ),
        Err(Stage8bP1eFirstBootSourceError::SourceHashMismatch)
    ));
    assert!(parse(&original, now + Duration::seconds(901)).is_err());
}

#[test]
fn observed_v4_first_boot_export_restore_retains_sparse_provenance_without_effects() {
    use strategy_runtime_core as core;
    let parent = super::super::tests::temp_directory("observed-v4");
    let profile = Stage8bP1RuntimeProfileKind::V2;
    let (runtime, fingerprint) = profile.build_hybrid_runtime().unwrap();
    let bootstrap = observed_bootstrap(&parent, fingerprint);
    let identity = bootstrap.operational_identity_sha256().to_string();
    let (value, now) = fixture(&identity);
    let bytes = serde_json::to_vec(&value).unwrap();
    let source = validate_stage8b_p1e_observed_first_boot_source_v4(
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
        provenance.source_plan_sha256(),
        STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
    );
    assert_eq!(export.riskgate.materialized_state.ledger_rows_count, 0);
    let key = core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9b; 32]).unwrap();
    let package = core::export_stage5g_clean_restart(
        core::Stage5gCleanRestartSource::P1BootstrapReady(ready),
        export,
        &key,
    )
    .unwrap();
    let restored = core::restore_stage5g_clean_restart(&package, &key, fresh).unwrap();
    assert_eq!(restored.summary().stage5c_callback_count, 1);
    let (fresh, _) = profile.build_hybrid_runtime().unwrap();
    let again = core::restore_stage5g_clean_restart(&package, &key, fresh).unwrap();
    assert_eq!(
        restored.reconstructed_runtime_state_fingerprint_sha256(),
        again.reconstructed_runtime_state_fingerprint_sha256()
    );
    assert_eq!(
        std::fs::read_dir(&parent).unwrap().count(),
        0,
        "pure prepare creates no durable root"
    );
    std::fs::remove_dir(parent).unwrap();
}

pub(crate) fn observed_bootstrap(
    parent: &Path,
    fingerprint: String,
) -> Stage8bP1ValidatedBootstrapConfig {
    let mut config = super::super::tests::bootstrap_config(parent.to_path_buf(), fingerprint);
    config.schema_version = 2;
    config.market_data_policy_sha256 = Some(STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256.into());
    crate::validate_stage8b_p1_bootstrap_config(config).unwrap()
}

#[test]
fn observed_policy_changes_identity_root_and_rejects_legacy_or_wrong_profile() {
    let parent = super::super::tests::temp_directory("observed-identity");
    let (_, fingerprint) = Stage8bP1RuntimeProfileKind::V2
        .build_hybrid_runtime()
        .unwrap();
    let legacy_config =
        || super::super::tests::bootstrap_config(parent.clone(), fingerprint.clone());
    let legacy = crate::validate_stage8b_p1_bootstrap_config(legacy_config()).unwrap();
    let observed = observed_bootstrap(&parent, fingerprint.clone());
    let encoded = serde_json::to_value(legacy.operational_identity()).unwrap();
    assert!(encoded.get("market_data_policy_sha256").is_none());
    let decoded: strategy_runtime_core::Stage6dOperationalIdentityConfig =
        serde_json::from_value(encoded).unwrap();
    assert_eq!(&decoded, legacy.operational_identity());
    assert_ne!(
        legacy.operational_identity_sha256(),
        observed.operational_identity_sha256()
    );
    assert_ne!(legacy.expected_root_name(), observed.expected_root_name());
    for case in 0..5 {
        let mut c = legacy_config();
        c.schema_version = 2;
        c.market_data_policy_sha256 = Some(STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256.into());
        match case {
            0 => c.schema_version = 1,
            1 => c.market_data_policy_sha256 = None,
            2 => c.market_data_policy_sha256 = Some("ab".repeat(32)),
            3 => c.schema_version = 3,
            _ => {
                c.runtime_config_fingerprint_sha256 = Stage8bP1RuntimeProfileKind::V1
                    .build_hybrid_runtime()
                    .unwrap()
                    .1
            }
        }
        assert!(
            crate::validate_stage8b_p1_bootstrap_config(c).is_err(),
            "case {case}"
        );
    }
    let (value, now) = fixture(legacy.operational_identity_sha256());
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(parse_bootstrap_bound_source(
        &bytes,
        &sha256_hex(&bytes),
        &legacy,
        now,
        FirstBootTruthPolicy::FreshAdmission
    )
    .is_err());
    assert!(parse_bootstrap_bound_source(
        &bytes,
        &sha256_hex(&bytes),
        &observed,
        now,
        FirstBootTruthPolicy::FreshAdmission
    )
    .is_err());
    assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 0);
    std::fs::remove_dir(parent).unwrap();
}

#[test]
fn observed_v4_transaction_seal_restart_and_marker_bound_historical_recovery() {
    use crate::stage8b_p1e_first_boot_transaction as transaction;
    use crate::Stage8bP1eFirstBootClassificationV5 as Classification;
    use crate::Stage8bP1ePreSealRecoveryActionV5 as Action;
    use strategy_runtime_core as core;
    // Reuse the accepted transaction crash frontier: no new crash framework.
    for crash in [false, true] {
        let parent = super::super::tests::temp_directory("observed-transaction");
        let (runtime, fp) = Stage8bP1RuntimeProfileKind::V2
            .build_hybrid_runtime()
            .unwrap();
        let config = || observed_bootstrap(&parent, fp.clone());
        let bootstrap = config();
        let op = bootstrap.operational_identity_sha256().to_string();
        let (value, now) = fixture(&op);
        let bytes = serde_json::to_vec(&value).unwrap();
        let bundle_hash = sha256_hex(&bytes);
        let source = parse_bootstrap_bound_source(
            &bytes,
            &bundle_hash,
            &bootstrap,
            now,
            FirstBootTruthPolicy::FreshAdmission,
        )
        .unwrap();
        let receipt_hash = source.observed_receipt().unwrap().sha256().to_string();
        let candidate_hash = source.candidate_canonical_m10_sha256().to_string();
        let admin = crate::authorize_stage8b_p1_first_boot(
            &bootstrap,
            crate::STAGE8B_P1_FIRST_BOOT_CONFIRMATION,
        )
        .unwrap();
        let prepared =
            prepare_stage8b_p1_first_boot_source_v1(bootstrap, runtime.clone(), source).unwrap();
        let key = core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9b; 32]).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            transaction::test_first_boot_stage8b_p1e_transaction_v5_with_observer(
                prepared,
                admin,
                1,
                &key,
                |phase| {
                    if crash && phase == "after-prepared-marker-temp-sync-before-rename" {
                        panic!("controlled interrupted Prepared write");
                    }
                },
            )
        }));
        if crash {
            assert!(result.is_err());
            std::fs::rename(
                parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_TEMP_FILE),
                parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE),
            )
            .unwrap();
            std::fs::File::open(&parent).unwrap().sync_all().unwrap();
            let inspection =
                crate::classify_stage8b_p1e_first_boot_v5(config(), &key, runtime.clone());
            assert_eq!(
                inspection.classification,
                Classification::PreparedWithoutRoot
            );
            let selector = || {
                crate::authorize_stage8b_p1e_pre_seal_recovery_v5(
                    &config(),
                    inspection.transaction_id_sha256.as_ref().unwrap(),
                    Action::ResumePrepared,
                    crate::STAGE8B_P1E_FIRST_BOOT_RECOVERY_CONFIRMATION,
                )
                .unwrap()
            };
            let historical =
                transaction::historical_source_binding_v5(&config(), &selector(), &key).unwrap();
            assert_eq!(
                historical.source_plan_sha256,
                STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
            );
            assert_eq!(historical.source_bundle_sha256, bundle_hash);
            let later = now + Duration::days(30);
            assert!(parse_bootstrap_bound_source(
                &bytes,
                &bundle_hash,
                &config(),
                later,
                FirstBootTruthPolicy::FreshAdmission
            )
            .is_err());
            // Exact receipt survives long downtime; this is recovery, never a
            // fresh admission of historical broker truth.
            let recovered_source = parse_bootstrap_bound_source(
                &bytes,
                &historical.source_bundle_sha256,
                &config(),
                later,
                FirstBootTruthPolicy::HistoricalRecovery,
            )
            .unwrap();
            assert!(historical_binding_matches_source(
                &historical,
                &recovered_source
            ));
            assert_eq!(
                recovered_source.observed_receipt().unwrap().sha256(),
                receipt_hash
            );
            assert_eq!(
                recovered_source.candidate_canonical_m10_sha256(),
                candidate_hash
            );
            let mut corrupted = value.clone();
            corrupted["observed_source"]["receipt_sha256"] = json!("dd".repeat(32));
            assert!(
                transaction::test_recover_stage8b_p1e_first_boot_pre_seal_historical_from_bytes_v5(
                    config(),
                    runtime.clone(),
                    &serde_json::to_vec(&corrupted).unwrap(),
                    later,
                    selector(),
                    &key
                )
                .is_err()
            );
            let outcome =
                transaction::test_recover_stage8b_p1e_first_boot_pre_seal_historical_from_bytes_v5(
                    config(),
                    runtime.clone(),
                    &bytes,
                    later,
                    selector(),
                    &key,
                )
                .unwrap();
            let crate::Stage8bP1ePreSealRecoveryOutcomeV5::Adopted(outcome) = outcome else {
                panic!("must adopt")
            };
            assert!(outcome.owner().recovery_ready());
            drop(outcome);
        } else {
            drop(result.unwrap().unwrap());
        }
        let marker: Value = serde_json::from_slice(
            &std::fs::read(parent.join(crate::STAGE8B_P1E_TRANSACTION_MARKER_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            marker["source_plan_sha256"],
            STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256
        );
        assert_eq!(marker["source_bundle_sha256"], bundle_hash);
        let inspection = crate::classify_stage8b_p1e_first_boot_v5(config(), &key, runtime.clone());
        assert_eq!(
            inspection.classification,
            Classification::AdoptedCommittedRoot
        );
        let restart = crate::restart_stage8b_p1(config(), &key, runtime).unwrap();
        let source_binding = restart
            .restore_stage8b_p1_observed_source_binding(
                &config(),
                &bytes,
                &bundle_hash,
                &key,
                now + Duration::days(30),
            )
            .unwrap();
        assert_eq!(source_binding.source().sha256(), receipt_hash);
        let canonical = crate::build_stage8b_p1_observed_canonical_m10(
            &op,
            value["candidate"]["open_ts_utc_ms"].as_i64().unwrap(),
            source_binding.source(),
        )
        .unwrap();
        assert_eq!(sha256_hex(&canonical), candidate_hash);
        source_binding.parse_exact(&canonical, &op).unwrap();
        assert!(restart
            .restore_stage8b_p1_observed_source_binding(
                &config(),
                &bytes,
                &"bb".repeat(32),
                &key,
                now
            )
            .is_err());
        let wrong_key =
            core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9c; 32]).unwrap();
        assert!(restart
            .restore_stage8b_p1_observed_source_binding(
                &config(),
                &bytes,
                &bundle_hash,
                &wrong_key,
                now
            )
            .is_err());
        let mut changed = value.clone();
        changed["observed_source"]["receipt_sha256"] = json!("aa".repeat(32));
        let changed = serde_json::to_vec(&changed).unwrap();
        assert!(restart
            .restore_stage8b_p1_observed_source_binding(
                &config(),
                &changed,
                &sha256_hex(&changed),
                &key,
                now
            )
            .is_err());
        let crate::Stage7bRestartOutcome::Ready(owner) = restart else {
            panic!("no effects on restart")
        };
        assert!(owner
            .recovered()
            .unwrap()
            .stage8b_p1e_initial_adoption_ready());
        drop(owner);
        std::fs::remove_dir_all(parent).unwrap();
    }
}
