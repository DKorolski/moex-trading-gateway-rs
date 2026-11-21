use super::*;
use serde_json::json;

#[test]
fn v7_r1_reader_binds_failed_receipt_to_request_and_rejects_forged_renewal() {
    let mut raw = crate::primary::tests::config();
    raw["primary"]["delivery"] =
        json!(crate::primary::DeliveryPolicy::ObservedBoDataWaitRecoveryV7);
    let config = WsPrimaryConfig::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let cfg = config.fingerprint().unwrap();
    let policy = config.primary.fingerprint().unwrap();
    let start = config.base.observation_start_utc_ms;
    let cutover = start + 600000;
    let issued = cutover + 100;
    let episode_deadline = issued + 45000;
    let request = |ordinal: u8, serial: u64, at: i64| {
        let scope = RecoveryRequestScope {
            episode: 1,
            ordinal,
            deadline_ms: episode_deadline,
        };
        let deadline = (at + 5000).min(episode_deadline);
        let mut b = serde_json::to_vec(&(
            &cfg,
            &policy,
            1u64,
            start,
            cutover,
            0usize,
            at,
            deadline,
            false,
            Option::<String>::None,
        ))
        .unwrap();
        b.extend(serde_json::to_vec(&(Some(scope), serial)).unwrap());
        let hash = digest(&b);
        json!({"event":"primary_recovery_request_issued","ticket_sha256":hash,"scope":scope,
            "serial":serial,"generation":1,"frontier_index":0,"start_ms":start,"cutover_ms":cutover,
            "issued_at_ms":at,"deadline_ms":deadline,"maximum_reads":4,"startup_anchor":false,"calendar_sha256":null})
    };
    let first = request(1, 1, issued);
    let next = request(2, 2, issued + 1001);
    let receipt = json!({"started_at_ms":issued,"finished_at_ms":issued+1,"deadline_ms":issued+5000,
        "get_attempts":1,"auth_generation":1});
    let wrap = |e: Value| json!({"kind":"transport","config_sha256":cfg,"event":e});
    let rows = vec![
        wrap(json!({"event":"primary_subscription_sent","generation":1})),
        wrap(
            json!({"event":"primary_recovery_episode_started","audit":{"episode":1,"fault_observed_at_ms":issued,"deadline_ms":episode_deadline}}),
        ),
        wrap(first.clone()),
        wrap(json!({"event":"primary_http_completed","role":"Recovery","receipt":receipt})),
        wrap(
            json!({"event":"primary_recovery_request_failed","ticket_sha256":first["ticket_sha256"],"scope":first["scope"],
            "generation":1,"error":"Unavailable","receipt":receipt,"at_ms":issued+1,"callback":false,"validated_source":false}),
        ),
        wrap(
            json!({"event":"primary_recovery_request_retired","ticket_sha256":first["ticket_sha256"],
            "replacement_sha256":next["ticket_sha256"],"reason":"Renewal","at_ms":issued+1001}),
        ),
        wrap(next),
    ];
    let encode = |rows: &[Value]| {
        rows.iter()
            .flat_map(|r| {
                let mut b = serde_json::to_vec(r).unwrap();
                b.push(b'\n');
                b
            })
            .collect::<Vec<u8>>()
    };
    let valid = |rows: &[Value]| {
        validate(
            &config,
            &encode(rows),
            &[start],
            &BTreeMap::new(),
            issued + 2000,
        )
    };
    valid(&rows).unwrap();
    for kind in 0..8 {
        let mut bad = rows.clone();
        match kind {
            0 => bad[4]["event"]["callback"] = json!(true),
            1 => bad[4]["event"]["validated_source"] = json!(true),
            2 => bad[4]["event"]["error"] = json!("Authentication"),
            3 => bad[4]["event"]["receipt"]["deadline_ms"] = json!(issued + 6000),
            4 => bad[4]["event"]["ticket_sha256"] = json!("f".repeat(64)),
            5 => {
                bad.remove(4);
            }
            6 => bad[5]["event"]["at_ms"] = json!(issued + 999),
            _ => bad[6]["event"]["scope"]["ordinal"] = json!(5),
        }
        assert!(valid(&bad).is_err(), "mutation {kind}");
    }
}
