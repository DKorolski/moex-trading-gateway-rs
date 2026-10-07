//! No credentials, network or fixed-path writes: exercise the actual CLI
//! selection/replay functions with real History and a synthetic candidate.
use super::*;
use broker_finam::{
    AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
    Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteKindV1 as Kind,
};
use serde_json::{json, Value};

fn fixture() -> (
    StagedMaterializationV1,
    MaterializationPolicyV1,
    DateTime<Utc>,
) {
    fixture_for_identity(None)
}

fn fixture_for_identity(
    identity: Option<&str>,
) -> (
    StagedMaterializationV1,
    MaterializationPolicyV1,
    DateTime<Utc>,
) {
    // Real clocks retain fractions: exercise the complete fixed CLI and
    // staged-consumer paths with same-second receipt/assembly, not only secs.
    let now = canonical_time("2026-10-02T04:10:03Z").unwrap()
        + chrono::Duration::nanoseconds(123_456_789);
    let mut template = tests::short_template();
    if let Some(identity) = identity {
        template["operational_identity_sha256"] = json!(identity);
    }
    for session in template["history_coverage"]["sessions"]
        .as_array_mut()
        .unwrap()
    {
        let date = session["session_date"]
            .as_str()
            .unwrap()
            .parse::<chrono::NaiveDate>()
            .unwrap()
            + chrono::Duration::days(6);
        session["session_date"] = json!(date.to_string());
        for window in session["windows"].as_array_mut().unwrap() {
            for field in ["first_close_time_utc", "last_close_time_utc"] {
                window[field] = json!(window[field].as_i64().unwrap() + 6 * 86400);
            }
        }
    }
    let mut current = template["history_coverage"]["sessions"][3].clone();
    current["session_date"] = json!("2026-10-02");
    for window in current["windows"].as_array_mut().unwrap() {
        for field in ["first_close_time_utc", "last_close_time_utc"] {
            window[field] = json!(window[field].as_i64().unwrap() + 86400);
        }
    }
    template["history_coverage"]["candidate_session"] = current;
    let template_json = serde_json::to_string(&template).unwrap();
    let mut policy = tests::short_policy();
    policy.schema_version = 3;
    policy.domain = "stage8b-p1f-o2-materialization-policy-v3".into();
    policy.market_data_policy_sha256 =
        Some(runtime_durable_service::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256.into());
    policy.operational_identity_sha256 = Some(
        template["operational_identity_sha256"]
            .as_str()
            .unwrap()
            .into(),
    );
    policy.bars_start_utc = "2026-09-28T04:00:00Z".into();
    policy.bars_end_utc = "2026-10-02T20:50:00Z".into();
    let plan = observed::plan(&policy, template_json.as_bytes(), now).unwrap();
    let mut raw: Value = serde_json::from_slice(include_bytes!(
        "../../../../broker-finam/tests/fixtures/sparse-m10/finam-long-response.json"
    ))
    .unwrap();
    for minute in [1, 2, 3, 5, 6, 7, 8] {
        let time = plan.request_end() - chrono::Duration::minutes(10 - minute);
        raw["bars"]
            .as_array_mut()
            .unwrap()
            .push(json!({"timestamp":time.to_rfc3339(),
            "open":{"value":"2280"},"high":{"value":"2281"},"low":{"value":"2279"},
            "close":{"value":"2280.5"},"volume":{"value":"1"}}));
    }
    let raw_body = serde_json::to_string(&raw).unwrap();
    let snapshot = broker_finam::sparse_m10::ClosedM1SnapshotEvidenceV1 {
        policy: broker_finam::sparse_m10::OBSERVED_M1_POLICY_V1.into(),
        start: plan.request_start(),
        end: plan.request_end(),
        parts: vec![broker_finam::sparse_m10::ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start: plan.request_start(),
            end: plan.request_end(),
            requested_at: now - chrono::Duration::seconds(2),
            received_at: now - chrono::Duration::nanoseconds(1),
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw_body.len() as u64),
            response_sha256: sha256_hex(raw_body.as_bytes()),
            raw_body,
        }],
    };
    let materialized = plan.materialize(snapshot.clone(), now).unwrap();
    assert_eq!(materialized.history().len(), 404);
    assert_eq!(
        materialized
            .history()
            .iter()
            .filter(|b| b.actual_m1().len() < 10)
            .count(),
        18
    );
    let account: AccountResponse = serde_json::from_value(json!({"account_id":tests::ACCOUNT,
        "cash":[],"positions":[],"status":"ACCOUNT_STATUS_OPEN"}))
    .unwrap();
    let orders = AccountOrdersResponse { orders: vec![] };
    let params: AssetParamsResponse = serde_json::from_value(json!({"account_id":tests::ACCOUNT,
        "symbol":"IMOEXF@RTSX","is_tradable":true}))
    .unwrap();
    let schedule: AssetScheduleResponse = serde_json::from_value(json!({"symbol":"IMOEXF@RTSX",
        "sessions":[{"interval":{"start_time":"2026-10-02T04:00:00Z","end_time":"2026-10-02T20:50:00Z"},"type":"SESSION_TYPE_MAIN"}]})).unwrap();
    let mut observations = [
        (Kind::Account, serde_json::to_vec(&account).unwrap()),
        (Kind::AccountOrders, serde_json::to_vec(&orders).unwrap()),
        (Kind::AssetParams, serde_json::to_vec(&params).unwrap()),
        (Kind::AssetSchedule, serde_json::to_vec(&schedule).unwrap()),
    ]
    .into_iter()
    .map(|(route, bytes)| Stage8bP1fO2GetObservationV1 {
        route,
        request_sha256: "ab".repeat(32),
        response_sha256: sha256_hex(&bytes),
        exact_response_bytes: bytes,
    })
    .collect::<Vec<_>>();
    observations.extend(
        Stage8bP1fO2GetOnlyClientV1::new(tests::ACCOUNT)
            .unwrap()
            .observations_for_closed_m1_snapshot(&snapshot)
            .unwrap(),
    );
    let source = materialized
        .first_boot_source_v4(
            template_json.as_bytes(),
            tests::ACCOUNT,
            account,
            orders,
            params,
            schedule,
            observations,
            now,
        )
        .unwrap();
    (
        StagedMaterializationV1 {
            schema_version: 2,
            domain: "stage8b-p1f-o2-staged-materialization-v2".into(),
            manifest_sha256: "aa".repeat(32),
            source_bundle_sha256: sha256_hex(&source.exact_source_bytes),
            exact_source_json: String::from_utf8(source.exact_source_bytes).unwrap(),
            evidence: source.evidence,
            observed_input: Some(observed::RetainedObservedInput {
                template_json,
                snapshot,
            }),
        },
        policy,
        now,
    )
}

