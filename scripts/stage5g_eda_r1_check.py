#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-d-a R1."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
INVENTORY = Path("docs/stage-5/stage5g-lifecycle-entry-inventory.json")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
GATE = Path("scripts/stage5g_eda_r1_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r1_negative_harness.py")
REJECTED_PREDECESSOR = "f44b154753ea8b60a73cfb6ee3b5e487263dcb3b"
ACCEPTED_EC_REF = "b9db87947723cf9c50e64b5fcc3b5ab30e857fd1"

EXPECTED_IDS = [
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


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r1-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r1-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain a JSON object")
    return value


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()
    files = (SOURCE, LIB, CONTRACT, DESIGN, INVENTORY, STATUS, ONBOARDING, GATE, NEGATIVE)
    for relative in files:
        require((root / relative).is_file(), f"missing {relative}")

    source = (root / SOURCE).read_text()
    lib = (root / LIB).read_text()
    design = (root / DESIGN).read_text()
    status = (root / STATUS).read_text()
    onboarding = (root / ONBOARDING).read_text()
    gate = (root / GATE).read_text()
    negative = (root / NEGATIVE).read_text()
    contract = load_json(root / CONTRACT)
    inventory = load_json(root / INVENTORY)

    restart_family = next(
        (row for row in inventory.get("scenario_families", []) if row.get("id") == "RESTART"),
        None,
    )
    require(restart_family is not None, "RESTART family missing")
    require(restart_family.get("case_ids") == EXPECTED_IDS, "frozen GRST inventory drifted")
    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart_scenarios must be a list")
    require([row.get("id") for row in scenarios] == EXPECTED_IDS, "contract GRST order drifted")
    require(contract.get("restart_scenario_count") == 12, "restart count drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b cases must remain deferred")
    require(contract.get("status") == "r1_implementation_review_candidate", "R1 status drifted")
    require(contract.get("rejected_predecessor_commit") == REJECTED_PREDECESSOR,
            "rejected predecessor binding drifted")

    shape = contract.get("contract", {})
    for flag in (
        "section_observation_required",
        "post_restore_row_receipt_required",
        "semantic_position_dedup_required",
        "validated_identity_constructor_only",
        "explicit_remaining_quantity_required",
        "filled_requires_complete_fill",
        "replay_lineage_split_required",
        "changed_fingerprint_conflict_required",
    ):
        require(shape.get(flag) is True, f"contract flag {flag} must be true")
    require(shape.get("validated_package_serializable") is False,
            "validated package must remain non-serializable")
    require(shape.get("validated_package_owns_callback_authority") is False,
            "validated package must own no callback authority")

    exact_validated_derive = (
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n"
        "#[serde(deny_unknown_fields)]\n"
        "pub(crate) struct Stage5gOperationalIdentityV1"
    )
    require(exact_validated_derive in source, "validated identity derive must exclude Deserialize")
    require(
        "#[derive(Debug, Clone, Serialize, Deserialize)]\n"
        "#[serde(deny_unknown_fields)]\n"
        "pub(crate) struct Stage5gOperationalIdentityInput" in source,
        "raw identity DTO must own deserialization",
    )
    required_source_tokens = [
        "orders_observed_at: DateTime<Utc>",
        "trades_observed_at: DateTime<Utc>",
        "positions_observed_at: DateTime<Utc>",
        "observed_at <= clean_restore_completed_at",
        "observed_at > captured_at",
        "order.received_ts < clean_restore_completed_at",
        "trade.received_ts < clean_restore_completed_at",
        "position.received_ts < clean_restore_completed_at",
        "order.received_ts > package.orders_observed_at",
        "trade.received_ts > package.trades_observed_at",
        "position.received_ts > package.positions_observed_at",
        "instrument_identity_matches(&previous.instrument, &position.instrument)",
        "pub(crate) pre_restart_package_id: &'a str",
        "last_reconciled_fresh_package",
        "accepted_replay_ledger",
        "known_historical_fresh_packages",
        "Stage5gFreshPackageLineage::ExactLastReconciledReplay",
        "Stage5gFreshPackageLineage::ExactAcceptedHistoricalReplay",
        "Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict",
        "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n    }\n}\n\nfn validate_replay_ledger(",
        "matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty",
        "order.is_inconsistent_active_zero_remaining()",
        "match order.remaining_qty",
        "fn stale_pre_restore_order_trade_and_position_rows_fail_closed",
        "fn complete_empty_section_requires_post_restore_observation",
        "fn semantic_position_duplicates_and_wildcard_bridge_fail_closed",
        "fn invalid_json_identity_zero_generation_and_hash_fail_closed",
        "fn filled_incomplete_and_working_zero_remaining_fail_closed",
        "fn exact_last_replay_is_eligible_and_changed_fingerprint_conflicts",
        "fn old_non_immediate_replay_requires_bounded_acceptance",
        "fn canonical_fingerprint_is_independent_of_row_order",
        "fn incomplete_section_is_preserved_not_treated_as_absence",
    ]
    for token in required_source_tokens:
        require(token in source, f"missing R1 source/test token: {token}")
    require(source.count("validate_section_observation(") == 4,
            "all three sections must be observed after restore")
    require("previous_package_id" not in source, "ambiguous previous package authority restored")
    require("previous_snapshot_epoch" not in source, "ambiguous previous epoch authority restored")

    require("mod stage5g_fresh_broker_truth;" in lib, "private module missing")
    require("pub mod stage5g_fresh_broker_truth" not in lib, "module leaked publicly")
    require("pub use stage5g_fresh_broker_truth" not in lib, "module re-exported")
    require("pub fn " not in source, "public function introduced")
    for forbidden in (
        "use redis", "redis::", "reqwest::", ".post(", ".delete(", "finam_client",
        "HybridIntradayRuntimeStrategy", "Stage5gCleanRestartedCapability", "on_bar(", "on_timer(",
    ):
        require(forbidden not in source, f"forbidden surface/authority: {forbidden}")

    for token in (
        "clean_restore_completed_at < section_observed_at <= captured_at",
        "same package ID with a changed canonical fingerprint",
        "wildcard venue collision",
        "e-d-b remains closed",
    ):
        require(token in design, f"design missing R1 statement: {token}")
    require("Stage 5G-e-d-a R1" in status, "current status does not identify R1")
    require("Stage 5G-e-d-a R1" in onboarding, "onboarding does not identify R1")
    require("Stage 5G-e-d-b remains closed" in status, "current status opened e-d-b")

    require(f'rejected_eda_ref="{REJECTED_PREDECESSOR}"' in gate,
            "detached f44 predecessor binding drifted")
    require('git worktree add --detach "$snapshot_root" "$rejected_eda_ref"' in gate,
            "inherited e-d-a gate must use a detached worktree")
    require("bash scripts/stage5g_ed_gate.sh" in gate, "inherited e-d-a gate missing")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth" in gate,
            "focused release tests missing")
    require("cargo test -p strategy-runtime-core --lib" in gate, "full core test missing")
    require(ACCEPTED_EC_REF in design, "accepted e-c lineage missing from design")

    required_mutations = [
        "remove-row-lower-freshness-bound",
        "remove-complete-empty-observation-proof",
        "restore-strict-position-key-dedup",
        "restore-unchecked-validated-deserialize",
        "remove-filled-complete-fill-rule",
        "collapse-pre-restart-and-last-reconciled",
        "allow-changed-fingerprint-for-same-package-id",
    ]
    for mutation in required_mutations:
        require(mutation in negative, f"negative mutation missing: {mutation}")

    closed = contract.get("closed_surfaces", {})
    require(closed and all(value is False for value in closed.values()),
            "all closed surfaces must remain false")
    print("stage5g-eda-r1-check: PASS")


if __name__ == "__main__":
    main()
