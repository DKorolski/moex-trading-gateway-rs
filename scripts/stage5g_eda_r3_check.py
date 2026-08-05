#!/usr/bin/env python3
"""Controlling current-HEAD checker for Stage 5G-e-d-a R3."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
INVARIANTS = Path("docs/stage-5/stage5g-e-d-a-r3-current-head-invariants.json")
LIFECYCLE_INVENTORY = Path("docs/stage-5/stage5g-lifecycle-entry-inventory.json")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
GATE = Path("scripts/stage5g_eda_r3_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r3_negative_harness.py")
BUILDER = Path("scripts/make_stage5g_ed_handoff_archive.py")
PRESEAL = Path("scripts/stage5g_eda_r3_preseal_check.py")
R2_REF = "8384a13bc8b7babcb11f6f5bb0f717f1a6c70388"

EXPECTED_GRST_IDS = [
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

EXPECTED_GRST_VARIANTS = [
    "Grst01RestartBeforeAck",
    "Grst02RestartAfterAckBeforeOrder",
    "Grst03RestartWithWorkingOrder",
    "Grst04RestartAfterPartialFill",
    "Grst05RestartFilledBeforePosition",
    "Grst06RestartAfterTerminalPositionApplied",
    "Grst07RestartAtTimerCheckpoint",
    "Grst08RestartWithGeneratedIntentEscrow",
    "Grst09ExactReplayIsIdempotent",
    "Grst10ConflictingReplayBlocks",
    "Grst11FreshBrokerTruthOverridesStaleHint",
    "Grst12MissingOrAmbiguousTruthRequiresReconciliation",
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

EXPECTED_OPERATIONAL_FIELDS = [
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

EXPECTED_CONTEXT_FIELDS = [
    "expected_operational_identity",
    "pre_restart_package_id",
    "pre_restart_snapshot_epoch",
    "last_reconciled_fresh_package",
    "accepted_replay_ledger",
    "known_historical_fresh_packages",
    "clean_restore_completed_at",
    "validation_observed_at",
]

EXPECTED_INVARIANT_IDS = [
    "package_schema_version", "package_id_canonical", "snapshot_epoch_canonical",
    "package_id_not_pre_restart", "snapshot_epoch_not_pre_restart",
    "package_captured_after_restore", "package_captured_by_validation",
    "operational_identity_exact", "orders_section_observed", "trades_section_observed",
    "positions_section_observed", "section_after_restore", "section_by_capture",
    "order_received_after_restore", "trade_received_after_restore",
    "position_received_after_restore", "order_received_by_section",
    "trade_received_by_section", "position_received_by_section", "order_source_by_receipt",
    "trade_source_by_receipt", "position_source_by_receipt", "order_account_bound",
    "trade_account_bound", "position_account_bound", "order_status_lifecycle",
    "order_remaining_exact", "filled_order_complete", "active_zero_remaining_rejected",
    "market_has_no_limit_price", "limit_has_positive_price", "canonical_order_native_ids",
    "canonical_trade_native_ids", "positive_trade_quantity_price",
    "unique_broker_order_id", "unique_client_order_id", "unique_broker_trade_id",
    "semantic_position_identity_unique", "replay_ledger_id_epoch_unique",
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r3-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r3-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain an object")
    return value


def rust_body(source: str, kind: str, name: str) -> str:
    match = re.search(
        rf"pub\(crate\)\s+{kind}\s+{re.escape(name)}(?:<'[^>]+>)?\s*\{{(.*?)\n\}}",
        source,
        re.DOTALL,
    )
    require(match is not None, f"cannot extract Rust {kind} {name}")
    return match.group(1)


def rust_enum_variants(source: str, name: str) -> list[str]:
    variants = []
    for line in rust_body(source, "enum", name).splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("//") and stripped.endswith(","):
            variants.append(stripped[:-1])
    return variants


def rust_struct_fields(source: str, name: str) -> list[str]:
    fields = []
    for line in rust_body(source, "struct", name).splitlines():
        stripped = line.strip()
        if stripped and not stripped.startswith("//") and ":" in stripped:
            fields.append(stripped.split(":", 1)[0].split()[-1])
    return fields


def rust_all_variants(source: str) -> list[str]:
    region = source.split("impl Stage5gRestartScenarioId", 1)[1].split(
        "pub(crate) enum Stage5gFreshBrokerTruthError", 1
    )[0]
    match = re.search(r"pub\(crate\) const ALL: \[Self; 12\] = \[(.*?)\n    \];", region, re.DOTALL)
    require(match is not None, "cannot extract Stage5gRestartScenarioId::ALL")
    return re.findall(r"Self::([A-Za-z0-9_]+)", match.group(1))


def assert_no_reducer_authority(source: str) -> None:
    signatures = re.finditer(
        r"(?:pub(?:\(crate\))?\s+)?(?:const\s+)?fn\s+([A-Za-z0-9_]+)\s*"
        r"(?:<[^>{}]*>)?\s*\((.*?)\)\s*(?:->\s*([^\{]+))?\{",
        source,
        re.DOTALL,
    )
    allowed_test_helpers = {
        "reconciled_identity",
        "conflicting_rows_fail_before_any_reconciliation_authority_exists",
    }
    for match in signatures:
        name, parameters, returns = match.group(1), match.group(2), match.group(3) or ""
        require(
            "Stage5gRestartReconciliationDisposition" not in returns,
            f"function {name} returns reconciliation disposition",
        )
        if "Stage5gValidatedFreshBrokerTruthPackage" in parameters:
            require(name == "reconciled_identity", f"function {name} consumes validated package")
            require(match.start() > source.index("#[cfg(test)]"),
                    "validated-package helper escaped the test module")
        if any(token in name.lower() for token in ("reducer", "reconcile", "apply", "callback")):
            require(name in allowed_test_helpers, f"reducer-like function introduced: {name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    root = parser.parse_args().root.resolve()
    files = (
        SOURCE, LIB, CONTRACT, DESIGN, INVARIANTS, LIFECYCLE_INVENTORY, STATUS,
        ONBOARDING, GATE, NEGATIVE, BUILDER, PRESEAL,
    )
    for relative in files:
        require((root / relative).is_file(), f"missing {relative}")

    source = (root / SOURCE).read_text()
    lib = (root / LIB).read_text()
    design = (root / DESIGN).read_text()
    status = (root / STATUS).read_text()
    onboarding = (root / ONBOARDING).read_text()
    gate = (root / GATE).read_text()
    negative = (root / NEGATIVE).read_text()
    builder = (root / BUILDER).read_text()
    contract = load_json(root / CONTRACT)
    invariants = load_json(root / INVARIANTS)
    lifecycle = load_json(root / LIFECYCLE_INVENTORY)

    require(contract.get("status") == "r3_gate_only_review_candidate", "R3 status drifted")
    require(contract.get("rejected_r2_commit") == R2_REF, "R2 base binding drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b implementation claimed")
    require(contract.get("dispositions") == EXPECTED_DISPOSITIONS,
            "contract disposition vocabulary/order drifted")
    require(contract.get("operational_identity_fields") == EXPECTED_OPERATIONAL_FIELDS,
            "contract operational identity fields/order drifted")
    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart_scenarios must be a list")
    require([row.get("id") for row in scenarios] == EXPECTED_GRST_IDS,
            "contract GRST IDs/order drifted")
    require(contract.get("restart_scenario_count") == 12, "restart scenario count drifted")
    restart_family = next(
        (row for row in lifecycle.get("scenario_families", []) if row.get("id") == "RESTART"),
        None,
    )
    require(restart_family is not None, "RESTART family missing")
    require(restart_family.get("case_ids") == EXPECTED_GRST_IDS, "inventory GRST IDs drifted")

    require(rust_enum_variants(source, "Stage5gRestartReconciliationDisposition")
            == EXPECTED_DISPOSITIONS, "Rust disposition enum drifted")
    require(rust_enum_variants(source, "Stage5gRestartScenarioId") == EXPECTED_GRST_VARIANTS,
            "Rust GRST enum drifted")
    require(rust_all_variants(source) == EXPECTED_GRST_VARIANTS,
            "Stage5gRestartScenarioId::ALL order drifted")
    mapping_region = source.split("impl Stage5gRestartScenarioId", 1)[1].split(
        "pub(crate) enum Stage5gFreshBrokerTruthError", 1
    )[0]
    require(re.findall(r'"(GRST\d\d_[A-Z0-9_]+)"', mapping_region) == EXPECTED_GRST_IDS,
            "Rust frozen GRST ID mapping drifted")
    require(rust_struct_fields(source, "Stage5gOperationalIdentityV1")
            == EXPECTED_OPERATIONAL_FIELDS, "validated identity members drifted")
    require(rust_struct_fields(source, "Stage5gOperationalIdentityInput")
            == EXPECTED_OPERATIONAL_FIELDS, "raw identity members drifted")
    require(rust_struct_fields(source, "Stage5gFreshBrokerTruthValidationContext")
            == EXPECTED_CONTEXT_FIELDS, "validation context authorities drifted")

    rows = invariants.get("invariants")
    require(isinstance(rows, list), "current-head invariant inventory must be a list")
    require([row.get("invariant_id") for row in rows] == EXPECTED_INVARIANT_IDS,
            "current-head invariant inventory drifted")
    require(invariants.get("base_commit") == R2_REF, "invariant inventory base drifted")
    require(invariants.get("implemented_restart_case_ids") == [], "inventory claims GRST execution")
    for row in rows:
        invariant_id = row["invariant_id"]
        anchor = row.get("production_anchor")
        witness = row.get("focused_rust_witness")
        mutation = row.get("negative_mutation_id")
        require(isinstance(anchor, str) and anchor in source,
                f"production anchor missing for {invariant_id}")
        require(isinstance(witness, str) and witness in source,
                f"focused Rust witness missing for {invariant_id}")
        require(isinstance(mutation, str) and f'"{mutation}"' in negative,
                f"negative mutation missing for {invariant_id}")

    exact_validated_derive = (
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n"
        "#[serde(deny_unknown_fields)]\n"
        "pub(crate) struct Stage5gOperationalIdentityV1"
    )
    require(exact_validated_derive in source, "validated identity regained Deserialize")
    require("fn canonical_identity_token(value: &str) -> bool" in source,
            "canonical identity-token helper missing")
    require("character.is_whitespace() || character.is_control()" in source,
            "canonical identity-token grammar weakened")
    require(
        "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n"
        "    }\n}\n\nfn validate_replay_ledger(" in source,
        "same-package changed-fingerprint conflict guard drifted",
    )
    require("previous_package_id" not in source and "previous_snapshot_epoch" not in source,
            "ambiguous replay authorities restored")
    require(
        "if order\n            .broker_order_id\n            .as_ref()\n"
        "            .is_some_and(|id| !canonical_native_id(id.as_str()))" in source,
        "canonical order broker ID guard drifted",
    )

    require("mod stage5g_fresh_broker_truth;" in lib, "private module missing")
    require("pub mod stage5g_fresh_broker_truth" not in lib, "module leaked publicly")
    require("pub use stage5g_fresh_broker_truth" not in lib, "module re-exported")
    require("pub fn " not in source, "public function introduced")
    assert_no_reducer_authority(source)
    for forbidden in (
        "use redis", "redis::", "reqwest::", ".post(", ".delete(", "finam_client",
        "HybridIntradayRuntimeStrategy", "Stage5gCleanRestartedCapability", "on_bar(",
        "on_timer(",
    ):
        require(forbidden not in source, f"forbidden surface/authority: {forbidden}")

    required_mutations = (
        "drop-json-disposition", "rename-rust-disposition", "drop-json-operational-field",
        "drop-rust-operational-field", "restore-trim-only-identity-grammar",
        "restore-unchecked-validated-deserialize", "collapse-replay-authorities",
        "allow-same-id-changed-fingerprint", "public-module-leak", "open-reducer",
        "append-real-source-reducer", "runtime-callback-surface", "redis-surface",
        "swap-grst-all-first-two", "remove-grst-all-entry", "duplicate-grst-all-entry",
        "change-frozen-grst-mapping",
    )
    for mutation in required_mutations:
        require(f'"{mutation}"' in negative, f"required R3 mutation missing: {mutation}")

    require(f'r2_ref="{R2_REF}"' in gate, "detached R2 binding drifted")
    require('git worktree add --detach "$snapshot_root" "$r2_ref"' in gate,
            "R2 gate must run in detached source")
    require("bash scripts/stage5g_eda_r2_gate.sh" in gate, "inherited R2 gate missing")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth" in gate,
            "focused release test missing")
    require("cargo test -p strategy-runtime-core --lib" in gate, "full core test missing")
    require("python3 scripts/stage5g_eda_r3_preseal_check.py" in gate,
            "archive safety preseal missing")
    require(f'REQUIRED_PARENT = "{R2_REF}"' in builder, "R3 builder parent drifted")
    require('BRANCH = "stage5g-lifecycle"' in builder, "R3 builder branch drifted")
    require('STAGE = "5G-e-d-a-r3"' in builder, "R3 builder stage drifted")
    require('["bash", "scripts/stage5g_eda_r3_gate.sh"]' in builder,
            "R3 builder gate drifted")
    require("Stage 5G-e-d-a R3" in status and "Stage 5G-e-d-b remains closed" in status,
            "current status does not preserve R3 boundary")
    require("Stage 5G-e-d-a R3" in onboarding, "reviewer onboarding target is stale")
    require("Primary current-HEAD gate: `bash scripts/stage5g_eda_r3_gate.sh`" in design,
            "design lacks primary R3 gate")
    require("implemented_restart_case_ids remains empty" in design,
            "design does not state empty implementation set")
    require("R3 is gate-only closure" in design, "gate-only boundary missing")

    closed = contract.get("closed_surfaces", {})
    require(closed and all(value is False for value in closed.values()),
            "all contract closed surfaces must remain false")
    inventory_closed = invariants.get("closed_surfaces", {})
    require(inventory_closed and all(value is False for value in inventory_closed.values()),
            "all inventory closed surfaces must remain false")
    print("stage5g-eda-r3-check: PASS")


if __name__ == "__main__":
    main()
