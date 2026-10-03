//! Real four-session History plus explicitly synthetic current-session controls.
//! No credentials, network FINAM call, fixed path or operational activation.
use super::*;
use crate::stage8b_p1f_fixed_producers::observed::*;
use crate::stage8b_p1f_o2_materializer::observed::Stage8bP1fObservedM10Plan;
use broker_finam::{
    sparse_m10::{
        AdmittedClosedM1Snapshot, ClosedM1RequestPlan, ClosedM1RestPartV1,
        ClosedM1SnapshotEvidenceV1,
    },
    AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
    Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteKindV1 as Kind,
};
use runtime_durable_service as d;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;

fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn append_control(raw: &mut Value, close: DateTime<Utc>, price: i64) {
    for minute in [1, 2, 3, 5, 6, 7, 8] {
        raw["bars"].as_array_mut().unwrap().push(json!({
            "timestamp":(close-Duration::minutes(10-minute)).to_rfc3339(),
            "open":{"value":price.to_string()},"high":{"value":(price+1).to_string()},
            "low":{"value":(price-1).to_string()},"close":{"value":price.to_string()},
            "volume":{"value":"1"}
        }));
    }
}

fn evidence(
    raw: &Value,
    end: DateTime<Utc>,
    received: DateTime<Utc>,
) -> ClosedM1SnapshotEvidenceV1 {
    let start = time("2026-09-28T04:00:00Z");
    let raw_body = serde_json::to_string(raw).unwrap();
    ClosedM1SnapshotEvidenceV1 {
        policy: broker_finam::sparse_m10::OBSERVED_M1_POLICY_V1.into(),
        start,
        end,
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start,
            end,
            requested_at: received - Duration::seconds(1),
            received_at: received,
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw_body.len() as u64),
            response_sha256: sha256_hex(raw_body.as_bytes()),
            raw_body,
        }],
    }
}

fn admit(raw: &Value, close: DateTime<Utc>) -> AdmittedClosedM1Snapshot {
    let e = evidence(raw, close, close + Duration::seconds(1));
    let plan = ClosedM1RequestPlan::new(e.start, close, vec![close]).unwrap();
    AdmittedClosedM1Snapshot::admit(e, &plan, close + Duration::seconds(1)).unwrap()
}

async fn schedule(
    redis: &RedisServer,
    signer: &FixtureSigner,
    identity: &str,
    fingerprint: &str,
    now: DateTime<Utc>,
    path: &Path,
    previous: Option<&Stage8bP1eSchedulePublisherStateV1>,
) -> Stage8bP1eSchedulePublisherStateV1 {
    let input = fixture_input_for_binding(
        now,
        "2026-10-02T04:00:00.000000Z",
        "2026-10-02T20:50:00.000000Z",
        "2026-10-02",
        FixtureBindingV1 {
            operational_identity_sha256: identity.into(),
            runtime_config_fingerprint_sha256: fingerprint.into(),
            instrument_map_fingerprint_sha256:
                d::stage8b_p1_imoexf_instrument_map_fingerprint_sha256(),
            registry_identity_sha256: "6".repeat(64),
        },
    );
    let phase_id = "o3-real-history-synthetic-continuation".to_string();
    let hash = stage8b_p1f_synthetic_schedule_fixture_sha256(&phase_id, &input).unwrap();
    let lineage = match previous {
        Some(s) => Stage8bP1eSchedulePublisherLineage::Resume(s),
        None => Stage8bP1eSchedulePublisherLineage::First(
            authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                .unwrap(),
        ),
    };
    let prepared = prepare_stage8b_p1f_o3_synthetic_schedule_with(
        Stage8bP1fO3ScheduleInputV1 {
            phase_id,
            expected_synthetic_fixture_sha256: hash,
            publisher_input: input,
            trusted_now_utc: now,
        },
        lineage,
        signer,
        fixture_prepare,
    )
    .unwrap();
    let mut writer =
        Stage8bP1fSchedulePublisherRedisV1::connect_synthetic_local_evidence(&redis.url)
            .await
            .unwrap();
    test_publish_stage8b_p1e_prepared_schedule_with_key(
        path,
        prepared,
        &mut writer,
        signer.public_key_ed25519_hex(),
    )
    .await
    .unwrap()
}

