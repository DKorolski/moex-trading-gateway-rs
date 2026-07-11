use std::collections::BTreeSet;

use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/stage5b2_callback_state_mapping.json"
    ))
    .expect("Stage 5B-2 callback/state mapping must decode")
}

fn oracle() -> &'static str {
    include_str!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs")
}

#[test]
fn stage5b2_mapping_is_callback_complete_and_distinguishes_source_defaults() {
    let fixture = fixture();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["stage"], "Stage5B2a");
    assert_eq!(fixture["implementation_status"], "boundary_manifest_only");

    let callbacks = fixture["callbacks"]
        .as_array()
        .expect("callbacks must be an array");
    let names = callbacks
        .iter()
        .map(|callback| callback["name"].as_str().expect("callback name"))
        .collect::<BTreeSet<_>>();
    let expected = [
        "acknowledge_risk_gate_session_finalizations",
        "drain_observation_journal_records",
        "exit_risk_status",
        "intent_comment_tag",
        "on_ack",
        "on_bar",
        "on_bootstrap_snapshot",
        "on_command_prepared",
        "on_intent_blocked",
        "on_order",
        "on_position",
        "on_risk_gate_state",
        "on_runtime_state_restored",
        "on_stop_order",
        "on_timer",
        "pending_request_ids",
        "risk_gate_session_finalizations",
        "set_state",
        "state",
        "tracked_order_ids",
        "warmup_from_history",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(names, expected);

    for callback in callbacks {
        let name = callback["name"].as_str().expect("callback name");
        let source_override = callback["source_override"]
            .as_bool()
            .expect("source_override bool");
        if source_override {
            assert!(
                oracle().contains(&format!("fn {name}(")),
                "source override marker missing for {name}"
            );
        } else {
            assert_eq!(callback["gate"], "Stage5CExplicitPolicy");
        }
    }
}

#[test]
fn stage5b2_mapping_freezes_typed_ids_state_groups_and_backlog_invariants() {
    let fixture = fixture();
    let identities = fixture["identity_mappings"]
        .as_array()
        .expect("identity mappings");
    assert_eq!(identities.len(), 4);
    assert!(identities.iter().any(|mapping| {
        mapping["source"] == "Uuid pending request id" && mapping["target"] == "StrategyRequestId"
    }));
    assert_eq!(
        identities
            .iter()
            .filter(|mapping| mapping["target"] == "BrokerOrderId(String)")
            .count(),
        3
    );
    assert!(identities
        .iter()
        .filter(|mapping| {
            mapping["source"] == "i64 order id" || mapping["source"] == "i64 stop exchange order id"
        })
        .all(|mapping| mapping["rule"]
            .as_str()
            .expect("identity rule")
            .contains("no surrogate")));

    let state_groups = fixture["state_groups"].as_array().expect("state groups");
    assert_eq!(state_groups.len(), 8);
    assert!(state_groups.iter().all(|group| group["required"] == true));
    assert!(state_groups.iter().any(|group| {
        group["name"] == "bracket_terminal_reconcile_marker"
            && group["persistence"] == "transient_reset_on_restart"
    }));

    let backlog = fixture["backlog_executable_invariants"]
        .as_array()
        .expect("backlog invariants");
    assert_eq!(backlog.len(), 2);
    assert!(backlog.iter().any(|item| {
        item == "repeated_qualifying_tp_or_sl_execution_event_restarts_grace_timestamp"
    }));
    assert!(backlog
        .iter()
        .any(|item| { item == "timeout_qty_uses_context_position_qty_or_else_last_position_qty" }));
}

#[test]
fn stage5b2_mapping_keeps_wrapper_and_execution_surfaces_closed() {
    let fixture = fixture();
    let safety = &fixture["safety_boundary"];
    assert_eq!(safety["paper_boundary"], true);
    for field in [
        "wrapper_copied",
        "wrapper_compiled",
        "runtime_host_attached",
        "runtime_live_ready_enabled",
        "command_consumer_to_real_finam_enabled",
        "strategy_driven_real_order_enabled",
        "real_post_delete_added",
        "stop_sltp_bracket_execution_enabled",
    ] {
        assert_eq!(safety[field], false, "{field} must remain false");
    }
}
