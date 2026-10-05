use super::*;
use broker_finam::sparse_m10::ClosedM1RestPartV1;
use chrono::TimeZone;
use serde_json::{json, Value};

fn time(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn template() -> Value {
    let session = |date: &str| {
        json!({
            "session_date":date,
            "windows":[{"first_close_time_utc":time(&format!("{date}T04:10:00Z")).timestamp(),
                        "last_close_time_utc":time(&format!("{date}T20:50:00Z")).timestamp()}]
        })
    };
    let mut v: Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/stage-8/stage8b-p1f-o2-no-riskgate-source-template.example.json"
    ))
    .unwrap();
    v["history_coverage"]["sessions"] = json!([
        session("2026-09-28"),
        session("2026-09-29"),
        session("2026-09-30"),
        session("2026-10-01")
    ]);
    v["history_coverage"]["candidate_session"] = session("2026-10-02");
    v
}

fn plan(
    template: &Value,
    now: DateTime<Utc>,
) -> Result<Stage8bP1fObservedM10Plan, Stage8bP1fObservedM10Error> {
    Stage8bP1fObservedM10Plan::from_calendar_template(
        &serde_json::to_vec(template).unwrap(),
        &"11".repeat(32),
        now,
    )
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../broker-finam/tests/fixtures/sparse-m10")
            .join(name),
    )
    .unwrap()
}

/// Mixed test control: all four real sessions unchanged, PLUS a clearly
/// synthetic current session. This is not a claimed FINAM Oct-2 capture.
fn mixed_evidence(
    plan: &Stage8bP1fObservedM10Plan,
    missing_candidate: &[i64],
) -> ClosedM1SnapshotEvidenceV1 {
    let raw = fixture("finam-long-response.json");
    let mut body: Value = serde_json::from_str(&raw).unwrap();
    let bars = body["bars"].as_array_mut().unwrap();
    let current_open = time("2026-10-02T04:00:00Z");
    for ts in (current_open.timestamp()..plan.request_end().timestamp()).step_by(60) {
        let open = Utc.timestamp_opt(ts, 0).unwrap();
        let in_candidate = open >= plan.request_end() - Duration::minutes(10);
        let minute = ts.rem_euclid(600) / 60;
        if in_candidate && missing_candidate.contains(&minute) {
            continue;
        }
        let price = 2280.0 + minute as f64 * 0.5;
        bars.push(json!({"timestamp":open.to_rfc3339(),
            "open":{"value":price.to_string()},"high":{"value":(price+1.0).to_string()},
            "low":{"value":(price-1.0).to_string()},"close":{"value":(price+0.5).to_string()},
            "volume":{"value":"1"}}));
    }
    let raw_body = serde_json::to_string(&body).unwrap();
    ClosedM1SnapshotEvidenceV1 {
        policy: OBSERVED_M1_POLICY_V1.into(),
        start: plan.request_start(),
        end: plan.request_end(),
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start: plan.request_start(),
            end: plan.request_end(),
            requested_at: plan.request_end() + Duration::seconds(1),
            received_at: plan.request_end() + Duration::seconds(2),
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw_body.len() as u64),
            response_sha256: sha256_hex(raw_body.as_bytes()),
            raw_body,
        }],
    }
}

#[test]
fn observed_history_uses_all_real_sparse_buckets_and_exact_calendar_candidate() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let materialized = plan
        .materialize(mixed_evidence(&plan, &[1, 2, 3]), now)
        .unwrap();
    assert_eq!(materialized.history().len(), 409); // 404 real + 5 synthetic prefix.
    assert_eq!(
        materialized.history()[..404]
            .iter()
            .filter(|b| b.actual_m1().len() < 10)
            .count(),
        18
    );
    for (line, bucket) in fixture("alor-native-m10.jsonl")
        .lines()
        .zip(materialized.history())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            bucket.bar().open_ts.timestamp(),
            v["envelope"]["payload"]["close_time_utc"].as_i64().unwrap()
        );
        assert_eq!(bucket.snapshot_sha256(), materialized.snapshot().sha256());
    }
    assert_eq!(materialized.candidate().actual_m1().len(), 7);
    assert_eq!(
        materialized.candidate().bar().close_ts,
        time("2026-10-02T05:00:00Z")
    );
    assert_eq!(
        materialized.history().last().unwrap().bar().close_ts,
        time("2026-10-02T04:50:00Z")
    );
    assert_eq!(
        materialized.candidate().snapshot_sha256(),
        materialized.snapshot().sha256()
    );
    assert_eq!(materialized.calendar_sha256(), plan.calendar_sha256());
    assert_eq!(materialized.template_sha256(), plan.template_sha256());
    assert_eq!(materialized.operational_identity_sha256(), "11".repeat(32));
}