fn prepared(
    source: &AdmittedClosedM1Snapshot,
    identity: &str,
    schedule: &Stage8bP1eSchedulePublisherStateV1,
    signer: &FixtureSigner,
    previous: Option<&Stage8bP1fObservedM10State>,
) -> Result<Stage8bP1fObservedM10State, Stage8bP1fProducerErrorV1> {
    let lineage = match previous {
        Some(p) => Stage8bP1fObservedM10Lineage::Resume(p),
        None => Stage8bP1fObservedM10Lineage::First(
            authorize_stage8b_p1f_first_m10(FIRST_M10_CONFIRMATION).unwrap(),
        ),
    };
    let result = crate::stage8b_p1f_fixed_producers::observed::prepare_with_verifier(
        Stage8bP1fObservedM10Input {
            phase: Stage8bP1fProducerPhaseV1::O3Synthetic,
            phase_id: "o3-real-history-synthetic-continuation".into(),
            nominal_close: source.receipt().end(),
            trusted_now: source.receipt().received_at(),
            snapshot: source,
            expected_receipt_sha256: source.receipt().sha256(),
            expected_synthetic_snapshot_sha256: Some(source.sha256()),
        },
        identity,
        schedule,
        lineage,
        &|s, op, now| {
            s.test_verify_fresh_envelope_with_key(op, now, signer.public_key_ed25519_hex())
        },
    )?;
    match result {
        Stage8bP1fObservedM10Outcome::Prepared(s) => Ok(s),
        _ => panic!("new publication expected"),
    }
}

