//! Disposable LOCAL fixture INPUT generation only. The actual bootstrap and
//! runtime are separately compiled production executables. No runtime callback,
//! root constructor, fixture signer, historical clock or injected intent here.
//! Operator-authorized 2026-10-09; NEVER a replacement for the accepted O2 seed.
use super::*;
use broker_finam::{
    AccountOrdersResponse, AccountResponse, AssetParamsResponse, AssetScheduleResponse,
    Stage8bP1fO2GetObservationV1, Stage8bP1fO2GetOnlyClientV1, Stage8bP1fO2GetRouteKindV1 as Kind,
};
use chrono::{Datelike, NaiveDate, Weekday};

fn session(day: NaiveDate) -> Value {
    json!({"session_date":day,"windows":[{
        "first_close_time_utc":day.and_hms_opt(4,10,0).unwrap().and_utc().timestamp(),
        "last_close_time_utc":day.and_hms_opt(20,50,0).unwrap().and_utc().timestamp()
    }]})
}

fn append(raw: &mut Value, close: DateTime<Utc>, price: i64) {
    for minute in [1, 2, 3, 5, 6, 7, 8] {
        raw["bars"].as_array_mut().unwrap().push(json!({
            "timestamp":stamp(close-Duration::minutes(10-minute)),
            "open":{"value":price.to_string()},"high":{"value":(price+1).to_string()},
            "low":{"value":(price-1).to_string()},"close":{"value":price.to_string()},
            "volume":{"value":"1"}
        }));
    }
}

fn evidence(
    raw: &Value,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    observed: DateTime<Utc>,
) -> ClosedM1SnapshotEvidenceV1 {
    let raw_body = serde_json::to_string(raw).unwrap();
    ClosedM1SnapshotEvidenceV1 {
        policy: OBSERVED_M1_POLICY_V1.into(),
        start,
        end,
        parts: vec![ClosedM1RestPartV1 {
            method: "GET".into(),
            endpoint: "https://api.finam.ru/v1/instruments/IMOEXF@RTSX/bars".into(),
            symbol: "IMOEXF@RTSX".into(),
            timeframe: "TIME_FRAME_M1".into(),
            start,
            end,
            requested_at: observed,
            received_at: observed,
            status: 200,
            transport_complete: true,
            declared_body_bytes: Some(raw_body.len() as u64),
            response_sha256: hash(raw_body.as_bytes()),
            raw_body,
        }],
    }
}