#[test]
fn observed_calendar_missing_first_and_last_preserve_nominal_bounds_and_prices() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let materialized = plan
        .materialize(mixed_evidence(&plan, &[0, 9]), now)
        .unwrap();
    let b = materialized.candidate();
    assert_eq!(b.actual_m1().len(), 8);
    assert_eq!(b.minute_bitmap(), 0b0111111110);
    assert_eq!(b.bar().open_ts, time("2026-10-02T04:50:00Z"));
    assert_eq!(b.bar().close_ts, time("2026-10-02T05:00:00Z"));
    assert_eq!(b.bar().open.to_string(), "2280.5");
    assert_eq!(b.bar().close.to_string(), "2284.5");
}

#[test]
fn observed_calendar_empty_latest_bucket_cannot_fall_back_to_older_dense_one() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let e = mixed_evidence(&plan, &(0..10).collect::<Vec<_>>());
    assert!(matches!(
        plan.materialize(e, now),
        Err(Stage8bP1fObservedM10Error::Snapshot(
            ClosedM1SnapshotError::EmptyBucket
        ))
    ));
}

#[test]
fn observed_calendar_empty_history_bucket_cannot_be_silently_skipped() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let mut e = mixed_evidence(&plan, &[]);
    let mut body: Value = serde_json::from_str(&e.parts[0].raw_body).unwrap();
    let missing = time("2026-09-29T20:10:00Z");
    body["bars"].as_array_mut().unwrap().retain(|b| {
        let t = time(b["timestamp"].as_str().unwrap());
        t < missing || t >= missing + Duration::minutes(10)
    });
    e.parts[0].raw_body = serde_json::to_string(&body).unwrap();
    e.parts[0].response_sha256 = sha256_hex(e.parts[0].raw_body.as_bytes());
    e.parts[0].declared_body_bytes = Some(e.parts[0].raw_body.len() as u64);
    assert!(matches!(
        plan.materialize(e, now),
        Err(Stage8bP1fObservedM10Error::Snapshot(
            ClosedM1SnapshotError::EmptyBucket
        ))
    ));
}

#[test]
fn observed_calendar_end_of_session_closes_without_any_next_bar() {
    let now = time("2026-10-02T20:50:03Z");
    let plan = plan(&template(), now).unwrap();
    let result = plan.materialize(mixed_evidence(&plan, &[9]), now).unwrap();
    assert_eq!(result.history().len(), 504);
    assert_eq!(
        result.candidate().bar().close_ts,
        time("2026-10-02T20:50:00Z")
    );
    assert_eq!(result.candidate().actual_m1().len(), 9);
    assert!(result
        .snapshot()
        .bars()
        .all(|bar| bar.close_ts < plan.request_end()));
}

#[test]
fn observed_calendar_preflight_and_late_response_fail_closed() {
    let template = template();
    assert!(matches!(
        plan(&template, time("2026-10-02T04:09:59Z")),
        Err(Stage8bP1fObservedM10Error::CalendarCandidate)
    ));
    assert!(matches!(
        plan(&template, time("2026-10-02T21:05:01Z")),
        Err(Stage8bP1fObservedM10Error::CalendarCandidate)
    ));
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template, now).unwrap();
    let mut e = mixed_evidence(&plan, &[]);
    // Newly received metadata cannot make the old selected candle fresh.
    e.parts[0].requested_at += Duration::minutes(16);
    e.parts[0].received_at += Duration::minutes(16);
    assert!(matches!(
        plan.materialize(e, now + Duration::minutes(16)),
        Err(Stage8bP1fObservedM10Error::Snapshot(
            ClosedM1SnapshotError::Freshness
        ))
    ));
    assert!(Stage8bP1fObservedM10Plan::from_calendar_template(
        &serde_json::to_vec(&template).unwrap(),
        &"22".repeat(32),
        now
    )
    .is_err());
    let mut legacy = template.clone();
    legacy["runtime_profile_sha256"] = json!(Stage8bP1RuntimeProfileKind::V1.profile_sha256());
    assert!(super::tests::plan(&legacy, now).is_err());
}

