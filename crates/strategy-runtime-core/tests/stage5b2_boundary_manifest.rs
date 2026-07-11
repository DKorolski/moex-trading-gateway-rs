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

fn direct_strategy_impl_methods() -> BTreeSet<&'static str> {
    let implementation_marker = ["impl Strategy for Hybrid", "IntradayRuntimeStrategy {"].concat();
    oracle()
        .split_once(&implementation_marker)
        .map(|(_, implementation)| implementation)
        .expect("Strategy impl must exist")
        .lines()
        .filter_map(|line| {
            line.strip_prefix("    fn ")
                .and_then(|signature| signature.split_once('(').map(|(name, _)| name))
        })
        .collect()
}

#[test]
fn stage5b2_mapping_is_callback_complete_and_matches_exact_source_impl() {
    let fixture = fixture();
    assert_eq!(fixture["schema_version"], 2);
    assert_eq!(fixture["stage"], "Stage5B2a");
    assert_eq!(
        fixture["implementation_status"],
        "lossless_boundary_contracts_defined_no_wrapper"
    );

    let callbacks = fixture["callbacks"]
        .as_array()
        .expect("callbacks must be an array");
    assert_eq!(callbacks.len(), 21);
    assert_eq!(fixture["callback_count"], 21);
    assert_eq!(fixture["source_override_count"], 15);
    assert_eq!(fixture["generic_host_seam_count"], 6);

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

    let declared_source_overrides = callbacks
        .iter()
        .filter(|callback| callback["source_override"] == true)
        .map(|callback| callback["name"].as_str().expect("callback name"))
        .collect::<BTreeSet<_>>();
    assert_eq!(declared_source_overrides.len(), 15);
    assert_eq!(declared_source_overrides, direct_strategy_impl_methods());

    for callback in callbacks {
        let name = callback["name"].as_str().expect("callback name");
        if callback["source_override"] == false {
            assert_eq!(callback["gate"], "Stage5CExplicitPolicy");
        }
        assert!(callback["source_required_fields"].is_array());
        assert!(callback["target_required_fields"].is_array());
        for field in [
            "target_contract",
            "mapping_status",
            "status_mapping",
            "timestamp_mapping",
            "identity_mapping",
            "origin_semantics",
            "redaction_location",
        ] {
            assert!(
                callback[field]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()),
                "{name}.{field} must be explicit"
            );
        }
        assert!(callback["lossless"].is_boolean());
    }
}

#[test]
fn stage5b2_mapping_rejects_lossy_aliases_for_source_critical_callbacks() {
    let fixture = fixture();
    let callbacks = fixture["callbacks"].as_array().expect("callbacks");
    let callback = |name: &str| {
        callbacks
            .iter()
            .find(|callback| callback["name"] == name)
            .expect("mapped callback")
    };

    let ack = callback("on_ack");
    assert_eq!(
        ack["target_contract"],
        "broker_core::HybridRuntimeCommandAck"
    );
    assert!(ack["source_required_fields"]
        .as_array()
        .expect("ACK fields")
        .iter()
        .any(|field| field == "error_code"));
    assert!(ack["status_mapping"]
        .as_str()
        .expect("ACK status mapping")
        .contains("Submitted/Recovered->Confirmed"));

    let order = callback("on_order");
    assert_eq!(
        order["target_contract"],
        "broker_core::HybridRuntimeOrderEvent"
    );
    assert!(order["target_required_fields"]
        .as_array()
        .expect("order fields")
        .iter()
        .any(|field| field == "attribution"));
    assert_eq!(order["lossless"], true);

    let position = callback("on_position");
    assert!(position["target_required_fields"]
        .as_array()
        .expect("position fields")
        .iter()
        .any(|field| field == "existing"));

    let bootstrap = callback("on_bootstrap_snapshot");
    assert!(bootstrap["target_required_fields"]
        .as_array()
        .expect("bootstrap fields")
        .iter()
        .any(|field| field == "working_stop_orders_strategy"));

    let bar = callback("on_bar");
    assert!(bar["origin_semantics"]
        .as_str()
        .expect("bar origin mapping")
        .contains("HistoryGap"));

    let stop = callback("on_stop_order");
    assert!(stop["identity_mapping"]
        .as_str()
        .expect("stop identity mapping")
        .contains("namespaces never merge"));
}

#[test]
fn stage5b2_mapping_freezes_typed_ids_state_groups_and_backlog_invariants() {
    let fixture = fixture();
    let identities = fixture["identity_mappings"]
        .as_array()
        .expect("identity mappings");
    assert_eq!(identities.len(), 3);
    assert!(identities.iter().any(|mapping| {
        mapping["source"] == "all strategy command/request UUID identities"
            && mapping["target"] == "StrategyRequestId"
    }));
    assert_eq!(
        identities
            .iter()
            .filter(|mapping| mapping["target"] == "BrokerOrderId(String)")
            .count(),
        1
    );
    assert!(identities.iter().any(|mapping| {
        mapping["target"] == "BrokerStopOrderId(String)"
            && mapping["rule"]
                .as_str()
                .expect("stop identity rule")
                .contains("never interchangeable")
    }));

    let ack_matrix = fixture["ack_status_matrix"]
        .as_array()
        .expect("ACK status matrix");
    assert_eq!(ack_matrix.len(), 9);
    assert!(ack_matrix.iter().any(|row| {
        row["broker_status"] == "Submitted"
            && row["hybrid_status"] == "Confirmed"
            && row["callback_allowed"] == true
    }));
    assert!(ack_matrix.iter().any(|row| {
        row["broker_status"] == "UnknownPending" && row["callback_allowed"] == false
    }));

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
