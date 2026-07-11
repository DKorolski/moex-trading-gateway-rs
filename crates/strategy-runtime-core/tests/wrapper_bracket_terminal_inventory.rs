use serde_json::Value;

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .and_then(|(_, tail)| tail.split_once(end).map(|(section, _)| section))
        .expect("source oracle section must exist")
}

fn string_array(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("fixture value must be an array")
        .iter()
        .map(|item| item.as_str().expect("fixture array item must be a string"))
        .collect()
}

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/bracket_terminal_reconciliation.json"
    ))
    .expect("bracket terminal fixture must decode")
}

fn oracle() -> &'static str {
    include_str!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs")
}

#[test]
fn selected_wrapper_oracle_freezes_exact_bracket_terminal_status_matrix() {
    let fixture = fixture();
    assert_eq!(fixture["schema_version"], 2);
    assert_eq!(
        string_array(&fixture["grace_start_statuses"]["take_profit"]),
        ["filled"]
    );
    assert_eq!(
        string_array(&fixture["grace_start_statuses"]["stop_loss"]),
        ["filled", "executed", "triggered", "done", "completed"]
    );
    assert_eq!(
        string_array(&fixture["terminal_without_grace"]["take_profit"]),
        ["canceled", "cancelled", "expired", "rejected"]
    );
    assert_eq!(
        string_array(&fixture["terminal_without_grace"]["stop_loss"]),
        ["canceled", "cancelled", "expired", "rejected"]
    );

    let on_order = between(
        oracle(),
        "    fn on_order(&mut self,",
        "    fn on_stop_order(&mut self,",
    );
    assert_eq!(
        on_order
            .matches("self.mark_bracket_terminal_reconcile();")
            .count(),
        1
    );
    assert!(
        on_order.contains("\"filled\" | \"canceled\" | \"cancelled\" | \"expired\" | \"rejected\"")
    );
    assert!(on_order.contains(
        "if status == \"filled\" {\n                    self.mark_bracket_terminal_reconcile();"
    ));

    let on_stop_order = between(
        oracle(),
        "    fn on_stop_order(&mut self,",
        "    fn on_position(&mut self,",
    );
    assert_eq!(
        on_stop_order
            .matches("self.mark_bracket_terminal_reconcile();")
            .count(),
        1
    );
    assert!(on_stop_order
        .contains("\"filled\" | \"executed\" | \"triggered\" | \"done\" | \"completed\""));
    for status in ["canceled", "cancelled", "expired", "rejected"] {
        assert!(on_stop_order.contains(&format!("\"{status}\"")));
    }
    let execution_start = on_stop_order
        .find("\"filled\" | \"executed\" | \"triggered\" | \"done\" | \"completed\"")
        .expect("SL protective-execution matrix must exist");
    let marker_start = on_stop_order
        .find("self.mark_bracket_terminal_reconcile();")
        .expect("SL marker start must exist");
    let canceled_status = on_stop_order
        .find("\"canceled\"")
        .expect("SL canceled terminal status must exist");
    assert!(execution_start < marker_start);
    assert!(marker_start < canceled_status);
}

#[test]
fn selected_wrapper_oracle_freezes_timeout_suppression_and_clock_domains() {
    let fixture = fixture();
    assert_eq!(fixture["grace_ms"], 3000);
    assert_eq!(
        fixture["timeout_guards"]["emit_when"],
        "grace_expired_and_residual_qty_nonzero_and_pending_exit_absent"
    );
    assert_eq!(
        fixture["timeout_guards"]["flat_result"],
        "clear_marker_without_intent"
    );
    assert_eq!(
        fixture["timeout_guards"]["pending_exit_result"],
        "clear_marker_without_duplicate_intent"
    );
    assert_eq!(
        fixture["clock_domain"]["marker_start"],
        "wall_clock_utc_now_ms"
    );
    assert_eq!(
        fixture["clock_domain"]["position_grace_check"],
        "wall_clock_utc_now_ms"
    );
    assert_eq!(
        fixture["clock_domain"]["timer_expiry_check"],
        "timer_callback_now_ts_utc_ms"
    );
    assert_eq!(
        fixture["clock_domain"]["bar_or_event_time_substitution_allowed"],
        false
    );

    let oracle = oracle();
    assert!(oracle.contains("bracket_terminal_reconcile_started_ms: Option<i64>"));
    assert!(oracle.contains("const BRACKET_TERMINAL_RECONCILE_GRACE_MS: i64 = 3_000"));
    assert!(oracle.contains("bracket_terminal_reconcile_started_ms: None"));
    assert!(oracle.contains(
        "self.bracket_terminal_reconcile_started_ms = Some(Utc::now().timestamp_millis());"
    ));
    assert!(
        oracle.contains("self.bracket_terminal_reconcile_active(Utc::now().timestamp_millis())")
    );
    assert!(oracle.contains(
        "if now_ts_utc_ms.saturating_sub(started) < Self::BRACKET_TERMINAL_RECONCILE_GRACE_MS"
    ));
    assert!(oracle.contains(
        "if qty.abs() <= f64::EPSILON || self.pending_exit_request_id.is_some() {\n            self.clear_bracket_terminal_reconcile();\n            return Vec::new();"
    ));
    assert!(oracle.contains("self.emit_bracket_reconcile_timeout_exit(ctx, now_ts_utc_ms)"));
    assert!(oracle.contains("\"bracket_terminal_reconcile_timeout\""));
    assert!(oracle.contains("partial_protective_fill_waits_for_terminal_reconcile"));
    assert!(oracle.contains("terminal_reconcile_timeout_emits_single_residual_flatten"));
}

#[test]
fn selected_wrapper_oracle_keeps_reconcile_marker_transient_across_restart() {
    let fixture = fixture();
    assert_eq!(fixture["persisted_in_strategy_state"], false);
    assert_eq!(
        fixture["restart_policy"],
        "marker_resets_to_none_and_no_pre_restart_grace_is_inherited"
    );

    let oracle = oracle();
    let sync_state = between(
        oracle,
        "    fn sync_state(&mut self) {",
        "    fn is_window_closed_recoverable_reject",
    );
    assert!(!sync_state.contains("bracket_terminal_reconcile_started_ms"));

    let set_state = oracle
        .split_once("    fn set_state(&mut self, state: StrategyState) {")
        .map(|(_, section)| section)
        .expect("set_state must exist in source oracle");
    assert!(!set_state.contains("bracket_terminal_reconcile_started_ms"));
}