#[test]
fn observed_staged_consumer_accepts_real_history_only_with_explicit_protected_config() {
    use runtime_durable_service as d;
    let parent =
        std::env::temp_dir().join(format!("observed-staged-consumer-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
    let parent = parent.canonicalize().unwrap();
    let mut supervisor: Value = serde_json::from_slice(include_bytes!(
        "../../../../../docs/stage-8/stage8b-p1f-o2-supervisor-template.json"
    ))
    .unwrap();
    let profile = d::Stage8bP1RuntimeProfileKind::V2;
    supervisor["runtime_profile_id"] = json!(profile.profile_id());
    supervisor["runtime_profile_sha256"] = json!(profile.profile_sha256());
    supervisor["bootstrap"]["runtime_config_fingerprint_sha256"] =
        json!(profile.build_hybrid_runtime().unwrap().1);
    supervisor["bootstrap"]["durable_parent"] = json!(parent);
    supervisor["bootstrap"]["schema_version"] = json!(2);
    supervisor["bootstrap"]["market_data_policy_sha256"] =
        json!(d::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256);
    let validated = d::validate_stage8b_p1e_supervisor_config_v1(
        d::parse_stage8b_p1e_supervisor_config_v1(&serde_json::to_vec(&supervisor).unwrap())
            .unwrap(),
        [0; 16],
    )
    .unwrap();
    let (staged, policy, now) =
        fixture_for_identity(Some(validated.bootstrap().operational_identity_sha256()));
    let package = serde_json::to_value(&staged).unwrap();
    let policy = serde_json::to_value(&policy).unwrap();
    let template = staged
        .observed_input
        .as_ref()
        .unwrap()
        .template_json
        .as_bytes();
    let check = |package: &Value, policy: &Value, supervisor: &Value, time| {
        d::check_stage8b_p1f_staged_source(
            &serde_json::to_vec(package).unwrap(),
            &staged.manifest_sha256,
            &serde_json::to_vec(policy).unwrap(),
            &serde_json::to_vec(supervisor).unwrap(),
            template,
            time,
        )
    };
    let checked = check(&package, &policy, &supervisor, now).unwrap();
    assert_eq!(checked.source_bytes(), staged.exact_source_json.as_bytes());
    let exact: Value = serde_json::from_slice(checked.source_bytes()).unwrap();
    assert_eq!(exact["captured_at_utc"], "2026-10-02T04:10:03.123456789Z");
    assert_eq!(
        exact["broker_truth"]["checked_at_utc"],
        exact["captured_at_utc"]
    );
    // The separate guardian receipt remains a conservative seconds-only
    // projection; it must not rewrite the exact source or its hash.
    assert_eq!(
        checked.broker_truth_checked_at_utc(),
        "2026-10-02T04:10:03Z"
    );
    for fault in 0..13 {
        let mut p = package.clone();
        let mut config = supervisor.clone();
        let mut policy = policy.clone();
        match fault {
            0 => p["schema_version"] = json!(1),
            1 => p["domain"] = json!("stage8b-p1f-o2-staged-materialization-v1"),
            2 => p["manifest_sha256"] = json!("bb".repeat(32)),
            3 => p["source_bundle_sha256"] = json!("bb".repeat(32)),
            4 => p["observed_input"] = Value::Null,
            5 => p["observed_input"]["template_json"] = json!("{}"),
            6 => p["evidence"]["candidate_semantic_id_sha256"] = json!("bb".repeat(32)),
            7 => p["evidence"]["selected_m1_count"] = json!(10),
            8 => policy["market_data_policy_sha256"] = json!("bb".repeat(32)),
            9 => policy["schema_version"] = json!(2),
            10 => {
                config["bootstrap"]["schema_version"] = json!(1);
                config["bootstrap"]
                    .as_object_mut()
                    .unwrap()
                    .remove("market_data_policy_sha256");
            }
            11 => config["first_boot_source_bundle_sha256"] = json!("aa".repeat(32)),
            _ => p["unapproved_field"] = json!(true),
        }
        assert!(
            check(&p, &policy, &config, now).is_err(),
            "mutation {fault}"
        );
    }
    assert!(check(
        &package,
        &policy,
        &supervisor,
        now + chrono::Duration::minutes(20)
    )
    .is_err());
    // Rehashing valid source bytes must not replace the configured generation.
    let mut changed = package.clone();
    let mut source: Value = serde_json::from_str(&staged.exact_source_json).unwrap();
    source["source_bundle_generation"] = json!(2);
    let changed_json = serde_json::to_string(&source).unwrap();
    changed["source_bundle_sha256"] = json!(sha256_hex(changed_json.as_bytes()));
    changed["evidence"]["source_bundle_sha256"] = changed["source_bundle_sha256"].clone();
    changed["exact_source_json"] = json!(changed_json);
    assert_eq!(
        check(&changed, &policy, &supervisor, now).err(),
        Some("source/template identity mismatch")
    );
    // A retained template copied from the package is not independent authority.
    // If the protected template changes, the unchanged source must match it too.
    for field in ["candidate_session", "sessions"] {
        let mut calendar: Value = serde_json::from_slice(template).unwrap();
        let windows = if field == "sessions" {
            &mut calendar["history_coverage"][field][0]["windows"]
        } else {
            &mut calendar["history_coverage"][field]["windows"]
        };
        windows[0]["first_close_time_utc"] =
            json!(windows[0]["first_close_time_utc"].as_i64().unwrap() + 600);
        let new_template = serde_json::to_string(&calendar).unwrap();
        let mut changed = package.clone();
        changed["observed_input"]["template_json"] = json!(new_template);
        assert!(d::check_stage8b_p1f_staged_source(
            &serde_json::to_vec(&changed).unwrap(),
            &staged.manifest_sha256,
            &serde_json::to_vec(&policy).unwrap(),
            &serde_json::to_vec(&supervisor).unwrap(),
            new_template.as_bytes(),
            now
        )
        .is_err());
    }
    // Legacy output remains forbidden under the explicit observed config.
    assert!(check(
        &serde_json::to_value(tests::fixture().0).unwrap(),
        &policy,
        &supervisor,
        now
    )
    .is_err());
    fs::remove_dir(parent).unwrap();
}

fn check(
    staged: &StagedMaterializationV1,
    policy: &MaterializationPolicyV1,
    now: DateTime<Utc>,
) -> Result<MaterializerResultV1, String> {
    validate_retained_bytes(
        &serde_json::to_vec(staged).unwrap(),
        &staged.manifest_sha256,
        policy,
        tests::ACCOUNT,
        now,
    )
}

#[test]
fn observed_fixed_materializer_real_history_roundtrip_preserves_exact_receipt_and_bytes() {
    let (staged, policy, now) = fixture();
    let bytes = serde_json::to_vec(&staged).unwrap();
    for _ in 0..2 {
        let result = check(&staged, &policy, now).unwrap();
        assert_eq!(result.staged_package_sha256, sha256_hex(&bytes));
        assert_eq!(result.disposition, "EXACT_RETAINED_REPLAY");
    }
    assert_eq!(staged.evidence.selected_m1_count, 7);
    assert!(check(&staged, &policy, now + chrono::Duration::minutes(20)).is_err());
    assert!(check(&staged, &tests::short_policy(), now).is_err());
    assert!(check(&tests::fixture().0, &policy, now).is_err());
}

#[test]
fn observed_fixed_materializer_preflight_rejects_wrong_policy_identity_calendar_and_range() {
    let (staged, mut policy, now) = fixture();
    let template = staged
        .observed_input
        .as_ref()
        .unwrap()
        .template_json
        .as_bytes();
    for start in ["2026-09-28T04:01:00Z", "2026-09-20T04:00:00Z"] {
        policy.bars_start_utc = start.into();
        assert!(observed::plan(&policy, template, now).is_err());
    }
    policy.bars_start_utc = "2026-09-28T04:00:00Z".into();
    policy.bars_end_utc = "2026-10-02T04:09:00Z".into();
    assert!(observed::plan(&policy, template, now).is_err());
    policy.bars_end_utc = "2026-10-02T20:50:00Z".into();
    policy.operational_identity_sha256 = Some("ff".repeat(32));
    assert!(observed::plan(&policy, template, now).is_err());
    policy.market_data_policy_sha256 = Some("ff".repeat(32));
    assert!(validate_policy(&policy).is_err());
    policy.schema_version = 2;
    policy.domain = "stage8b-p1f-o2-materialization-policy-v2".into();
    assert!(validate_policy(&policy).is_err());
    for field in ["operational_identity_sha256", "market_data_policy_sha256"] {
        let mut legacy: Value = serde_json::from_slice(include_bytes!(
            "../../../../../docs/stage-8/stage8b-p1f-o2-materialization-policy.json"
        ))
        .unwrap();
        legacy[field] = Value::Null;
        assert!(serde_json::from_value::<MaterializationPolicyV1>(legacy).is_err());
    }
}

#[test]
fn observed_fixed_materializer_retained_mutations_cannot_rehash_raw_or_source_independently() {
    let (staged, policy, now) = fixture();
    let pristine = serde_json::to_value(&staged).unwrap();
    for mutation in 0..12 {
        let mut value = pristine.clone();
        match mutation {
            0 => value["observed_input"] = Value::Null,
            1 => {
                value.as_object_mut().unwrap().remove("observed_input");
            }
            2 => {
                value["observed_input"]["snapshot"]["parts"][0]["transport_complete"] = json!(false)
            }
            3 => value["evidence"]["selected_m1_count"] = json!(10),
            4 => value["evidence"]["route_evidence"][4]["request_sha256"] = json!("ff".repeat(32)),
            5 => value["schema_version"] = json!(1),
            6 => value["observed_input"]["snapshot"]["end"] = json!("2026-10-02T04:00:00Z"),
            7 => {
                let part = &mut value["observed_input"]["snapshot"]["parts"][0];
                let mut raw: Value =
                    serde_json::from_str(part["raw_body"].as_str().unwrap()).unwrap();
                raw["bars"].as_array_mut().unwrap().pop();
                let raw = serde_json::to_string(&raw).unwrap();
                part["raw_body"] = json!(raw);
                part["response_sha256"] = json!(sha256_hex(raw.as_bytes()));
                part["declared_body_bytes"] = json!(raw.len());
            }
            8..=10 => {
                let mut source: Value =
                    serde_json::from_str(value["exact_source_json"].as_str().unwrap()).unwrap();
                match mutation {
                    8 => source["source_bundle_generation"] = json!(2),
                    9 => source["instrument_map_fingerprint_sha256"] = json!("cc".repeat(32)),
                    _ => source["operational_identity_sha256"] = json!("cc".repeat(32)),
                }
                let source = serde_json::to_string(&source).unwrap();
                let hash = sha256_hex(source.as_bytes());
                value["exact_source_json"] = json!(source);
                value["source_bundle_sha256"] = json!(hash);
                value["evidence"]["source_bundle_sha256"] = json!(hash);
            }
            _ => value["evidence"]["route_evidence"][4]["response_sha256"] = json!("ee".repeat(32)),
        }
        assert!(
            validate_retained_bytes(
                &serde_json::to_vec(&value).unwrap(),
                &staged.manifest_sha256,
                &policy,
                tests::ACCOUNT,
                now
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
}
