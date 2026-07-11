use serde_json::Value;

fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .and_then(|(_, tail)| tail.split_once(end).map(|(section, _)| section))
        .expect("source oracle section must exist")
}

#[test]
fn selected_wrapper_oracle_freezes_bracket_terminal_reconcile_lifecycle() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/stage5/bracket_terminal_reconciliation.json"
    ))
    .expect("bracket terminal fixture must decode");
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["grace_ms"], 3000);
    assert_eq!(fixture["persisted_in_strategy_state"], false);
    assert_eq!(
        fixture["restart_policy"],
        "marker_resets_to_none_and_no_pre_restart_grace_is_inherited"
    );

    let oracle = include_str!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");
    assert!(oracle.contains("bracket_terminal_reconcile_started_ms: Option<i64>"));
    assert!(oracle.contains("const BRACKET_TERMINAL_RECONCILE_GRACE_MS: i64 = 3_000"));
    assert!(oracle.contains("bracket_terminal_reconcile_started_ms: None"));
    assert!(oracle.contains(
        "if status == \"filled\" {\n                    self.mark_bracket_terminal_reconcile();"
    ));
    assert!(oracle
        .contains("self.mark_bracket_terminal_reconcile();\n                self.sl_triggered_ts"));
    assert!(
        oracle.contains("if self.bracket_terminal_reconcile_active(Utc::now().timestamp_millis())")
    );
    assert!(oracle.contains(
        "self.clear_bracket_terminal_reconcile();\n            self.active_cycle_id = None"
    ));
    assert!(oracle.contains("self.emit_bracket_reconcile_timeout_exit(ctx, now_ts_utc_ms)"));
    assert!(oracle.contains("\"bracket_terminal_reconcile_timeout\""));
    assert!(oracle.contains("partial_protective_fill_waits_for_terminal_reconcile"));
    assert!(oracle.contains("terminal_reconcile_timeout_emits_single_residual_flatten"));
}

#[test]
fn selected_wrapper_oracle_keeps_reconcile_marker_transient_across_restart() {
    let oracle = include_str!("../../../source-oracles/alor-stage5/hybrid_intraday_runtime.rs");
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
