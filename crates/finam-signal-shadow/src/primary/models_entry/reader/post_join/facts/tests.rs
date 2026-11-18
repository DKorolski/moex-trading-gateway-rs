use super::*;
fn sample() -> (WsPrimaryConfig, Value, crate::primary::eod::Completion) {
    let config = crate::primary::models_entry::tests::fixture().config;
    let mut http = json!({});
    for (role, _) in ROLES {
        http[role] = json!({"scheduled":0,"completed":0,"cancelled_or_unaccounted":0,
            "known_get_send_attempts":0,"complete_response_body_bytes":0,"completed_command_elapsed_ms":0});
    }
    let shutdown = json!({"recovery":[{
        "episode":1,"attempt":0,"generation":1,"stage":"Ready",
        "original":{"generation":1,"kind":{"Source":"Continuity"},"liveness":null},
        "fault_observed_at_ms":1,"deadline_ms":45001,
        "transport_unresolved":false,"canonical_unresolved":false,
        "last_error":null,"terminal":null}],
        "http":http,"drained_http":[],"continuity_lost":false,"unresolved_canonical_gap":false});
    let done = crate::primary::eod::Completion {
        schema_version: 1,
        config_sha256: String::new(),
        calendar_sha256: String::new(),
        journal: None,
        calendar_eod_trigger: false,
        eod_check_at_ms: 0,
        stop_started_at_ms: 0,
        joined_at_ms: 0,
        stop_budget_ms: 1000,
        elapsed_stop_ms: 0,
        clock_consistent: true,
        socket_drained: true,
        http_drained: true,
        writer_drained: true,
        fault_free: true,
        continuity_resolved: true,
        recovery_resolved: true,
        model_prepared: false,
        semantic_eod_passed: false,
        cgroup_qualified: false,
        network_qualified: false,
    };
    (config, shutdown, done)
}
#[test]
fn final_recovery_uses_runtime_formula_not_the_summary() {
    let (config, s, d) = sample();
    assert!(validate(&config, &[], &s, &d).unwrap());
    for field in ["transport_unresolved", "canonical_unresolved", "terminal"] {
        let mut bad = s.clone();
        bad["recovery"][0][field] = if field == "terminal" {
            json!("Stopped")
        } else {
            json!(true)
        };
        let error = validate(&config, &[], &bad, &d).unwrap_err();
        assert_eq!(
            error.to_string(),
            "final recovery facts contradict completion"
        );
    }
}
#[test]
fn unknown_or_missing_recovery_shape_never_defaults_to_resolved() {
    let (config, s, d) = sample();
    assert!(validate(&config, &[], &s, &d).unwrap());
    for recovery in [
        Value::Null,
        json!({}),
        json!([{}]),
        json!([{"canonical_unresolved":false}]),
    ] {
        let mut bad = s.clone();
        bad["recovery"] = recovery;
        assert!(validate(&config, &[], &bad, &d).is_err());
    }
    let mut bad = s;
    bad["recovery"][0]["stage"] = json!("Unrecognised");
    assert!(validate(&config, &[], &bad, &d).is_err());
}
#[test]
fn final_gap_alias_is_required_typed_and_matches_continuity() {
    let (config, s, d) = sample();
    assert!(validate(&config, &[], &s, &d).unwrap());
    for gap in [json!(true), json!("false"), Value::Null] {
        let mut bad = s.clone();
        bad["unresolved_canonical_gap"] = gap;
        assert!(validate(&config, &[], &bad, &d).is_err());
    }
}
#[test]
fn final_http_accounting_is_typed_overflow_safe_and_not_copy_equality() {
    let (config, s, d) = sample();
    assert!(validate(&config, &[], &s, &d).unwrap());
    let mut bad = s.clone();
    bad["http"]["recovery"]["scheduled"] = json!(1);
    assert_eq!(
        validate(&config, &[], &bad, &d).unwrap_err().to_string(),
        "final HTTP accounting arithmetic"
    );
    bad["http"]["recovery"]["completed"] = json!(u64::MAX);
    bad["http"]["recovery"]["cancelled_or_unaccounted"] = json!(1);
    assert!(validate(&config, &[], &bad, &d).is_err());
    let mut bad = s;
    bad["http"]["unknown"] = json!({});
    assert!(validate(&config, &[], &bad, &d).is_err());
}
#[test]
fn optional_failed_and_executor_incomplete_comparisons_remain_permitted() {
    let (config, mut s, d) = sample();
    s["http"]["proof"]["scheduled"] = json!(1);
    s["http"]["proof"]["completed"] = json!(1);
    s["drained_http"] =
        json!([{"role":"Proof","class":"Timeout","receipt":{"auth_generation":null}}]);
    assert!(
        validate(&config, &[], &s, &d).unwrap(),
        "optional Timeout is not a new REST gate"
    );
    s["http"]["proof"]["completed"] = json!(0);
    s["http"]["proof"]["cancelled_or_unaccounted"] = json!(1);
    s["drained_http"][0]["receipt"]["executor_completed"] = json!(false);
    assert!(
        validate(&config, &[], &s, &d).unwrap(),
        "known optional incomplete result is accounted"
    );
    let mut row = s["drained_http"][0].clone();
    row["event"] = json!("primary_http_completed");
    s["drained_http"] = json!([]);
    assert!(validate(&config, &[json!({"event":row})], &s, &d).unwrap());
}
#[test]
fn honestly_unfinished_recovery_and_http_do_not_mint_complete_facts() {
    let (config, mut s, mut d) = sample();
    s["recovery"][0]["canonical_unresolved"] = json!(true);
    d.recovery_resolved = false;
    d.fault_free = false;
    s["http"]["recovery"]["scheduled"] = json!(1);
    s["http"]["recovery"]["cancelled_or_unaccounted"] = json!(1);
    assert!(
        !validate(&config, &[], &s, &d).unwrap(),
        "consistent unfinished run remains eligible only for PARTIAL"
    );
}