#[test]
fn observed_calendar_request_plan_rejects_truncated_tail_even_with_valid_prices() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let mut e = mixed_evidence(&plan, &[]);
    e.end -= Duration::minutes(10);
    e.parts[0].end = e.end;
    assert!(matches!(
        plan.materialize(e, now),
        Err(Stage8bP1fObservedM10Error::Snapshot(
            ClosedM1SnapshotError::Range
        ))
    ));
}

#[test]
fn observed_real_history_and_sparse_candidate_reach_contextual_canonical_and_stage5c() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    let materialized = plan
        .materialize(mixed_evidence(&plan, &[0, 4, 9]), now)
        .unwrap();
    let receipt = materialized.snapshot().receipt();
    let restored = broker_core::observed_m1::ObservedM1Receipt::restore(
        receipt.retained_bytes(),
        receipt.sha256(),
    )
    .unwrap();
    for bucket in materialized.history() {
        let bytes = runtime_durable_service::build_stage8b_p1_observed_canonical_m10(
            &"11".repeat(32),
            bucket.bar().open_ts.timestamp_millis(),
            receipt,
        )
        .unwrap();
        let validated = runtime_durable_service::parse_stage8b_p1_observed_canonical_m10(
            &bytes,
            &"11".repeat(32),
            &restored,
            receipt.sha256(),
        )
        .unwrap();
        assert_eq!(
            validated.open_ts_utc_ms(),
            bucket.bar().open_ts.timestamp_millis()
        );
        assert!(
            runtime_durable_service::parse_stage8b_p1_canonical_m10(&bytes, &"11".repeat(32))
                .is_err()
        );
    }
    let candidate = materialized.candidate_canonical_bytes().unwrap();
    let accepted = materialized
        .validate_candidate_canonical(&candidate)
        .unwrap();
    let stage5c = accepted.into_stage5c_semantic_bar().unwrap();
    assert_eq!(
        stage5c.strategy_model_bar_label_utc(),
        materialized.candidate().bar().open_ts.timestamp()
    );
    let previous = runtime_durable_service::build_stage8b_p1_observed_canonical_m10(
        &"11".repeat(32),
        time("2026-10-02T04:40:00Z").timestamp_millis(),
        receipt,
    )
    .unwrap();
    assert!(materialized
        .validate_candidate_canonical(&previous)
        .is_err());
    // Explicit policy/receipt reference, not the former density assertion.
    let value: Value = serde_json::from_slice(&candidate).unwrap();
    assert_eq!(value["schema_version"], 2);
    assert_eq!(value["payload"]["policy"], OBSERVED_M1_POLICY_V1);
    assert_eq!(value["payload"]["source_m1"].as_array().unwrap().len(), 7);
    assert_eq!(value["payload"]["source_receipt_sha256"], receipt.sha256());
}

fn source_v4(
    materialized: &Stage8bP1fObservedM10Materialization,
    template: &Value,
    now: DateTime<Utc>,
    mutation: usize,
) -> Result<
    super::super::Stage8bP1fO2MaterializedSourceV1,
    super::super::Stage8bP1fO2MaterializerErrorV1,
