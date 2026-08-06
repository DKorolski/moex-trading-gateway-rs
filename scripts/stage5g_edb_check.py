#!/usr/bin/env python3
"""Current-head contract and closed-surface checker for Stage 5G-e-d-b R5."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


BASE_REF = "66c5fbd2518ec2e7398c88bb59cc7e4dae3ce1bd"
ACCEPTED_R6_REF = "4ece2c7c83ca5575dbca306b5fa29a48dae2bd47"
CONTRACT = Path("docs/stage-5/stage5g-e-d-b-reducer-contract.json")
MAIN_CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
REDUCER_DOC = Path("docs/stage-5/stage5g-e-d-b-reducer-contract.md")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
PARENT_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
REDUCER_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
CLEAN_RESTART_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
ORDER_POSITION_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")

SCENARIOS = [
    "GRST01_RESTART_BEFORE_ACK",
    "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER",
    "GRST03_RESTART_WITH_WORKING_ORDER",
    "GRST04_RESTART_AFTER_PARTIAL_FILL",
    "GRST05_RESTART_FILLED_BEFORE_POSITION",
    "GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED",
    "GRST07_RESTART_AT_TIMER_CHECKPOINT",
    "GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW",
    "GRST09_EXACT_REPLAY_IS_IDEMPOTENT",
    "GRST10_CONFLICTING_REPLAY_BLOCKS",
    "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT",
    "GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION",
]
DISPOSITIONS = [
    "ExactReplay", "ContinueFromCommittedCheckpoint", "ApplyOwnedCandidate",
    "AwaitFreshBrokerTruth", "ReconciliationRequired", "ManualInterventionRequired",
    "TerminalInconsistency",
]
REASONS = [
    "FreshWorkingOrderMatched", "FreshTerminalOrderMatched", "PartialFillPositionConverged",
    "TerminalPositionAlreadyApplied", "TimerCheckpointExact",
    "GeneratedIntentEscrowRetained", "OrdersTruthIncomplete", "TradesTruthIncomplete",
    "PositionsTruthIncomplete", "AuthoritativeOrderMissing",
    "ClientOrderIdentityConflict", "BrokerOrderIdentityConflict", "TradeIdentityConflict",
    "PositionQuantityMismatch", "PositionDirectionMismatch", "UnexpectedTargetPosition",
    "ReplayFingerprintConflict", "HistoricalReplayNotAccepted",
    "ReplayTupleNotInRestartLedger", "AccountWideActiveOrderConflict",
    "AccountWideUnknownOrderConflict", "AmbiguousOwnedOrderSet",
    "SourceOrderActionConflict", "SourceLimitPriceAuthorityUnsupported",
    "SourceLimitPriceMismatch", "CancelTargetClientIdentityConflict",
    "TargetOrderIdentityConflict", "OrderTerminalRegression", "FilledQuantityRegression",
    "CommittedTradeMissing", "CommittedTradePayloadConflict",
    "TargetInstrumentIdentityConflict", "SourceNumericAuthorityUnsupported",
    "OperationalIdentityConflict",
    "UnsupportedLifecycleCombination", "TerminalContradiction",
]
CROSS_BINDINGS = [
    "broker_id", "account_id", "strategy_definition_id", "strategy_instance_id",
    "deployment_id", "deployment_generation", "gateway_instance_id",
    "config_fingerprint_sha256", "instrument_map_fingerprint_sha256",
    "market_data_generation", "command_consumer_generation", "target_instrument_exact",
    "reconstructed_runtime_state_fingerprint_sha256", "restart_replay_commitment_sha256",
    "request_client_broker_identity",
]
CLOSED_SURFACES = {
    "strategy_callback": False,
    "runtime_mutation": False,
    "stage5d_persistence_mutation": False,
    "candidate_application": False,
    "redis": False,
    "finam": False,
    "http_post_delete": False,
    "broker_dispatch": False,
    "runtime_live": False,
    "real_orders": False,
    "stage5g_e_d_c": False,
    "stage5g_f": False,
    "stage6": False,
}
EXPECTED_DELTA = [
    ("M", "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs"),
    ("M", "crates/strategy-runtime-core/src/stage5g_order_position.rs"),
    ("M", "docs/current-status.md"),
    ("M", "docs/reviewer-onboarding-and-roadmap.md"),
    ("M", "docs/stage-5/stage5g-e-d-b-reducer-contract.json"),
    ("M", "docs/stage-5/stage5g-e-d-b-reducer-contract.md"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md"),
    ("M", "scripts/make_stage5g_ed_handoff_archive.py"),
    ("M", "scripts/stage5g_edb_check.py"),
    ("M", "scripts/stage5g_edb_gate.sh"),
    ("M", "scripts/stage5g_edb_negative_harness.py"),
    ("M", "scripts/stage5g_edb_preseal_check.py"),
    ("A", "scripts/stage5g_edb_r5_gate.sh"),
]

R4_POLICY = {
    "global_history_partition_before_grst01_grst07": True,
    "no_slot_terminal_orders_are_historical": True,
    "no_slot_trades_are_historical_after_exact_position": True,
    "history_counts_exist_without_candidate": True,
    "complete_empty_and_explicit_zero_are_flat": True,
    "flat_ignores_avg_price_and_unrealized_pnl": True,
    "cancel_target_client_never_comes_from_command_identity": True,
    "cancel_target_authority_is_action_scoped": True,
    "immutable_target_order_payload_required": True,
    "minimum_negative_mutation_count": 225,
}
R5_POLICY = {
    "status_independent_exact_terminal_grst06": True,
    "exact_terminal_statuses": ["Filled", "Rejected", "Canceled", "Expired"],
    "same_status_canceled_expired_monotonic_advance": True,
    "rejected_positive_fill_forbidden": True,
    "filled_additional_fill_forbidden": True,
    "cross_terminal_status_transition_forbidden": True,
    "committed_trade_subset_and_payload_required": True,
    "fresh_trade_sum_and_position_convergence_required": True,
    "history_counts_on_missing_owned_and_conflict_paths": True,
    "minimum_negative_mutation_count": 265,
}
SEMANTIC_COMPARATOR_CONTRACT = {
    "terminal_order_fields": [
        "account_id", "instrument", "broker_order_id", "client_order_id", "side",
        "order_type", "time_in_force", "qty", "limit_price", "broker_asset_id",
        "board", "expiration_date", "status", "lifecycle", "filled_qty", "remaining_qty",
    ],
    "non_terminal_immutable_order_fields": [
        "account_id", "instrument", "broker_order_id", "client_order_id", "side",
        "order_type", "time_in_force", "qty", "limit_price", "broker_asset_id",
        "board", "expiration_date",
    ],
    "position_fields": ["account_id", "instrument", "qty", "avg_price_non_flat_only"],
    "trade_immutable_fields": [
        "account_id", "broker_trade_id", "broker_order_id", "client_order_id",
        "instrument", "side", "qty", "price", "gross_amount", "commission",
        "broker_asset_id", "board", "expiration_date", "source_ts",
    ],
    "excluded_observation_fields": [
        "order.source_ts", "order.received_ts", "position.source_ts",
        "position.received_ts", "position.unrealized_pnl", "trade.received_ts",
    ],
}

OWNING_SCENARIO_WITNESSES = [
    ("GRST01_RESTART_BEFORE_ACK", "stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history", "Grst01RestartBeforeAck"),
    ("GRST02_RESTART_AFTER_ACK_BEFORE_ORDER", "stage5g_edb_r1_owning_remaining_grst_paths_are_fail_closed_or_noop", "Grst02RestartAfterAckBeforeOrder"),
    ("GRST03_RESTART_WITH_WORKING_ORDER", "stage5g_edb_r3_owning_grst03_runs_full_authenticated_path", "Grst03RestartWithWorkingOrder"),
    ("GRST04_RESTART_AFTER_PARTIAL_FILL", "stage5g_edb_r1_owning_awaiting_runs_export_decode_restore_validate_bind_reduce", "Grst04RestartAfterPartialFill"),
    ("GRST05_RESTART_FILLED_BEFORE_POSITION", "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing", "Grst05RestartFilledBeforePosition"),
    ("GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED", "stage5g_edb_r4_owning_grst06_canonicalizes_both_flat_representations", "Grst06RestartAfterTerminalPositionApplied"),
    ("GRST07_RESTART_AT_TIMER_CHECKPOINT", "stage5g_edb_r4_owning_grst01_and_grst07_ignore_complete_harmless_history", "Grst07RestartAtTimerCheckpoint"),
    ("GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW", "stage5g_edb_r1_owning_generated_intent_escrow_is_retained", "Grst08RestartWithGeneratedIntentEscrow"),
    ("GRST09_EXACT_REPLAY_IS_IDEMPOTENT", "stage5g_edb_r1_owning_exact_current_and_historical_replay_are_noops", "Grst09ExactReplayIsIdempotent"),
    ("GRST10_CONFLICTING_REPLAY_BLOCKS", "stage5g_edb_r1_owning_remaining_grst_paths_are_fail_closed_or_noop", "Grst10ConflictingReplayBlocks"),
    ("GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT", "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing", "Grst11FreshBrokerTruthOverridesStaleHint"),
    ("GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION", "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing", "Grst12MissingOrAmbiguousTruthRequiresReconciliation"),
]
ORDER_CORRELATIONS = [
    "ExactOwned", "ConflictingOwnedIdentity", "UnrelatedTerminal",
    "NonOwnedActive", "NonOwnedUnknown",
]
TRADE_LINKAGES = ["Exact", "Unrelated", "Conflict"]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-edb-check: FAIL: {message}")


def load_json(root: Path, relative: Path) -> dict:
    try:
        value = json.loads((root / relative).read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-edb-check: FAIL: cannot read {relative}: {error}") from error
    require(isinstance(value, dict), f"{relative} must contain an object")
    return value


def enum_variants(source: str, name: str) -> list[str]:
    match = re.search(rf"enum\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    require(match is not None, f"enum {name} missing")
    return re.findall(r"(?m)^\s{4}([A-Z][A-Za-z0-9_]*)\s*,\s*$", match.group(1))


def struct_fields(source: str, name: str) -> list[str]:
    match = re.search(rf"struct\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    require(match is not None, f"struct {name} missing")
    return re.findall(r"(?m)^\s{4}(?:pub\(crate\)\s+)?([a-z][a-z0-9_]*)\s*:", match.group(1))


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact_git_delta(root: Path) -> list[tuple[str, str]]:
    output = subprocess.check_output(
        ["git", "diff", "--name-status", f"{BASE_REF}..HEAD"], cwd=root, text=True
    ).strip()
    rows: list[tuple[str, str]] = []
    for line in output.splitlines():
        fields = line.split("\t")
        require(len(fields) == 2 and fields[0] in {"A", "M"}, f"bad delta row: {line}")
        rows.append((fields[0], fields[1]))
    return rows


def check(root: Path, check_git: bool) -> None:
    for relative in [
        CONTRACT, MAIN_CONTRACT, DESIGN, REDUCER_DOC, STATUS, ONBOARDING,
        PARENT_SOURCE, REDUCER_SOURCE, CLEAN_RESTART_SOURCE, ORDER_POSITION_SOURCE,
        Path("scripts/make_stage5g_ed_handoff_archive.py"),
        Path("scripts/stage5g_edb_gate.sh"), Path("scripts/stage5g_edb_negative_harness.py"),
        Path("scripts/stage5g_edb_preseal_check.py"),
        Path("scripts/stage5g_edb_r3_gate.sh"), Path("scripts/stage5g_edb_r4_gate.sh"),
        Path("scripts/stage5g_edb_r5_gate.sh"),
    ]:
        require((root / relative).is_file() and not (root / relative).is_symlink(),
                f"required regular file missing: {relative}")

    if check_git:
        require(subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=root, text=True).strip() == BASE_REF,
                "HEAD must be one direct successor to rejected e-d-b R4")
        require(exact_git_delta(root) == EXPECTED_DELTA, "exact e-d-b changed-path allowlist drifted")

    contract = load_json(root, CONTRACT)
    require(contract.get("stage") == "5G-e-d-b-r5", "contract stage drifted")
    require(contract.get("accepted_base") == BASE_REF, "accepted base drifted")
    require(contract.get("owning_entry_point") == "reduce_stage5g_fresh_broker_truth",
            "owning reducer entry drifted")
    require(contract.get("scenario_ids") == SCENARIOS, "scenario order/content drifted")
    require(contract.get("dispositions") == DISPOSITIONS, "dispositions drifted")
    require(contract.get("reasons") == REASONS, "reason taxonomy drifted")
    require(contract.get("cross_bindings") == CROSS_BINDINGS, "cross-bindings drifted")
    require(contract.get("closed_surfaces") == CLOSED_SURFACES, "closed surfaces drifted")
    determinism = contract.get("determinism")
    require(isinstance(determinism, dict) and determinism == {
        "frozen_order_count": 12,
        "sequential": True,
        "canonical_row_order": True,
        "parallel_no_shared_state": True,
        "owning_input_evidence_byte_identical": True,
        "multi_trade_row_order_canonical": True,
        "wall_clock_reads": False,
        "exact_replay_semantic_noop": False,
        "exact_replay_disabled_without_authenticated_ledger": True,
        "integral_source_lots_only": True,
    }, "determinism contract drifted")
    require(contract.get("r3_policy") == {
        "source_limit_price_authority": "fail_closed_until_canonical_decimal_tick_authority",
        "cancel_command_and_target_identity_separated": True,
        "historical_terminal_orders_ignored_after_account_wide_safety": True,
        "historical_unrelated_trades_ignored": True,
        "semantic_terminal_refresh_excludes_receipt_timestamp_and_unrealized_pnl": True,
        "working_order_requires_complete_exact_pre_position": True,
        "owning_grst_witness_count": 12,
        "minimum_negative_mutation_count": 195,
    }, "R3 policy contract drifted")
    require(contract.get("r4_policy") == R4_POLICY, "R4 policy contract drifted")
    require(contract.get("r5_policy") == R5_POLICY, "R5 policy contract drifted")
    require(contract.get("semantic_comparator_contract") == SEMANTIC_COMPARATOR_CONTRACT,
            "semantic comparator contract drifted")

    main = load_json(root, MAIN_CONTRACT)
    require(main.get("stage") == "5G-e-d-b-r5", "main contract stage drifted")
    require(main.get("rejected_stage5g_e_d_b_r4_commit") == BASE_REF,
            "main contract rejected R4 binding drifted")
    require(main.get("accepted_stage5g_e_d_a_r6_commit") == ACCEPTED_R6_REF,
            "main contract R6 binding drifted")
    require(main.get("implemented_restart_case_ids") == SCENARIOS,
            "implemented GRST list drifted")
    require(main.get("next_slice") == "5G-e-d-c_after_independent_e_d_b_acceptance",
            "next slice drifted")
    require(main.get("closed_surfaces", {}).get("reconciliation_reducer") is True,
            "main contract does not open only reducer")
    for key in [
        "global_history_partition_before_no_slot_decisions_required",
        "canonical_flat_position_required",
        "action_scoped_cancel_target_authority_required",
        "immutable_target_order_monotonicity_required",
        "reduction_level_history_evidence_required",
        "status_independent_exact_terminal_idempotency_required",
        "same_status_terminal_late_fill_subset_required",
        "terminal_status_transition_fail_closed_required",
        "missing_owned_history_evidence_required",
    ]:
        require(main.get("contract", {}).get(key) is True,
                f"main R4 contract property drifted: {key}")
    for key, expected in CLOSED_SURFACES.items():
        if key in main.get("closed_surfaces", {}):
            require(main["closed_surfaces"][key] is expected, f"main closed surface drifted: {key}")

    reducer = (root / REDUCER_SOURCE).read_text()
    parent = (root / PARENT_SOURCE).read_text()
    clean = (root / CLEAN_RESTART_SOURCE).read_text()
    order_position = (root / ORDER_POSITION_SOURCE).read_text()
    require(parent.count("mod reducer;") == 1, "child reducer registration drifted")
    require(reducer.count("pub(crate) fn reduce_stage5g_fresh_broker_truth(") == 1,
            "there must be exactly one owning reducer entry")
    require(enum_variants(reducer, "Stage5gFreshTruthReductionReason") == REASONS,
            "Rust reason taxonomy drifted")
    require(enum_variants(order_position, "Stage5gOrderOwnershipCorrelation")
            == ORDER_CORRELATIONS, "order correlation partition drifted")
    require(enum_variants(order_position, "Stage5gTradeOrderLinkage")
            == TRADE_LINKAGES, "trade linkage partition drifted")
    require(struct_fields(order_position, "Stage5gCanonicalImmutableOrderPayloadV1") == [
        "schema_version", "domain", "account_id", "broker_order_id", "client_order_id",
        "instrument", "side", "order_type", "time_in_force", "qty", "limit_price",
        "broker_asset_id", "board", "expiration_date",
    ], "immutable order comparator source fields drifted")
    require(struct_fields(order_position, "Stage5gCanonicalImmutableTradePayloadV1") == [
        "schema_version", "domain", "account_id", "broker_trade_id", "broker_order_id",
        "client_order_id", "instrument", "side", "qty", "price", "gross_amount",
        "commission", "broker_asset_id", "board", "expiration_date", "source_ts",
    ], "immutable trade comparator source fields drifted")
    require("_restart: Stage5gCleanRestartedCapability" in reducer
            and "_truth: Stage5gRestartBoundFreshBrokerTruthPackage" in reducer,
            "linear input ownership retention drifted")
    require("candidate: Option<Stage5gOwnedReconciliationCandidate>" in reducer,
            "opaque candidate ownership drifted")
    require("fn candidate_is_self_consistent(" in reducer,
            "candidate self-consistency validation missing")
    require("fn cross_binding_matches(" in reducer, "cross-binding function missing")
    for marker in [
        "Stage5gRestartBoundFreshBrokerTruthPackage",
        "stage5g_operational_binding_commitment(",
        "stage5g_restart_replay_commitment(",
        "restart.account_id == package.operational_identity.account_id",
        "restart.strategy_id == package.operational_identity.strategy_definition_id.as_str()",
        "restart.config_fingerprint_sha256",
        "restart.instrument_id == package.operational_identity.target_instrument",
        "ReplayFingerprintConflict",
        "stage5g_exact_trade_order_linkage(order, trade)",
        "stage5g_expected_post_position_qty(slot.pre_position_qty, order)",
        "stage5g_intent_position_is_compatible(",
        "OrderStatus::New | OrderStatus::Working",
        "OrderStatus::PartiallyFilled",
        "OrderStatus::Filled",
        "OrderStatus::Rejected",
        "OrderStatus::Canceled | OrderStatus::Expired",
        "positions_complete",
        "generated_intent_escrow_fingerprint_sha256",
        "stage5g_account_wide_order_safety(",
        "Stage5gAccountWideOrderSafety::NonOwnedActive",
        "Stage5gAccountWideOrderSafety::NonOwnedUnknown",
        "Stage5gAccountWideOrderSafety::AmbiguousOwned",
        "stage5g_order_matches_source_action(&slot.source_action, order)",
        "SourceLimitPriceAuthorityUnsupported",
        "Stage5gOrderOwnershipCorrelation::ExactOwned",
        "Stage5gOrderOwnershipCorrelation::ConflictingOwnedIdentity",
        "Stage5gOrderOwnershipCorrelation::UnrelatedTerminal",
        "ignored_unrelated_terminal_order_count",
        "ignored_unrelated_historical_trade_count",
        "stage5g_semantic_terminal_order_matches(",
        "stage5g_semantic_position_matches(",
        "let source_price_authority_is_supported = !matches!(",
        "source_to_fresh_progress(",
        "Stage5gSourceFreshProgress::ExactCommittedTerminal",
        "stage5g_immutable_trade_payload_matches(committed, fresh)",
        "TargetInstrumentIdentityConflict",
        "SourceNumericAuthorityUnsupported",
        "Stage5gFreshPackageLineage::ReplayTupleNotInRestartLedger =>",
        "Stage5gFreshPackageLineage::HistoricalReplayNotAccepted =>",
        "committed_order.lifecycle == BrokerOrderLifecycle::Terminal",
        "fresh_order.filled_qty < committed_order.filled_qty",
        "for committed in &slot.trades",
        "&& progress == Stage5gSourceFreshProgress::ExactCommittedTerminal",
        "!restart.committed_position_numeric_authority_is_integral",
        "stage5g_global_history_partition(",
        "stage5g_canonical_position_observation(",
        ".with_history_counts(",
        "stage5g_immutable_order_payload_matches(committed_order, fresh_order)",
        "immutable_target_order_monotonicity_proven",
        "global_account_history_partition_proven",
        "canonical_position_semantics_proven",
        "cancel_target_order_authority",
        "same_status_terminal_advance_allowed",
        "added_trade_set_is_complete",
        "fresh_trade_sum == fresh_order.filled_qty",
        "observed_fresh_position == Some(expected_fresh_position)",
        "source_intent_remains_compatible",
        "exact_terminal_shape_is_compatible",
    ]:
        require(marker in reducer, f"reducer invariant anchor missing: {marker}")
    for marker, minimum in [
        ("stage5g_operational_binding_commitment(", 2),
        ("OrderStatus::New | OrderStatus::Working => {", 2),
        ("OrderStatus::PartiallyFilled => {", 2),
        ("OrderStatus::Filled => {", 2),
        ("OrderStatus::Rejected => {", 2),
        ("OrderStatus::Canceled | OrderStatus::Expired => {", 2),
        ("if !truth.positions_complete {", 6),
    ]:
        require(reducer.count(marker) >= minimum,
                f"reducer repeated safety anchor weakened: {marker}")
    for marker in [
        "pub(crate) struct Stage5gFreshTruthReplayHintsV1",
        "pub(crate) struct Stage5gFreshTruthOperationalAuthority",
        "pub(crate) struct Stage5gReviewedOperationalIdentityAuthority",
        "pub(crate) struct Stage5gRestartBoundFreshBrokerTruthPackage",
        "pub(crate) fn bind_stage5g_fresh_truth_to_clean_restart(",
        "restart_replay_hints_match_checkpoint(",
    ]:
        require(marker in parent, f"restart-owned authority anchor missing: {marker}")
    for field in CROSS_BINDINGS[:11]:
        require(field in parent, f"full operational identity field missing: {field}")
    require("pub(crate) struct Stage5gFreshTruthRestartProjection" in clean,
            "narrow restart projection missing")
    require("pub(crate) struct Stage5gFreshTruthRestartSlotProjection" in order_position,
            "narrow slot projection missing")
    for marker in [
        "command_request_id: String",
        "command_client_order_id: ClientOrderId",
        "target_broker_order_id: Option<BrokerOrderId>",
        "target_order_client_order_id: Option<ClientOrderId>",
        "intent_class: Stage5gRestartIntentClass",
        "pre_position_qty: Decimal",
        "target_qty: Option<Decimal>",
        "source_action: Stage5gMockIntentAction",
        "source_numeric_authority_is_integral: bool",
        "pub(crate) fn stage5g_exact_trade_order_linkage(",
        "pub(crate) fn stage5g_order_ownership_correlation(",
        "pub(crate) fn stage5g_account_wide_order_safety(",
        "pub(crate) fn stage5g_order_matches_source_action(",
        "pub(crate) fn stage5g_integral_lot_decimal(",
        "pre_position_qty + signed_fill",
        "pub(crate) struct Stage5gCancelTargetOrderAuthority",
        "pub(crate) cancel_target_order_authority: Option<Stage5gCancelTargetOrderAuthority>",
        "pub(crate) fn stage5g_immutable_order_payload_matches(",
        "pub(crate) fn stage5g_immutable_order_payload_commitment_sha256(",
    ]:
        require(marker in order_position, f"canonical slot/linkage anchor missing: {marker}")
    require(order_position.count("pre_position_qty + signed_fill") >= 2,
            "source-relative position formula coverage weakened")
    require("OrderStatus::New | OrderStatus::Working => {\n            order.filled_qty == Decimal::ZERO\n                && candidate.trades.is_empty()\n                && candidate.positions_complete"
            in reducer, "candidate Working/New position completeness drifted")
    semantic_order = reducer.split("pub(crate) fn stage5g_semantic_terminal_order_matches(", 1)[1].split(
        "pub(crate) fn stage5g_semantic_position_matches(", 1
    )[0]
    semantic_position = reducer.split("pub(crate) fn stage5g_semantic_position_matches(", 1)[1].split(
        "fn source_to_fresh_progress(", 1
    )[0]
    require("received_ts" not in semantic_order and "source_ts" not in semantic_order,
            "semantic order equality includes observation chronology")
    require("received_ts" not in semantic_position and "unrealized_pnl" not in semantic_position,
            "semantic position equality includes volatile observation fields")

    for type_name, source in [
        ("Stage5gFreshTruthReduction", reducer),
        ("Stage5gOwnedReconciliationCandidate", reducer),
        ("Stage5gFreshTruthOperationalAuthority", parent),
        ("Stage5gReviewedOperationalIdentityAuthority", parent),
    ]:
        prefix = source.split(f"struct {type_name}", 1)[0][-240:]
        require(not re.search(r"derive\([^)]*(Clone|Copy|Serialize|Deserialize|Default)", prefix),
                f"linear authority type gained forbidden derive: {type_name}")
    authorizer_signature = parent.split(
        "pub(crate) fn authorize_stage5g_fresh_truth_operational_identity(", 1
    )[1].split(") -> Result<", 1)[0]
    require("Stage5gReviewedOperationalIdentityAuthority" in authorizer_signature
            and "Stage5gOperationalIdentityInput" not in authorizer_signature,
            "raw operational DTO can still mint final authority")
    require("#[cfg(test)]\npub(super) fn stage5g_test_reviewed_operational_identity_authority(" in parent,
            "reviewed identity test issuer is not test-only")

    production = strip_comments(reducer.split("#[cfg(test)]", 1)[0]).lower()
    for forbidden in [
        "reqwest", "redis::", "finam", "tokio", "utc::now", "systemtime",
        ".post(", ".delete(", "stage5d_export", "stage5c_", "dispatch_order",
        "strategy::on", "intent_sink",
    ]:
        require(forbidden not in production, f"closed production surface opened: {forbidden}")
    classify_body = reducer.split("fn classify(", 1)[1].split("fn disposition_id(", 1)[0]
    require("Stage5gRestartReconciliationDisposition::ExactReplay" not in classify_body,
            "ExactReplay reopened without authenticated fresh replay ledger")
    owning_classify = classify_body.split("fn cross_binding_matches(", 1)[0]
    # Anchor the R5 decision to its named compatibility proof. Earlier helper
    # expressions also match on order.status, so splitting on the first textual
    # match would inspect the wrong region while still compiling successfully.
    exact_terminal_region = owning_classify.split(
        "let exact_terminal_shape_is_compatible = match order.status", 1
    )[1].split("match order.status {", 1)[0]
    require(
        "slot.terminal" in exact_terminal_region
        and "progress == Stage5gSourceFreshProgress::ExactCommittedTerminal"
        in exact_terminal_region
        and all(
            f"OrderStatus::{status}" in exact_terminal_region
            for status in ["Filled", "Rejected", "Canceled", "Expired"]
        ),
        "status-independent exact-terminal GRST06 decision drifted",
    )
    for marker in [
        "untrusted_last_reconciled_hint",
        "untrusted_accepted_replay_hints",
        "untrusted_known_historical_hints",
    ]:
        require(marker in parent, f"replay hint boundary marker missing: {marker}")
    require("last_reconciled_fresh_package" not in parent
            and "accepted_replay_ledger" not in parent
            and "known_historical_fresh_packages" not in parent,
            "caller replay hints still claim authenticated ledger authority")

    tests = reducer.split("#[cfg(test)]", 1)[1]
    for index in range(1, 13):
        require(tests.count(f"fn stage5g_edb_grst{index:02d}()") == 1,
                f"GRST{index:02d} positive test missing/duplicated")
    for witness in [
        "stage5g_edb_matrix_executes_frozen_ids_once_in_order",
        "stage5g_edb_sequential_and_row_order_are_deterministic",
        "stage5g_edb_exact_replay_is_semantic_noop",
        "stage5g_edb_parallel_execution_has_no_shared_mutable_state",
        "stage5g_edb_r1_all_operational_identity_fields_are_commitment_bound",
        "stage5g_edb_r1_restart_replay_commitment_and_conflict_are_enforced",
        "stage5g_edb_r1_exact_trade_linkage_rejects_secondary_id_conflicts",
        "stage5g_edb_r1_source_relative_entry_exit_and_terminal_matrix",
        "stage5g_edb_r1_owning_timer_ready_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_awaiting_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing",
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
        "stage5g_edb_r5_all_exact_terminal_statuses_are_generic_grst06",
        "stage5g_edb_r5_canceled_and_expired_late_fills_are_grst11_candidates",
        "stage5g_edb_r5_terminal_late_fill_evidence_is_canonical_and_parallel",
        "stage5g_edb_r5_terminal_late_fill_regressions_fail_closed",
        "stage5g_edb_r5_rejected_fill_and_terminal_status_transitions_fail_closed",
        "stage5g_edb_r5_missing_owned_and_terminal_conflict_retain_history_counts",
    ]:
        require(tests.count(f"fn {witness}()") == 1, f"focused witness drifted: {witness}")

    require(len(OWNING_SCENARIO_WITNESSES) == 12
            and [item[0] for item in OWNING_SCENARIO_WITNESSES] == SCENARIOS,
            "owning GRST witness inventory drifted")
    for scenario, witness, rust_variant in OWNING_SCENARIO_WITNESSES:
        require(tests.count(f"fn {witness}()") == 1,
                f"owning witness missing for {scenario}: {witness}")
        body = tests.split(f"fn {witness}()", 1)[1].split("\n    #[test]", 1)[0]
        require("reduce_stage5g_fresh_broker_truth(" in body and rust_variant in body,
                f"owning witness does not execute reducer for {scenario}")

    for relative in [DESIGN, REDUCER_DOC, STATUS, ONBOARDING]:
        text = (root / relative).read_text()
        require("c5f84bb" in text, f"rejected e-d-b R2 reference missing: {relative}")
        require("f9bc372" in text, f"rejected e-d-b R3 reference missing: {relative}")
        require("66c5fbd" in text, f"rejected e-d-b R4 reference missing: {relative}")
        require("4ece2c7" in text, f"accepted R6 reference missing: {relative}")
        require("Stage 5G-e-d-c" in text or "e-d-c" in text,
                f"next closed e-d-c boundary missing: {relative}")
    reducer_doc = (root / REDUCER_DOC).read_text()
    for marker in ["classification", "opaque", "callback", "Redis", "FINAM", "runtime-live"]:
        require(marker in reducer_doc, f"reducer documentation marker missing: {marker}")

    source_hashes = contract.get("source_sha256")
    require(isinstance(source_hashes, dict), "source SHA map missing")
    expected_hash_paths = [
        str(REDUCER_SOURCE), str(PARENT_SOURCE), str(CLEAN_RESTART_SOURCE),
        str(ORDER_POSITION_SOURCE),
    ]
    require(list(source_hashes) == expected_hash_paths, "source SHA path/order drifted")
    for relative, expected in source_hashes.items():
        require(sha256(root / relative) == expected, f"source SHA drifted: {relative}")

    print("stage5g-edb-r5-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