pub(super) fn inputs(
    config: &mut Value,
    identity: &str,
    now: DateTime<Utc>,
) -> (O3ObservedInput, O3ObservedInput) {
    let today = (now + Duration::hours(3)).date_naive();
    assert!(
        !matches!(today.weekday(), Weekday::Sat | Weekday::Sun),
        "weekday-only LOCAL synthetic calendar"
    );
    let candidate = DateTime::from_timestamp(now.timestamp() / 600 * 600, 0).unwrap();
    let decision = candidate + Duration::minutes(10);
    let successor = decision + Duration::minutes(10);
    assert!(
        decision >= today.and_hms_opt(7, 10, 0).unwrap().and_utc(),
        "wait for the real BO eligible window (first 07:10 Moscow bar + 3h)"
    );
    assert!(
        successor < today.and_hms_opt(20, 40, 0).unwrap().and_utc(),
        "no late-session fixture fallback"
    );
    let mut day = today;
    let mut days = Vec::new();
    while days.len() < 4 {
        day = day.pred_opt().unwrap();
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            days.push(day);
        }
    }
    days.reverse();
    let start = days[0].and_hms_opt(4, 0, 0).unwrap().and_utc();
    assert!(now - start < Duration::days(7));
    let mut template: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/stage-8/stage8b-p1f-o2-no-riskgate-source-template.example.json"
    ))
    .unwrap();
    template["operational_identity_sha256"] = json!(identity);
    template["history_coverage"]["sessions"] =
        json!(days.iter().copied().map(session).collect::<Vec<_>>());
    template["history_coverage"]["candidate_session"] = session(today);
    let template = serde_json::to_vec(&template).unwrap();
    let mut raw = json!({"symbol":"IMOEXF@RTSX","bars":[]});
    for day in &days {
        let mut close = day.and_hms_opt(4, 10, 0).unwrap().and_utc();
        let last = day.and_hms_opt(20, 50, 0).unwrap().and_utc();
        while close <= last {
            append(&mut raw, close, 2280);
            close += Duration::minutes(10);
        }
    }
    let mut close = today.and_hms_opt(4, 10, 0).unwrap().and_utc();
    while close <= candidate {
        append(&mut raw, close, 2280);
        close += Duration::minutes(10);
    }
    let snapshot = evidence(&raw, start, candidate, now);
    let plan = Stage8bP1fObservedM10Plan::from_calendar_template(&template, identity, now).unwrap();
    let material = plan.materialize(snapshot.clone(), now).unwrap();
    assert_eq!(material.candidate().bar().close_ts, candidate);
    let account: AccountResponse = serde_json::from_value(json!({"account_id":"ACC_TEST_0001","cash":[],"positions":[],"status":"ACCOUNT_STATUS_OPEN"})).unwrap();
    let orders = AccountOrdersResponse { orders: vec![] };
    let params: AssetParamsResponse = serde_json::from_value(
        json!({"account_id":"ACC_TEST_0001","symbol":"IMOEXF@RTSX","is_tradable":true}),
    )
    .unwrap();
    let schedule: AssetScheduleResponse = serde_json::from_value(json!({"symbol":"IMOEXF@RTSX","sessions":[{"type":"SESSION_TYPE_MAIN","interval":{
        "start_time":stamp(today.and_hms_opt(4,0,0).unwrap().and_utc()),"end_time":stamp(today.and_hms_opt(20,50,0).unwrap().and_utc())
    }}]})).unwrap();
    let mut routes = [
        (Kind::Account, serde_json::to_vec(&account).unwrap()),
        (Kind::AccountOrders, serde_json::to_vec(&orders).unwrap()),
        (Kind::AssetParams, serde_json::to_vec(&params).unwrap()),
        (Kind::AssetSchedule, serde_json::to_vec(&schedule).unwrap()),
    ]
    .into_iter()
    .map(|(route, bytes)| Stage8bP1fO2GetObservationV1 {
        route,
        request_sha256: hash(b"LOCAL-SYNTHETIC-NO-NETWORK-REQUEST"),
        response_sha256: hash(&bytes),
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
            &template,
            "ACC_TEST_0001",
            account,
            orders,
            params,
            schedule,
            routes,
            now,
        )
        .unwrap();
    config["first_boot_source_bundle_sha256"] = json!(hash(&source.exact_source_bytes));
    write(
        "/etc/moex-finam-p1-paper/bootstrap/stage8b-p1-first-boot-source-v1.json",
        &source.exact_source_bytes,
        0o440,
    );
    let mut secret = zeroize::Zeroizing::new([0u8; 32]);
    fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(secret.as_mut())
        .unwrap();
    write(
        "/run/o3-local-lifecycle/stage8b-p1-lifecycle.key",
        secret.as_ref(),
        0o400,
    );
    let mut make = |close, price| {
        append(&mut raw, close, price);
        let evidence = evidence(&raw, start, close, close);
        // Future fixture input PIN computation only. The production executable
        // admits these bytes later at Utc::now() >= close; no runtime clock arg.
        let admitted = AdmittedClosedM1Snapshot::admit(
            evidence.clone(),
            &ClosedM1RequestPlan::new(start, close, vec![close]).unwrap(),
            close,
        )
        .unwrap();
        let bytes = d::build_stage8b_p1_observed_canonical_m10(
            identity,
            (close - Duration::minutes(10)).timestamp_millis(),
            admitted.receipt(),
        )
        .unwrap();
        O3ObservedInput {
            evidence,
            snapshot_sha256: admitted.sha256().into(),
            receipt_sha256: admitted.receipt().sha256().into(),
            canonical_m10_sha256: hash(&bytes),
        }
    };
    let first = make(decision, 2310);
    let second = make(successor, 2311);
    write("/run/o3-public/local-fixture-provenance.json", &serde_json::to_vec_pretty(&json!({
        "scope":"LOCAL_SYNTHETIC_RECENT_HISTORY_NOT_ACCEPTED_O2_SEED",
        "operator_authorized":true,"captured_at":now,"history_start":start,"candidate":candidate,
        "decision":decision,"successor":successor,"source_sha256":hash(&source.exact_source_bytes),
        "accepted_o2_seed_changed":false,"runtime_clock_override":false,"vps_mutated":false,
        "decision_price":2310,"successor_price":2311,"history_price":2280
    })).unwrap(),0o444);
    (first, second)
}
