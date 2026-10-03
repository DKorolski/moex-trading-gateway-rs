// Offline executable linked to the exact default-feature release libraries.
// Historical raw History plus a labelled synthetic candidate; no broker calls.
use broker_finam::{
    sparse_m10::{ClosedM1RestPartV1, ClosedM1SnapshotEvidenceV1, OBSERVED_M1_POLICY_V1},
    AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
    Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteKindV1 as Kind,
};
use chrono::{DateTime, Duration, Utc};
use finam_gateway::Stage8bP1fObservedM10Plan;
use runtime_durable_service as d;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(name: &str) -> Value {
    serde_json::from_slice(&fs::read(Path::new("/src").join(name)).unwrap()).unwrap()
}
fn write(name: &str, value: &Value) {
    fs::write(
        Path::new("/proof").join(name),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn config(value: &Value) -> d::Stage8bP1eValidatedSupervisorConfigV1 {
    d::validate_stage8b_p1e_supervisor_config_v1(
        d::parse_stage8b_p1e_supervisor_config_v1(&serde_json::to_vec(value).unwrap()).unwrap(),
        [0x1e; 16],
    )
    .unwrap()
}
fn main() {
    let profile = d::Stage8bP1RuntimeProfileKind::V2;
    let fingerprint = profile.build_hybrid_runtime().unwrap().1;
    let legacy_fingerprint = d::Stage8bP1RuntimeProfileV1::build_hybrid_runtime()
        .unwrap()
        .1;
    let policy_hash = d::STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V4_SHA256;
    let mut supervisor = read("docs/stage-8/stage8b-p1f-o2-supervisor-template.json");
    supervisor["runtime_profile_id"] = json!(profile.profile_id());
    supervisor["runtime_profile_sha256"] = json!(profile.profile_sha256());
    supervisor["bootstrap"]["runtime_config_fingerprint_sha256"] = json!(fingerprint);
    let strict_identity = config(&supervisor)
        .bootstrap()
        .operational_identity_sha256()
        .to_string();
    supervisor["bootstrap"]["schema_version"] = json!(2);
    supervisor["bootstrap"]["market_data_policy_sha256"] = json!(policy_hash);
    let identity = config(&supervisor)
        .bootstrap()
        .operational_identity_sha256()
        .to_string();
    assert_ne!(strict_identity, identity);
    let mut template = read("docs/stage-8/stage8b-p1f-o2-no-riskgate-source-template.example.json");
    template["operational_identity_sha256"] = json!(identity);
    let session = |day: &str| {
        json!({"session_date":day,"windows":[{
        "first_close_time_utc": time(&format!("{day}T04:10:00Z")).timestamp(),
        "last_close_time_utc": time(&format!("{day}T20:50:00Z")).timestamp()}]})
    };
    template["history_coverage"]["sessions"] = json!([
        session("2026-09-28"),
        session("2026-09-29"),
        session("2026-09-30"),
        session("2026-10-01")
    ]);
    template["history_coverage"]["candidate_session"] = session("2026-10-02");
    let now = time("2026-10-02T04:10:03Z");
    let bytes = serde_json::to_vec(&template).unwrap();
    let plan = Stage8bP1fObservedM10Plan::from_calendar_template(&bytes, &identity, now).unwrap();
    let mut raw = read("crates/broker-finam/tests/fixtures/sparse-m10/finam-long-response.json");
    for minute in [1, 2, 3, 5, 6, 7, 8] {
        raw["bars"].as_array_mut().unwrap().push(json!({
            "timestamp":(plan.request_end()-Duration::minutes(10-minute)).to_rfc3339(),
            "open":{"value":"2280"},"high":{"value":"2281"},"low":{"value":"2279"},
            "close":{"value":"2280.5"},"volume":{"value":"1"}}));
    }
    let body = serde_json::to_string(&raw).unwrap();
    let snapshot = ClosedM1SnapshotEvidenceV1 {
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
            requested_at: now - Duration::seconds(2),
            received_at: now - Duration::seconds(1),
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(body.len() as u64),
            response_sha256: hash(body.as_bytes()),
            raw_body: body,
        }],
    };
    let material = plan.materialize(snapshot.clone(), now).unwrap();
    assert_eq!(material.history().len(), 404);
    assert_eq!(
        material
            .history()
            .iter()
            .filter(|b| b.actual_m1().len() < 10)
            .count(),
        18
    );
    let account_id = "ACC_TEST_0001";
    let account: AccountResponse = serde_json::from_value(json!({"account_id":account_id,
        "cash":[],"positions":[],"status":"ACCOUNT_STATUS_OPEN"}))
    .unwrap();
    let orders = AccountOrdersResponse { orders: vec![] };
    let params: AssetParamsResponse = serde_json::from_value(json!({"account_id":account_id,
        "symbol":"IMOEXF@RTSX","is_tradable":true}))
    .unwrap();
    let schedule: AssetScheduleResponse = serde_json::from_value(json!({"symbol":"IMOEXF@RTSX",
        "sessions":[{"interval":{"start_time":"2026-10-02T04:00:00Z",
        "end_time":"2026-10-02T20:50:00Z"},"type":"SESSION_TYPE_MAIN"}]}))
    .unwrap();
    let mut observations = [
        (Kind::Account, serde_json::to_vec(&account).unwrap()),
        (Kind::AccountOrders, serde_json::to_vec(&orders).unwrap()),
        (Kind::AssetParams, serde_json::to_vec(&params).unwrap()),
        (Kind::AssetSchedule, serde_json::to_vec(&schedule).unwrap()),
    ]
    .into_iter()
    .map(|(route, raw)| Stage8bP1fO2GetObservationV1 {
        route,
        request_sha256: "ab".repeat(32),
        response_sha256: hash(&raw),
        exact_response_bytes: raw,
    })
    .collect::<Vec<_>>();
    observations.extend(
        Stage8bP1fO2GetOnlyClientV1::new(account_id)
            .unwrap()
            .observations_for_closed_m1_snapshot(&snapshot)
            .unwrap(),
    );
    let source = material
        .first_boot_source_v4(
            &bytes,
            account_id,
            account,
            orders,
            params,
            schedule,
            observations,
            now,
        )
        .unwrap();
    let source_hash = hash(&source.exact_source_bytes);
    let admitted = d::validate_stage8b_p1e_observed_first_boot_source_v4(
        &source.exact_source_bytes,
        &source_hash,
        &identity,
        "finam-paper-primary",
        now,
    )
    .unwrap();
    assert_eq!(admitted.history_bars().len(), 404);
    assert_eq!(admitted.source_plan_sha256(), policy_hash);
    assert_eq!(admitted.runtime_profile(), profile);
    assert!(admitted.observed_receipt().is_some());
    assert_eq!(admitted.riskgate_observations().len(), 0);
    assert!(d::validate_stage8b_p1e_observed_first_boot_source_v4(
        &source.exact_source_bytes,
        &source_hash,
        &strict_identity,
        "finam-paper-primary",
        now
    )
    .is_err());
    supervisor["first_boot_source_bundle_sha256"] = json!(source_hash);
    config(&supervisor);
    write("supervisor-fixture.json", &supervisor);
    for name in [
        "schema1",
        "missing-policy",
        "foreign-policy",
        "legacy-fingerprint",
    ] {
        let mut bad = supervisor.clone();
        match name {
            "schema1" => bad["bootstrap"]["schema_version"] = json!(1),
            "missing-policy" => {
                bad["bootstrap"]
                    .as_object_mut()
                    .unwrap()
                    .remove("market_data_policy_sha256");
            }
            "foreign-policy" => {
                bad["bootstrap"]["market_data_policy_sha256"] = json!("aa".repeat(32))
            }
            _ => bad["bootstrap"]["runtime_config_fingerprint_sha256"] = json!(legacy_fingerprint),
        }
        write(&format!("mixed-{name}.json"), &bad);
    }
    let policy = json!({"schema_version":3,"domain":"stage8b-p1f-o2-materialization-policy-v3",
        "account_id_sha256":hash(account_id.as_bytes()),"account_alias":"finam-paper-primary",
        "venue_symbol":"IMOEXF@RTSX","bars_start_utc":"2026-09-28T04:00:00Z",
        "bars_end_utc":"2026-10-02T20:50:00Z","runtime_profile_id":profile.profile_id(),
        "runtime_profile_sha256":profile.profile_sha256(),"market_data_policy_sha256":policy_hash,
        "operational_identity_sha256":identity});
    write("materialization-policy-fixture.json", &policy);
    let mut bad_policy = policy.clone();
    bad_policy["market_data_policy_sha256"] = json!("aa".repeat(32));
    write("mixed-materialization-policy.json", &bad_policy);
    write("calendar-template-fixture.json", &template);
    fs::write("/proof/source-v4-fixture.json", &source.exact_source_bytes).unwrap();
    let result = json!({"materialization_policy_schema":3,"bootstrap_schema":2,"source_schema":4,
        "market_data_policy_sha256":policy_hash,"operational_identity_sha256":identity,
        "strict_operational_identity_sha256":strict_identity,"runtime_fingerprint":fingerprint,
        "runtime_profile_sha256":profile.profile_sha256(),"source_bundle_sha256":source_hash,
        "history_m10_count":404,"sparse_history_m10_count":18,"synthetic_candidate_m1_count":7,
        "strict_identity_rejected":true,"riskgate_observation_count":0,
        "fixture_only":true,"finam_contact":false,"redis_contact":false});
    write("probe-result.json", &result);
    println!("{}", serde_json::to_string(&result).unwrap());
}