> {
    use broker_finam::{
        AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
        Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetRouteKindV1 as Kind,
    };
    let mut account: AccountResponse = serde_json::from_value(json!({
        "account_id":"ACC_TEST_0001", "cash":[], "positions":[], "status":"ACCOUNT_STATUS_OPEN"
    }))
    .unwrap();
    if mutation == 1 {
        account.account_id = "OTHER".into();
    }
    let orders = AccountOrdersResponse { orders: Vec::new() };
    let params: AssetParamsResponse = serde_json::from_value(json!({
        "account_id":"ACC_TEST_0001", "symbol":"IMOEXF@RTSX", "is_tradable":true
    }))
    .unwrap();
    let schedule: AssetScheduleResponse = serde_json::from_value(json!({
        "symbol":"IMOEXF@RTSX", "sessions":[{"interval":{"start_time":"2026-10-02T04:00:00Z", "end_time":"2026-10-02T20:50:00Z"},"type":"SESSION_TYPE_MAIN"}]
    })).unwrap();
    let mut observations = [
        (Kind::Account, serde_json::to_vec(&account).unwrap()),
        (Kind::AccountOrders, serde_json::to_vec(&orders).unwrap()),
        (Kind::AssetParams, serde_json::to_vec(&params).unwrap()),
        (Kind::AssetSchedule, serde_json::to_vec(&schedule).unwrap()),
    ]
    .into_iter()
    .map(
        |(route, exact_response_bytes)| Stage8bP1fO2GetObservationV1 {
            route,
            request_sha256: "cd".repeat(32),
            response_sha256: sha256_hex(&exact_response_bytes),
            exact_response_bytes,
        },
    )
    .collect::<Vec<_>>();
    for mut observation in broker_finam::Stage8bP1fO2GetOnlyClientV1::new("ACC_TEST_0001")
        .unwrap()
        .observations_for_closed_m1_snapshot(materialized.snapshot().evidence())
        .unwrap()
    {
        if mutation == 2 {
            observation.exact_response_bytes.push(b' ');
            observation.response_sha256 = sha256_hex(&observation.exact_response_bytes);
        }
        if mutation == 4 {
            observation.request_sha256 = "ff".repeat(32);
        }
        observations.push(observation);
    }
    if mutation == 3 {
        observations.remove(0);
    }
    materialized.first_boot_source_v4(
        &serde_json::to_vec(template).unwrap(),
        "ACC_TEST_0001",
        account,
        orders,
        params,
        schedule,
        observations,
        now,
    )
}

#[test]
fn observed_v4_assembly_preserves_subsecond_receipt_chronology() {
    let template = template();
    for (received, captured) in [
        ("2026-10-02T05:00:03Z", "2026-10-02T05:00:03Z"),
        ("2026-10-02T05:00:02.900Z", "2026-10-02T05:00:03Z"),
        ("2026-10-02T05:00:03.100Z", "2026-10-02T05:00:03.200Z"),
        (
            "2026-10-02T05:00:03.123456788Z",
            "2026-10-02T05:00:03.123456789Z",
        ),
        (
            "2026-10-02T05:00:03.123456789Z",
            "2026-10-02T05:00:03.123456789Z",
        ),
    ] {
        let now = time(captured);
        let plan = plan(&template, now).unwrap();
        let mut raw = mixed_evidence(&plan, &[0, 4, 9]);
        raw.parts[0].received_at = time(received);
        let materialized = plan.materialize(raw, now).unwrap();
        let receipt = materialized.snapshot().receipt();
        let canonical_before = materialized.candidate_canonical_bytes().unwrap();
        let output = source_v4(&materialized, &template, now, 0)
            .unwrap_or_else(|error| panic!("received={received}, captured={captured}: {error:?}"));
        let source: Value = serde_json::from_slice(&output.exact_source_bytes).unwrap();
        assert_eq!(source["captured_at_utc"], captured);
        assert_eq!(source["broker_truth"]["checked_at_utc"], captured);
        assert_eq!(output.evidence.captured_at_utc, captured);
        assert_eq!(receipt.received_at(), time(received));
        assert_eq!(
            source["observed_source"]["receipt_sha256"],
            receipt.sha256()
        );
        assert_eq!(
            source["observed_source"]["receipt"]
                .as_str()
                .unwrap()
                .as_bytes(),
            receipt.retained_bytes()
        );
        assert_eq!(
            materialized.candidate_canonical_bytes().unwrap(),
            canonical_before
        );
        materialized
            .validate_retained_source(
                &serde_json::to_vec(&template).unwrap(),
                &output.exact_source_bytes,
                &output.evidence,
                "ACC_TEST_0001",
                now,
            )
            .unwrap();
    }
}