#[tokio::test]
async fn observed_real_history_fixed_producer_paper_truth_xack_restart() {
    let redis = RedisServer::start().await;
    d::initialize_stage8b_p1_redis_namespace(
        &redis.url,
        d::Stage8bP1RedisConfig::paper_default_auto(),
    )
    .await
    .unwrap();
    let mut db =
        redis::aio::ConnectionManager::new(redis::Client::open(redis.url.as_str()).unwrap())
            .await
            .unwrap();
    let stream = strategy_runtime_core::STAGE8B_P1E_SCHEDULE_STREAM;
    let seed: String = redis::cmd("XADD")
        .arg(stream)
        .arg("*")
        .arg("payload")
        .arg("provision")
        .query_async(&mut db)
        .await
        .unwrap();
    let _: u64 = redis::cmd("XDEL")
        .arg(stream)
        .arg(seed)
        .query_async(&mut db)
        .await
        .unwrap();
    let producer_path = state_path("observed-real-linked");
    let parent = producer_path.parent().unwrap().canonicalize().unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
    let profile = d::Stage8bP1RuntimeProfileKind::V2;
    let fingerprint = profile.build_hybrid_runtime().unwrap().1;
    let mut config: Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/stage-8/stage8b-p1f-o2-supervisor-template.json"
    ))
    .unwrap();
    config["runtime_profile_id"] = json!(profile.profile_id());
    config["runtime_profile_sha256"] = json!(profile.profile_sha256());
    config["bootstrap"]["runtime_config_fingerprint_sha256"] = json!(fingerprint);
    config["bootstrap"]["durable_parent"] = json!(parent);
    config["bootstrap"]["schema_version"] = json!(2);
    config["bootstrap"]["market_data_policy_sha256"] =
        json!(d::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256);
    let validated = d::validate_stage8b_p1e_supervisor_config_v1(
        d::parse_stage8b_p1e_supervisor_config_v1(&serde_json::to_vec(&config).unwrap()).unwrap(),
        [0x1e; 16],
    )
    .unwrap();
    let identity = validated
        .bootstrap()
        .operational_identity_sha256()
        .to_string();

    let mut template: Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/stage-8/stage8b-p1f-o2-no-riskgate-source-template.example.json"
    ))
    .unwrap();
    template["operational_identity_sha256"] = json!(identity);
    let session = |day: &str| {
        json!({"session_date":day,"windows":[{
            "first_close_time_utc":time(&format!("{day}T04:10:00Z")).timestamp(),
            "last_close_time_utc":time(&format!("{day}T20:50:00Z")).timestamp()
        }]})
    };
    template["history_coverage"]["sessions"] = json!([
        session("2026-09-28"),
        session("2026-09-29"),
        session("2026-09-30"),
        session("2026-10-01")
    ]);
    template["history_coverage"]["candidate_session"] = session("2026-10-02");
    let template_bytes = serde_json::to_vec(&template).unwrap();
    let candidate_close = time("2026-10-02T09:50:00Z");
    let decision_close = candidate_close + Duration::minutes(10);
    let successor_close = decision_close + Duration::minutes(10);
    let now = decision_close - Duration::seconds(29);
    let plan =
        Stage8bP1fObservedM10Plan::from_calendar_template(&template_bytes, &identity, now).unwrap();
    let mut raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../broker-finam/tests/fixtures/sparse-m10/finam-long-response.json"
    ))
    .unwrap();
    let mut close = time("2026-10-02T04:10:00Z");
    while close <= candidate_close {
        append_control(&mut raw, close, 2280);
        close += Duration::minutes(10);
    }
    let snapshot = evidence(&raw, candidate_close, now);
    let material = plan.materialize(snapshot.clone(), now).unwrap();
    assert_eq!(
        material
            .history()
            .iter()
            .filter(|bar| bar.bar().close_ts < time("2026-10-02T00:00:00Z"))
            .count(),
        404
    );
    assert_eq!(
        material
            .history()
            .iter()
            .filter(|bar| bar.bar().close_ts < time("2026-10-02T00:00:00Z")
                && bar.actual_m1().len() < 10)
            .count(),
        18
    );
    let account: AccountResponse = serde_json::from_value(json!({"account_id":"ACC_TEST_0001","cash":[],"positions":[],"status":"ACCOUNT_STATUS_OPEN"})).unwrap();
    let orders = AccountOrdersResponse { orders: vec![] };
    let params: AssetParamsResponse = serde_json::from_value(
        json!({"account_id":"ACC_TEST_0001","symbol":"IMOEXF@RTSX","is_tradable":true}),
    )
    .unwrap();
    let asset_schedule: AssetScheduleResponse = serde_json::from_value(json!({"symbol":"IMOEXF@RTSX","sessions":[{"interval":{"start_time":"2026-10-02T04:00:00Z","end_time":"2026-10-02T20:50:00Z"},"type":"SESSION_TYPE_MAIN"}]})).unwrap();
    let mut routes = [
        (Kind::Account, serde_json::to_vec(&account).unwrap()),
        (Kind::AccountOrders, serde_json::to_vec(&orders).unwrap()),
        (Kind::AssetParams, serde_json::to_vec(&params).unwrap()),
        (
            Kind::AssetSchedule,
            serde_json::to_vec(&asset_schedule).unwrap(),
        ),
    ]
    .into_iter()
    .map(|(route, bytes)| Stage8bP1fO2GetObservationV1 {
        route,
        request_sha256: "ab".repeat(32),
        response_sha256: sha256_hex(&bytes),
        exact_response_bytes: bytes,
    })
    .collect::<Vec<_>>();
    routes.extend(
        Stage8bP1fO2GetOnlyClientV1::new("ACC_TEST_0001")
            .unwrap()
            .observations_for_closed_m1_snapshot(&snapshot)
            .unwrap(),
    );
    let source = material
        .first_boot_source_v4(
            &template_bytes,
            "ACC_TEST_0001",
            account,
            orders,
            params,
            asset_schedule,
            routes,
            now,
        )
        .unwrap();
    config["first_boot_source_bundle_sha256"] = json!(sha256_hex(&source.exact_source_bytes));
    let provision_config =
        d::parse_stage8b_p1e_supervisor_config_v1(&serde_json::to_vec(&config).unwrap()).unwrap();
    config["redis_deployment_manifest_sha256"] = json!(
        d::stage8b_p1e_test_provision_production_redis_v1(provision_config, [0x1e; 16], &redis.url)
            .await
    );
    let config_bytes = serde_json::to_vec(&config).unwrap();
    let config_path = parent.join("supervisor.json");
    let source_path = parent.join("source.json");
    for (path, bytes) in [
        (&config_path, &config_bytes),
        (&source_path, &source.exact_source_bytes),
    ] {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    let signer = FixtureSigner::new(117);
    let schedule_path = parent.join("schedule.json");
    let first_schedule = schedule(
        &redis,
        &signer,
        &identity,
        &fingerprint,
        decision_close + Duration::seconds(1),
        &schedule_path,
        None,
    )
    .await;
    append_control(&mut raw, decision_close, 2310);
    let decision_source = admit(&raw, decision_close);
    let decision = prepared(&decision_source, &identity, &first_schedule, &signer, None).unwrap();
    let mut feeder = d::Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        d::Stage8bP1RedisConfig::paper_default_auto(),
        decision.binding().clone(),
        d::Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    )
    .await
    .unwrap();
    let decision = publish_stage8b_p1f_observed_m10(&producer_path, decision, &mut feeder)
        .await
        .unwrap();
    let final_schedule = schedule(
        &redis,
        &signer,
        &identity,
        &fingerprint,
        successor_close,
        &schedule_path,
        Some(&first_schedule),
    )
    .await;
    append_control(&mut raw, successor_close, 2311);
    let successor_source = admit(&raw, successor_close);
    // Independently decodable/rehashed responses cannot revise an admitted
    // overlap. No producer state, receipt file or Redis entry may be advanced.
    let before = fs::read(&producer_path).unwrap();
    let namespace = d::stage8b_p1_redis_namespace();
    let mut overlap_negatives = Vec::new();
    for case in [
        "changed_volume",
        "changed_prices",
        "lost_minute",
        "added_minute",
    ] {
        let mut changed = raw.clone();
        match case {
            "changed_volume" => changed["bars"][0]["volume"]["value"] = json!("999999"),
            "changed_prices" => {
                for field in ["open", "high", "low", "close"] {
                    changed["bars"][0][field]["value"] = json!("2300");
                }
            }
            "lost_minute" => {
                changed["bars"].as_array_mut().unwrap().remove(0);
            }
            "added_minute" => {
                // Synthetic current-session minute 0 was deliberately absent.
                let mut extra = changed["bars"].as_array().unwrap().last().unwrap().clone();
                extra["timestamp"] = json!("2026-10-02T04:00:00Z");
                changed["bars"].as_array_mut().unwrap().push(extra);
            }
            _ => unreachable!(),
        }
        let changed_source = admit(&changed, successor_close);
        let files_before = fs::read_dir(&parent).unwrap().count();
        assert!(
            prepared(
                &changed_source,
                &identity,
                &final_schedule,
                &signer,
                Some(&decision)
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(fs::read(&producer_path).unwrap(), before, "{case}");
        assert_eq!(
            fs::read_dir(&parent).unwrap().count(),
            files_before,
            "{case}"
        );
        let length: u64 = redis::cmd("XLEN")
            .arg(&namespace.canonical_m10_stream)
            .query_async(&mut db)
            .await
            .unwrap();
        assert_eq!(length, 1, "{case}");
        let pending: redis::streams::StreamPendingReply = redis::cmd("XPENDING")
            .arg(&namespace.canonical_m10_stream)
            .arg(&namespace.m10_consumer_group)
            .query_async(&mut db)
            .await
            .unwrap();
        assert_eq!(pending.count(), 0, "{case}");
        overlap_negatives.push(case);
    }
    let successor = prepared(
        &successor_source,
        &identity,
        &final_schedule,
        &signer,
        Some(&decision),
    )
    .unwrap();
    assert!(observed_published_window(&decision, &successor).is_err());
    let mut feeder = d::Stage8bP1fM10FeederRedisV1::connect_observed_local_evidence(
        &redis.url,
        d::Stage8bP1RedisConfig::paper_default_auto(),
        successor.binding().clone(),
        d::Stage8bP1fRedisRoleV1::SyntheticM10Feeder,
    )
    .await
    .unwrap();
    let successor = publish_stage8b_p1f_observed_m10(&producer_path, successor, &mut feeder)
        .await
        .unwrap();
    let published_window =
        crate::stage8b_p1f_observed_published_window(&decision, &successor).unwrap();
    assert!(observed_published_window(&successor, &decision).is_err());
    assert!(observed_published_window(&decision, &decision).is_err());
    // Keep separately admitted hashes, discard live producer handles and
    // restore both receipts from disk, including the predecessor which the
    // Published high-water no longer describes. Redis hashes grant nothing.
    let decision_id = decision.redis_id().to_string();
    let successor_id = successor.redis_id().to_string();
    let decision_hash = decision_source.receipt().sha256().to_string();
    let successor_hash = successor_source.receipt().sha256().to_string();
    drop((
        decision,
        successor,
        decision_source,
        successor_source,
        feeder,
    ));
    let restored = |hash: &str| {
        d::Stage8bP1ObservedM10Binding::restore_retained_receipt(&parent, &identity, hash).unwrap()
    };
    let successor =
        Stage8bP1fObservedM10State::load(&producer_path, restored(&successor_hash)).unwrap();
    assert_eq!(successor.publication_sequence(), 2);
    assert_eq!(successor.redis_id(), successor_id);
    assert_eq!(restored(&decision_hash).source().sha256(), decision_hash);
    let input = d::Stage8bP1fIeCompositionInputV1 {
        supervisor_path: config_path,
        source_path,
        expected_config_sha256: sha256_hex(&config_bytes),
        expected_source_sha256: sha256_hex(&source.exact_source_bytes),
        expected_operational_identity_sha256: identity,
        expected_runtime_config_fingerprint_sha256: fingerprint,
        redis_url: redis.url.clone(),
        decision_redis_id: decision_id,
        successor_redis_id: successor_id,
        successor_close_ts_utc_ms: successor_close.timestamp_millis(),
        schedule_redis_id: final_schedule.redis_stream_id().unwrap().into(),
        schedule_public_key_hex: signer.public_key_ed25519_hex().into(),
        schedule_key_valid_from_ms: time("2026-01-01T00:00:00Z").timestamp_millis(),
        schedule_key_valid_until_ms: time("2027-01-01T00:00:00Z").timestamp_millis(),
        schedule_registry_version: "imoexf-v1".into(),
        schedule_registry_identity_sha256: "6".repeat(64),
    };
    let evidence = d::stage8b_p1f_ie_run_observed_composition_v4(input, published_window)
        .await
        .unwrap();
    assert!(
        evidence.durable_truth_committed
            && evidence.source_xack_last
            && evidence.duplicate_command_absent
            && evidence.readmission_already_acknowledged
    );
    assert_eq!(evidence.final_m10_pel_count, 0);
    println!(
        "SPARSE_LINKED_EVIDENCE={}",
        json!({
            "schema_version": 1,
            "real_history_m10_count": 404,
            "real_sparse_history_m10_count": 18,
            "current_session_controls": "synthetic seven-M1 bars, not broker observations",
            "observed_policy": broker_finam::sparse_m10::OBSERVED_M1_POLICY_V1,
            "overlap_negatives": overlap_negatives,
            "producer_sequence_after_disk_restore": successor.publication_sequence(),
            "independently_admitted_receipt_hashes": [decision_hash, successor_hash],
            "composition": evidence
        })
    );
    fs::remove_dir_all(parent).unwrap();
}
