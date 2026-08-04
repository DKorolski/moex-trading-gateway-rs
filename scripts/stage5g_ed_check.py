#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-d-a."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
INVENTORY = Path("docs/stage-5/stage5g-lifecycle-entry-inventory.json")
GATE = Path("scripts/stage5g_ed_gate.sh")
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

EXPECTED_DISPOSITIONS = [
    "ExactReplay",
    "ContinueFromCommittedCheckpoint",
    "ApplyOwnedCandidate",
    "AwaitFreshBrokerTruth",
    "ReconciliationRequired",
    "ManualInterventionRequired",
    "TerminalInconsistency",
]

IDENTITY_FIELDS = [
    "broker_id",
    "account_id",
    "strategy_definition_id",
    "strategy_instance_id",
    "deployment_id",
    "deployment_generation",
    "gateway_instance_id",
    "config_fingerprint_sha256",
    "instrument_map_fingerprint_sha256",
    "market_data_generation",
    "command_consumer_generation",
    "target_instrument",
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-ed-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-ed-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain a JSON object")
    return value


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()

    for relative in (SOURCE, LIB, CONTRACT, DESIGN, INVENTORY, GATE):
        require((root / relative).is_file(), f"missing {relative}")

    source = (root / SOURCE).read_text()
    lib = (root / LIB).read_text()
    design = (root / DESIGN).read_text()
    contract = load_json(root / CONTRACT)
    inventory = load_json(root / INVENTORY)
    gate = (root / GATE).read_text()

    restart_family = next(
        (family for family in inventory.get("scenario_families", []) if family.get("id") == "RESTART"),
        None,
    )
    require(restart_family is not None, "RESTART family missing from frozen inventory")
    require(restart_family.get("case_ids") == EXPECTED_IDS, "frozen inventory GRST IDs drifted")

    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart_scenarios must be a list")
    require([row.get("id") for row in scenarios] == EXPECTED_IDS, "contract GRST IDs/order drifted")
    require(contract.get("restart_scenario_count") == 12, "restart_scenario_count must remain 12")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-a must not claim executed GRST cases")
    require(contract.get("dispositions") == EXPECTED_DISPOSITIONS, "typed disposition vocabulary drifted")
    require(contract.get("operational_identity_fields") == IDENTITY_FIELDS, "operational identity fields drifted")

    contract_shape = contract.get("contract", {})
    for flag in (
        "package_identity_required",
        "fresh_snapshot_epoch_required",
        "captured_after_clean_restore_required",
        "exact_operational_identity_required",
        "orders_completeness_explicit",
        "trades_completeness_explicit",
        "positions_completeness_explicit",
        "canonical_rows_required",
    ):
        require(contract_shape.get(flag) is True, f"contract flag {flag} must be true")
    require(contract_shape.get("incomplete_section_means_absent_rows") is False,
            "incomplete sections must not mean absent rows")
    require(contract_shape.get("validated_package_serializable") is False,
            "validated package must stay non-serializable")
    require(contract_shape.get("validated_package_owns_callback_authority") is False,
            "validated package must own no callback authority")

    required_source_tokens = [
        "struct Stage5gFreshBrokerTruthPackageV1",
        "struct Stage5gOperationalIdentityV1",
        "struct Stage5gValidatedFreshBrokerTruthPackage",
        "struct Stage5gFreshBrokerTruthValidationContext",
        "previous_package_id",
        "previous_snapshot_epoch",
        "clean_restore_completed_at",
        "package.captured_at <= context.clean_restore_completed_at",
        "&package.operational_identity != context.expected_operational_identity",
        "orders_complete: bool",
        "trades_complete: bool",
        "positions_complete: bool",
        "fn all_sections_complete",
        "enum Stage5gRestartReconciliationDisposition",
        "enum Stage5gRestartScenarioId",
    ] + EXPECTED_IDS + EXPECTED_DISPOSITIONS
    for token in required_source_tokens:
        require(token in source, f"missing source contract token: {token}")

    require("mod stage5g_fresh_broker_truth;" in lib, "private e-d-a module not registered")
    require("pub mod stage5g_fresh_broker_truth" not in lib, "e-d-a module became public")
    require("pub use stage5g_fresh_broker_truth" not in lib, "e-d-a API leaked from crate root")
    require("pub fn " not in source, "e-d-a introduced a public function")

    for forbidden in (
        "use redis",
        "redis::",
        "reqwest::",
        ".post(",
        ".delete(",
        "finam_client",
        "HybridIntradayRuntimeStrategy",
        "Stage5gCleanRestartedCapability",
        "on_bar(",
        "on_timer(",
    ):
        require(forbidden not in source, f"forbidden e-d-a authority/surface: {forbidden}")

    for case_id in EXPECTED_IDS:
        require(case_id in design, f"design does not map {case_id}")
    require("No function in e-d-a returns one of these dispositions" in design,
            "design must keep reducer deferred")
    require(f'accepted_ec_ref="{ACCEPTED_EC_REF}"' in gate,
            "accepted e-c predecessor ref drifted")
    require('git worktree add --detach "$snapshot_root" "$accepted_ec_ref"' in gate,
            "accepted e-c gate must run from a detached Git worktree")
    require('(\n  cd "$snapshot_root"\n  bash scripts/stage5g_ec_gate.sh\n)' in gate,
            "accepted e-c gate invocation drifted")

    closed = contract.get("closed_surfaces", {})
    require(closed and all(value is False for value in closed.values()),
            "every listed closed surface must remain false")
    print("stage5g-ed-check: PASS")


if __name__ == "__main__":
    main()