#[test]
fn observed_v4_parser_failure_keeps_typed_safe_diagnostic_and_collection_progress() {
    let template = template();
    let now = time("2026-10-02T05:00:03.200Z");
    let plan = plan(&template, now).unwrap();
    let mut raw = mixed_evidence(&plan, &[0, 4, 9]);
    raw.parts[0].received_at = time("2026-10-02T05:00:03.100Z");
    let mut materialized = plan.materialize(raw, now).unwrap();
    let output = source_v4(&materialized, &template, now, 0).unwrap();
    // Replay the former floor-to-seconds defect into a retained source. The
    // real parser, not a mocked error, must reject it specifically as history.
    let mut source: Value = serde_json::from_slice(&output.exact_source_bytes).unwrap();
    source["captured_at_utc"] = json!("2026-10-02T05:00:03Z");
    source["broker_truth"]["checked_at_utc"] = source["captured_at_utc"].clone();
    let replay_error = materialized
        .validate_retained_source(
            &serde_json::to_vec(&template).unwrap(),
            &serde_json::to_vec(&source).unwrap(),
            &output.evidence,
            "ACC_TEST_0001",
            now,
        )
        .expect_err("receipt later than truncated capture must be rejected");
    // Also exercise the assembly's parser-error projection, not just replay.
    materialized.history.pop();
    let assembly_error = source_v4(&materialized, &template, now, 0)
        .expect_err("missing history cannot be assembled");
    for error in [replay_error, assembly_error] {
        let progress = super::super::CollectionProgress {
            gets: 5,
            chunks: 1,
            last_collected: Some("collection_complete"),
            chunk: Some((
                0,
                plan.request_start().timestamp(),
                plan.request_end().timestamp(),
            )),
            ..Default::default()
        };
        let sensitive = "DO_NOT_ECHO_account_token_body";
        let diagnostic = progress
            .error(error)
            .diagnostic_json(sensitive, sensitive, sensitive, sensitive, sensitive, now);
        assert!(diagnostic.len() <= 4096);
        assert!(!diagnostic.contains(sensitive));
        assert!(!diagnostic.contains("ACC_TEST_0001"));
        let record: Value = serde_json::from_str(&diagnostic).unwrap();
        assert_eq!(record["failure"]["stage"], "canonical_validation");
        assert_eq!(record["failure"]["reason_code"], "source_invalid_history");
        assert_eq!(
            record["failure"]["last_validated_stage"],
            "collection_complete"
        );
        assert_eq!(record["failure"]["completed_typed_gets"], 5);
        assert_eq!(record["failure"]["completed_chunks"], 1);
        assert_eq!(
            record["failure"]["chunk_end_utc"],
            plan.request_end().timestamp()
        );
    }
}

