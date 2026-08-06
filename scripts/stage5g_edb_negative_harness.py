#!/usr/bin/env python3
"""Mutation matrix for the Stage 5G-e-d-b reducer boundary."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

import stage5g_edb_check as checker


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5g_edb_check.py"
REDUCER = str(checker.REDUCER_SOURCE)
PARENT = str(checker.PARENT_SOURCE)
CONTRACT = str(checker.CONTRACT)
FILES = sorted({path for _, path in checker.EXPECTED_DELTA} | {PARENT})


def replace_once(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(
            f"mutation anchor must occur once in {relative}: {old!r}; got {text.count(old)}"
        )
    path.write_text(text.replace(old, new, 1))


def replace_first(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing in {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


def append_before_tests(root: Path, value: str) -> None:
    replace_once(root, REDUCER, "#[cfg(test)]", value + "\n#[cfg(test)]")


def cases() -> list[tuple[str, object]]:
    values: list[tuple[str, object]] = []
    for index in range(1, 13):
        name = f"remove-grst{index:02d}-positive-witness"
        values.append((name, lambda root, index=index: replace_once(
            root, REDUCER, f"fn stage5g_edb_grst{index:02d}()",
            f"fn removed_stage5g_edb_grst{index:02d}()")))
    for reason in checker.REASONS:
        values.append((f"remove-reason-{reason}", lambda root, reason=reason: replace_once(
            root, REDUCER, f"    {reason},\n", "")))
    for scenario in checker.SCENARIOS:
        values.append((f"drift-scenario-{scenario}", lambda root, scenario=scenario: replace_once(
            root, CONTRACT, f'    "{scenario}"', f'    "MUTATED_{scenario}"')))
    for disposition in checker.DISPOSITIONS:
        values.append((f"drift-disposition-{disposition}", lambda root, disposition=disposition: replace_once(
            root, CONTRACT, f'    "{disposition}"', f'    "Mutated{disposition}"')))
    for binding in checker.CROSS_BINDINGS:
        values.append((f"remove-cross-binding-{binding}", lambda root, binding=binding: replace_first(
            root, CONTRACT, f'    "{binding}",\n' if binding != checker.CROSS_BINDINGS[-1]
            else f'    "{binding}"\n', "")))
    for surface in checker.CLOSED_SURFACES:
        values.append((f"open-surface-{surface}", lambda root, surface=surface: replace_once(
            root, CONTRACT, f'    "{surface}": false', f'    "{surface}": true')))
    values.extend([
        ("remove-child-module-registration", lambda root: replace_once(
            root, PARENT, "mod reducer;", "// reducer removed")),
        ("rename-owning-entry", lambda root: replace_once(
            root, REDUCER, "pub(crate) fn reduce_stage5g_fresh_broker_truth(",
            "pub(crate) fn bypass_reduce_stage5g_fresh_broker_truth(")),
        ("make-reduction-clone", lambda root: replace_once(
            root, REDUCER, "pub(crate) struct Stage5gFreshTruthReduction {",
            "#[derive(Clone)]\npub(crate) struct Stage5gFreshTruthReduction {")),
        ("remove-candidate-validation", lambda root: replace_once(
            root, REDUCER, "if !candidate_is_self_consistent(&candidate) {", "if false {")),
        ("remove-account-cross-binding", lambda root: replace_once(
            root, REDUCER, "restart.account_id == package.operational_identity.account_id",
            "true")),
        ("collapse-incomplete-to-absence", lambda root: replace_once(
            root, REDUCER, "if !truth.orders_complete {", "if false {")),
        ("open-wall-clock", lambda root: append_before_tests(
            root, "fn forbidden_clock() { let _ = chrono::Utc::now(); }")),
        ("open-redis", lambda root: append_before_tests(
            root, "fn forbidden_redis() { let _ = redis::Client::open(\"redis://x\"); }")),
        ("open-finam", lambda root: append_before_tests(
            root, "fn forbidden_finam() { let _ = finam_transport(); }")),
        ("open-http-post", lambda root: append_before_tests(
            root, "fn forbidden_post(client: reqwest::Client) { let _ = client.post(\"/orders\"); }")),
        ("open-callback", lambda root: append_before_tests(
            root, "fn forbidden_callback() { let _ = strategy::on_bar; }")),
        ("open-dispatch", lambda root: append_before_tests(
            root, "fn forbidden_dispatch() { dispatch_order(); }")),
    ])
    for witness in [
        "stage5g_edb_r1_all_operational_identity_fields_are_commitment_bound",
        "stage5g_edb_r1_restart_replay_commitment_and_conflict_are_enforced",
        "stage5g_edb_r1_exact_trade_linkage_rejects_secondary_id_conflicts",
        "stage5g_edb_r1_source_relative_entry_exit_and_terminal_matrix",
        "stage5g_edb_r1_owning_timer_ready_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_awaiting_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing",
        "stage5g_edb_r1_owning_remaining_grst_paths_are_fail_closed_or_noop",
        "stage5g_edb_r1_owning_generated_intent_escrow_is_retained",
        "stage5g_edb_r1_owning_exact_current_and_historical_replay_are_noops",
        "stage5g_edb_r1_owning_row_order_and_parallel_evidence_are_deterministic",
        "stage5g_edb_r2_twelve_prebind_operational_authority_mismatches_fail",
        "stage5g_edb_r2_account_wide_order_safety_is_owning_and_fail_closed",
        "stage5g_edb_r2_source_market_limit_and_cancel_actions_are_owning",
        "stage5g_edb_r2_source_fresh_monotonicity_is_owning",
        "stage5g_edb_r2_semantic_instrument_conflicts_and_fractional_source_block",
        "stage5g_edb_r3_source_limit_price_authority_fails_closed_for_every_broker_shape",
        "stage5g_edb_r3_cancel_command_and_target_order_identities_are_distinct",
        "stage5g_edb_r3_historical_same_instrument_rows_are_partitioned_and_counted",
        "stage5g_edb_r3_semantic_terminal_refresh_ignores_receipt_and_volatile_pnl",
        "stage5g_edb_r3_shared_order_and_trade_correlation_truth_tables_are_exact",
        "stage5g_edb_r3_semantic_comparators_ignore_only_reviewed_volatile_fields",
        "stage5g_edb_r3_owning_grst03_runs_full_authenticated_path",
        "stage5g_edb_r3_working_order_requires_complete_pre_position_truth",
        "stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history",
        "stage5g_edb_r4_no_slot_active_and_unknown_orders_still_block",
        "stage5g_edb_r4_owning_grst06_canonicalizes_both_flat_representations",
        "stage5g_edb_r4_flat_absence_never_overrides_incomplete_or_nonflat_truth",
        "stage5g_edb_r4_cancel_target_authority_is_action_scoped_and_production_derived",
        "stage5g_edb_r4_cancel_target_identity_conflicts_only_against_authenticated_authority",
        "stage5g_edb_r4_immutable_target_order_payload_cannot_drift",
        "stage5g_edb_r4_place_market_tif_is_immutable",
    ]:
        values.append((f"remove-r1-witness-{witness}", lambda root, witness=witness: replace_once(
            root, REDUCER, f"fn {witness}()", f"fn removed_{witness}()")))
    values.extend([
        *[(
            f"remove-prebind-operational-authority-{field}",
            lambda root, field=field: replace_once(
                root, REDUCER,
                "fn stage5g_edb_r2_twelve_prebind_operational_authority_mismatches_fail()",
                f"fn removed_prebind_authority_{field}()",
            ),
        ) for field in checker.CROSS_BINDINGS[:12]],
        ("remove-replay-exact-membership-boundary", lambda root: replace_once(
            root, PARENT, "fn restart_replay_hints_match_checkpoint(",
            "fn removed_restart_replay_hints_match_checkpoint(")),
        ("make-reviewed-operational-authority-clone", lambda root: replace_once(
            root, PARENT,
            "pub(crate) struct Stage5gReviewedOperationalIdentityAuthority {",
            "#[derive(Clone)]\npub(crate) struct Stage5gReviewedOperationalIdentityAuthority {")),
        ("remove-reviewed-authority-test-only-seal", lambda root: replace_once(
            root, PARENT,
            "#[cfg(test)]\npub(super) fn stage5g_test_reviewed_operational_identity_authority(",
            "pub(super) fn stage5g_test_reviewed_operational_identity_authority(")),
        ("replace-replay-membership-with-length", lambda root: replace_once(
            root, PARENT, ".any(|entry| entry.identity == current_identity);",
            ".count() == checkpoint.payload.evidence_replay_ledger.len();")),
        ("allow-arbitrary-current-replay-tuple", lambda root: replace_once(
            root, REDUCER, "Stage5gFreshPackageLineage::ReplayTupleNotInRestartLedger =>",
            "Stage5gFreshPackageLineage::NewFresh =>")),
        ("allow-arbitrary-historical-replay-tuple", lambda root: replace_once(
            root, REDUCER, "Stage5gFreshPackageLineage::HistoricalReplayNotAccepted =>",
            "Stage5gFreshPackageLineage::NewFresh =>")),
        ("drop-account-wide-active-guard", lambda root: replace_once(
            root, REDUCER, "Stage5gAccountWideOrderSafety::NonOwnedActive =>",
            "Stage5gAccountWideOrderSafety::Safe =>")),
        ("drop-account-wide-unknown-guard", lambda root: replace_once(
            root, REDUCER, "Stage5gAccountWideOrderSafety::NonOwnedUnknown =>",
            "Stage5gAccountWideOrderSafety::Safe =>")),
        ("drop-account-wide-ambiguous-guard", lambda root: replace_once(
            root, REDUCER, "Stage5gAccountWideOrderSafety::AmbiguousOwned =>",
            "Stage5gAccountWideOrderSafety::Safe =>")),
        ("drop-market-limit-parity", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) fn stage5g_order_matches_source_action(",
            "pub(crate) fn removed_stage5g_order_matches_source_action(")),
        ("drop-cancel-target-parity", lambda root: replace_once(
            root, REDUCER, "stage5g_order_matches_source_action(&slot.source_action, order)",
            "true")),
        ("drop-terminal-regression-guard", lambda root: replace_once(
            root, REDUCER,
            "committed_order.lifecycle == BrokerOrderLifecycle::Terminal",
            "false")),
        ("drop-filled-regression-guard", lambda root: replace_once(
            root, REDUCER, "fresh_order.filled_qty < committed_order.filled_qty", "false")),
        ("drop-committed-trade-subset-guard", lambda root: replace_once(
            root, REDUCER, "for committed in &slot.trades", "for committed in &[]")),
        ("drop-committed-trade-payload-guard", lambda root: replace_once(
            root, REDUCER, "stage5g_immutable_trade_payload_matches(committed, fresh)", "true")),
        ("weaken-grst06-to-position-only", lambda root: replace_once(
            root, REDUCER,
            "&& progress == Stage5gSourceFreshProgress::ExactCommittedTerminal",
            "&& true")),
        ("restore-exact-only-target-filter-without-semantic-conflict", lambda root: replace_once(
            root, REDUCER, "let target_identity_conflict = truth.orders.iter().any(|row| {",
            "let target_identity_conflict = false && truth.orders.iter().any(|row| {")),
        ("remove-fractional-numeric-policy", lambda root: replace_once(
            root, REDUCER, "!restart.committed_position_numeric_authority_is_integral", "false")),
        ("merge-historical-not-accepted-into-fingerprint-conflict", lambda root: replace_once(
            root, REDUCER, "Stage5gFreshPackageLineage::HistoricalReplayNotAccepted =>",
            "Stage5gFreshPackageLineage::ReplayFingerprintConflict =>")),
        ("remove-restart-bound-package-type", lambda root: replace_once(
            root, PARENT, "pub(crate) struct Stage5gRestartBoundFreshBrokerTruthPackage",
            "pub(crate) struct RemovedStage5gRestartBoundFreshBrokerTruthPackage")),
        ("remove-restart-binding-constructor", lambda root: replace_once(
            root, PARENT, "pub(crate) fn bind_stage5g_fresh_truth_to_clean_restart(",
            "pub(crate) fn removed_bind_stage5g_fresh_truth_to_clean_restart(")),
        ("remove-restart-replay-authority-check", lambda root: replace_once(
            root, PARENT, "fn restart_replay_hints_match_checkpoint(",
            "fn removed_restart_replay_hints_match_checkpoint(")),
        ("remove-operational-binding-commitment", lambda root: replace_first(
            root, REDUCER, "stage5g_operational_binding_commitment(",
            "removed_operational_binding_commitment(")),
        ("remove-restart-replay-commitment", lambda root: replace_once(
            root, REDUCER, "stage5g_restart_replay_commitment(restart, &package.replay_hints)",
            "removed_restart_replay_commitment(restart, &package.replay_hints)")),
        ("allow-target-venue-wildcard", lambda root: replace_once(
            root, REDUCER,
            "restart.instrument_id == package.operational_identity.target_instrument",
            "instrument_identity_matches(&restart.instrument_id, &package.operational_identity.target_instrument)")),
        ("bypass-exact-trade-linkage-selection", lambda root: replace_once(
            root, REDUCER, "match stage5g_exact_trade_order_linkage(order, trade) {",
            "match Stage5gTradeOrderLinkage::Exact {")),
        ("remove-source-relative-position", lambda root: replace_once(
            root, REDUCER, "stage5g_expected_post_position_qty(slot.pre_position_qty, order)",
            "stage5g_expected_post_position_qty(Decimal::ZERO, order)")),
        ("remove-slot-intent-class", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) intent_class: Stage5gRestartIntentClass,",
            "pub(crate) removed_intent_class: Stage5gRestartIntentClass,")),
        ("remove-slot-pre-position", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) pre_position_qty: Decimal,",
            "pub(crate) removed_pre_position_qty: Decimal,")),
        ("remove-slot-exact-target-quantity", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) target_qty: Option<Decimal>,",
            "pub(crate) removed_target_qty: Option<Decimal>,")),
        ("replace-shared-trade-linkage-helper", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) fn stage5g_exact_trade_order_linkage(",
            "pub(crate) fn removed_stage5g_exact_trade_order_linkage(")),
        ("revert-post-position-to-signed-fill", lambda root: replace_first(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pre_position_qty + signed_fill",
            "signed_fill")),
        ("remove-new-working-status-rule", lambda root: replace_first(
            root, REDUCER, "OrderStatus::New | OrderStatus::Working => {",
            "OrderStatus::New => {")),
        ("remove-partial-status-rule", lambda root: replace_first(
            root, REDUCER, "OrderStatus::PartiallyFilled => {",
            "OrderStatus::Unknown(_) => {")),
        ("remove-filled-status-rule", lambda root: replace_first(
            root, REDUCER, "OrderStatus::Filled => {",
            "OrderStatus::Unknown(_) => {")),
        ("remove-rejected-status-rule", lambda root: replace_first(
            root, REDUCER, "OrderStatus::Rejected => {",
            "OrderStatus::Unknown(_) => {")),
        ("remove-canceled-expired-status-rule", lambda root: replace_first(
            root, REDUCER, "OrderStatus::Canceled | OrderStatus::Expired => {",
            "OrderStatus::Canceled => {")),
        ("remove-positions-complete-authority", lambda root: replace_first(
            root, REDUCER, "if !truth.positions_complete {",
            "if false {")),
        ("remove-replay-conflict-production-arm", lambda root: replace_once(
            root, REDUCER,
            "Stage5gFreshPackageLineage::ReplayFingerprintConflict => {",
            "Stage5gFreshPackageLineage::UnknownHistoricalReplay => {")),
        ("drop-candidate-limit-price-fail-closed", lambda root: replace_once(
            root, REDUCER,
            "let source_price_authority_is_supported = !matches!(",
            "let source_price_authority_is_supported = true || !matches!(")),
        ("drop-candidate-working-position-completeness", lambda root: replace_first(
            root, REDUCER,
            "&& candidate.trades.is_empty()\n                && candidate.positions_complete",
            "&& candidate.trades.is_empty()\n                && true")),
    ])
    for correlation in checker.ORDER_CORRELATIONS:
        values.append((f"remove-order-correlation-{correlation}",
                       lambda root, correlation=correlation: replace_first(
                           root, str(checker.ORDER_POSITION_SOURCE),
                           f"    {correlation},\n", "")))
    for linkage in checker.TRADE_LINKAGES:
        values.append((f"remove-trade-linkage-{linkage}",
                       lambda root, linkage=linkage: replace_first(
                           root, str(checker.ORDER_POSITION_SOURCE),
                           f"    {linkage},\n", "")))
    r3_policy_mutations = {
        "source-limit-policy": (
            '"source_limit_price_authority": "fail_closed_until_canonical_decimal_tick_authority"',
            '"source_limit_price_authority": "accept_positive_broker_limit"',
        ),
        "cancel-identity-separation": (
            '"cancel_command_and_target_identity_separated": true',
            '"cancel_command_and_target_identity_separated": false',
        ),
        "historical-terminal-order-partition": (
            '"historical_terminal_orders_ignored_after_account_wide_safety": true',
            '"historical_terminal_orders_ignored_after_account_wide_safety": false',
        ),
        "historical-trade-partition": (
            '"historical_unrelated_trades_ignored": true',
            '"historical_unrelated_trades_ignored": false',
        ),
        "semantic-terminal-refresh": (
            '"semantic_terminal_refresh_excludes_receipt_timestamp_and_unrealized_pnl": true',
            '"semantic_terminal_refresh_excludes_receipt_timestamp_and_unrealized_pnl": false',
        ),
        "working-position-completeness": (
            '"working_order_requires_complete_exact_pre_position": true',
            '"working_order_requires_complete_exact_pre_position": false',
        ),
        "owning-grst-count": (
            '"owning_grst_witness_count": 12', '"owning_grst_witness_count": 11',
        ),
        "minimum-negative-count": (
            '"minimum_negative_mutation_count": 195', '"minimum_negative_mutation_count": 194',
        ),
    }
    for name, (old, new) in r3_policy_mutations.items():
        values.append((f"drift-r3-policy-{name}",
                       lambda root, old=old, new=new: replace_once(
                           root, CONTRACT, old, new)))
    for key, value in checker.R4_POLICY.items():
        encoded = json.dumps(value)
        replacement = json.dumps(not value if isinstance(value, bool) else value - 1)
        values.append((f"drift-r4-policy-{key}", lambda root, key=key,
                       encoded=encoded, replacement=replacement: replace_once(
                           root, CONTRACT, f'    "{key}": {encoded}',
                           f'    "{key}": {replacement}')))
    values.extend([
        ("remove-global-history-partition-helper", lambda root: replace_once(
            root, REDUCER, "fn stage5g_global_history_partition(",
            "fn removed_stage5g_global_history_partition(")),
        ("remove-canonical-position-observation-helper", lambda root: replace_once(
            root, REDUCER, "fn stage5g_canonical_position_observation(",
            "fn removed_stage5g_canonical_position_observation(")),
        ("remove-reduction-history-counter-propagation", lambda root: replace_once(
            root, REDUCER, "fn with_history_counts(", "fn removed_with_history_counts(")),
        ("remove-cancel-target-authority-type", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) struct Stage5gCancelTargetOrderAuthority",
            "pub(crate) struct RemovedStage5gCancelTargetOrderAuthority")),
        ("remove-immutable-order-payload-comparator", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) fn stage5g_immutable_order_payload_matches(",
            "pub(crate) fn removed_stage5g_immutable_order_payload_matches(")),
        ("remove-immutable-order-payload-commitment", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) fn stage5g_immutable_order_payload_commitment_sha256(",
            "pub(crate) fn removed_stage5g_immutable_order_payload_commitment_sha256(")),
        ("remove-action-scoped-cancel-authority-slot", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "pub(crate) cancel_target_order_authority: Option<Stage5gCancelTargetOrderAuthority>",
            "pub(crate) removed_cancel_target_order_authority: Option<Stage5gCancelTargetOrderAuthority>")),
    ])
    values.extend([
        *[(name, lambda root: replace_once(
            root, REDUCER,
            "fn stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history()",
            "fn removed_stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history()",
        )) for name in [
            "timer-ignore-old-terminal-order",
            "timer-ignore-old-historical-trade",
            "before-ack-ignore-old-terminal-order",
            "before-ack-ignore-old-historical-trade",
        ]],
        *[(name, lambda root: replace_once(
            root, REDUCER,
            "fn stage5g_edb_r4_no_slot_active_and_unknown_orders_still_block()",
            "fn removed_stage5g_edb_r4_no_slot_active_and_unknown_orders_still_block()",
        )) for name in [
            "timer-active-order-still-blocks",
            "timer-unknown-order-still-blocks",
        ]],
        *[(name, lambda root: replace_once(
            root, REDUCER,
            "fn stage5g_edb_r4_owning_grst06_canonicalizes_both_flat_representations()",
            "fn removed_stage5g_edb_r4_owning_grst06_canonicalizes_both_flat_representations()",
        )) for name in [
            "treat-complete-empty-flat-as-conflict",
            "treat-explicit-zero-flat-as-conflict",
            "compare-flat-avg-price",
        ]],
        ("derive-cancel-target-client-from-command-event", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "let target_client_order_id = target_order",
            "let target_client_order_id = Some(slot.ack.expected_client_order_id.clone());\n                        let _forbidden_target_order = target_order")),
        ("require-cancel-command-client-as-target-client", lambda root: replace_once(
            root, REDUCER,
            ".map(|expected| order.client_order_id.as_ref() == Some(expected))\n                    .unwrap_or(true)",
            ".map(|expected| order.client_order_id.as_ref() == Some(expected))\n                    .unwrap_or(order.client_order_id.as_ref() == Some(&candidate.command_client_order_id))")),
        ("drop-cancel-target-broker-authority", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "target_broker_order_id: target_order_id.clone(),",
            "target_broker_order_id: BrokerOrderId::new(\"FORGED\"),")),
        ("drop-immutable-side-check", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE), "side: order.side,",
            "side: OrderSide::Buy,")),
        ("drop-immutable-qty-check", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "qty: canonical_decimal_v1(order.qty),", "qty: \"ignored\".to_owned(),")),
        ("drop-immutable-order-type-check", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE), "order_type: order.order_type,",
            "order_type: OrderType::Market,")),
        ("drop-immutable-tif-check", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE), "time_in_force: order.time_in_force,",
            "time_in_force: None,")),
        ("drop-immutable-limit-price-check", lambda root: replace_once(
            root, str(checker.ORDER_POSITION_SOURCE),
            "limit_price: order.limit_price.map(canonical_decimal_v1),",
            "limit_price: None,")),
        ("remove-reduction-level-history-count", lambda root: replace_first(
            root, REDUCER, "ignored_unrelated_terminal_order_count: usize,",
            "removed_unrelated_terminal_order_count: usize,")),
    ])
    return values


def run_case(name: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix=f"stage5g-edb-{name}-") as directory:
        root = Path(directory)
        for relative in FILES:
            source = ROOT / relative
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        mutation(root)
        result = subprocess.run(
            ["python3", str(CHECKER), "--root", str(root), "--skip-git"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        if result.returncode == 0:
            raise SystemExit(f"stage5g-edb-negative: FAIL: mutation survived: {name}")
        print(f"PASS {name}")


def main() -> None:
    matrix = cases()
    if len(matrix) < 225 or len({name for name, _ in matrix}) != len(matrix):
        raise SystemExit("stage5g-edb-negative: FAIL: matrix count/names invalid")
    for name, mutation in matrix:
        run_case(name, mutation)
    print(f"stage5g-edb-negative: PASS ({len(matrix)}/{len(matrix)})")


if __name__ == "__main__":
    main()