#[test]
fn observed_real_history_wire_v4_first_boot_warmup_and_restore_use_same_receipt() {
    use runtime_durable_service as durable;
    use strategy_runtime_core as core;
    let template = template();
    let now = time("2026-10-02T05:00:03.123456789Z");
    let plan = plan(&template, now).unwrap();
    let mut raw = mixed_evidence(&plan, &[0, 4, 9]);
    raw.parts[0].received_at = now - Duration::nanoseconds(1);
    let materialized = plan.materialize(raw, now).unwrap();
    let output = source_v4(&materialized, &template, now, 0).unwrap();
    let bytes = &output.exact_source_bytes;
    assert_eq!(output.evidence.schema_version, 2);
    assert_eq!(output.evidence.selected_m1_count, 7);
    assert!(durable::validate_stage8b_p1e_first_boot_source_bytes_v1(
        bytes,
        &"11".repeat(32),
        "finam-paper-primary",
        now
    )
    .is_err());
    let admitted = durable::validate_stage8b_p1e_observed_first_boot_source_v4(
        bytes,
        &output.evidence.source_bundle_sha256,
        &"11".repeat(32),
        "finam-paper-primary",
        now,
    )
    .unwrap();
    assert_eq!(admitted.captured_at(), now);
    assert_eq!(admitted.broker_truth_checked_at(), now);
    assert_eq!(
        admitted.observed_receipt().unwrap().sha256(),
        materialized.snapshot().receipt().sha256()
    );
    assert_eq!(admitted.history_bars().len(), 409);
    assert_eq!(
        admitted.candidate_semantic_id_sha256(),
        output.evidence.candidate_semantic_id_sha256
    );
    let to_core = |b: &durable::Stage8bP1eFirstBootBarV1| core::Stage8bP1eFirstBootBarInputV1 {
        close_time_utc: b.close_time_utc,
        open: b.open_text.clone(),
        high: b.high_text.clone(),
        low: b.low_text.clone(),
        close: b.close_text.clone(),
        volume: b.volume_text.clone(),
    };
    let receipt = admitted.observed_receipt().unwrap();
    let runtime = || {
        durable::Stage8bP1RuntimeProfileKind::V2
            .build_hybrid_runtime()
            .unwrap()
            .0
    };
    let composition = core::build_stage8b_p1_observed_first_boot_composition(
        core::Stage8bP1eFirstBootCompositionInputV1 {
            runtime: runtime(),
            fresh_runtime: runtime(),
            account_id: broker_core::BrokerAccountId::new("finam-paper-primary"),
            operational_identity_sha256: "11".repeat(32),
            captured_at: now,
            broker_truth_checked_at: now,
            history_bars_sha256: admitted.history_bars_sha256().into(),
            riskgate_session_observations_sha256: admitted
                .riskgate_session_observations_sha256()
                .into(),
            validated_candidate_semantic_id_sha256: admitted.candidate_semantic_id_sha256().into(),
            history_bars: admitted.history_bars().iter().map(to_core).collect(),
            riskgate_observations: Vec::new(),
            candidate: to_core(admitted.candidate()),
        },
        receipt,
        materialized.snapshot().receipt().sha256(),
    )
    .unwrap();
    let (ready, export, fresh) = composition.into_parts();
    assert_eq!(export.riskgate.materialized_state.ledger_rows_count, 0);
    let key = core::Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0x9b; 32]).unwrap();
    let saved = core::export_stage5g_clean_restart(
        core::Stage5gCleanRestartSource::P1BootstrapReady(ready),
        export,
        &key,
    )
    .unwrap();
    let restored = core::restore_stage5g_clean_restart(&saved, &key, fresh).unwrap();
    assert_eq!(restored.summary().stage5c_callback_count, 1);
    assert_eq!(
        restored.stage8b_p1_model_last_bar_label_utc(),
        Some("2026-10-02T04:50:00.000000Z".into())
    );
    let restored_again = core::restore_stage5g_clean_restart(&saved, &key, runtime()).unwrap();
    assert_eq!(
        restored.reconstructed_runtime_state_fingerprint_sha256(),
        restored_again.reconstructed_runtime_state_fingerprint_sha256()
    );
    for mutation in 1..=4 {
        assert!(
            source_v4(&materialized, &template, now, mutation).is_err(),
            "mutation {mutation}"
        );
    }
    let mut other_template = template.clone();
    other_template["source_bundle_generation"] = json!(2);
    assert!(source_v4(&materialized, &other_template, now, 0).is_err());
    assert!(
        source_v4(&materialized, &template, now + Duration::minutes(10), 0).is_err(),
        "no reuse for later calendar candidate"
    );
}

#[tokio::test]
async fn observed_collection_bad_template_stops_before_client_or_get() {
    let now = time("2026-10-02T05:00:03Z");
    let plan = plan(&template(), now).unwrap();
    // Invalid template fails before constructing a client or examining these
    // deliberately invalid credentials. No network mock/fallback is involved.
    let result = plan
        .collect_first_boot_source("", &broker_finam::AccessToken::new(String::new()), b"{}")
        .await;
    assert!(matches!(
        result,
        Err(super::super::Stage8bP1fO2MaterializerErrorV1::Diagnostic(_))
    ));
}
