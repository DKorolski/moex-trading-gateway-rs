#!/usr/bin/env python3
"""Mutation-complete current-HEAD checker for Stage 5G-e-d-a R2."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
INVENTORY = Path("docs/stage-5/stage5g-lifecycle-entry-inventory.json")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
GATE = Path("scripts/stage5g_eda_r2_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r2_negative_harness.py")
HISTORICAL_CHECK = Path("scripts/stage5g_ed_check.py")
HISTORICAL_NEGATIVE = Path("scripts/stage5g_ed_negative_harness.py")
R1_REF = "9a3221602a902bc6207418f0131665a039d62768"

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

EXPECTED_VALIDATION_CONTEXT_FIELDS = [
    "expected_operational_identity",
    "pre_restart_package_id",
    "pre_restart_snapshot_epoch",
    "last_reconciled_fresh_package",
    "accepted_replay_ledger",
    "known_historical_fresh_packages",
    "clean_restore_completed_at",
    "validation_observed_at",
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r2-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r2-check: FAIL: cannot load {path}: {error}") from error
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
    body = rust_body(source, "enum", name)
    variants = []
    for line in body.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or not stripped.endswith(","):
            continue
        variants.append(stripped[:-1])
    return variants


def rust_struct_fields(source: str, name: str) -> list[str]:
    body = rust_body(source, "struct", name)
    fields = []
    for line in body.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("//") or ":" not in stripped:
            continue
        fields.append(stripped.split(":", 1)[0].split()[-1])
    return fields


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()
    files = (
        SOURCE, LIB, CONTRACT, DESIGN, INVENTORY, STATUS, ONBOARDING, GATE, NEGATIVE,
        HISTORICAL_CHECK, HISTORICAL_NEGATIVE,
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
    historical_check = (root / HISTORICAL_CHECK).read_text()
    historical_negative = (root / HISTORICAL_NEGATIVE).read_text()
    contract = load_json(root / CONTRACT)
    inventory = load_json(root / INVENTORY)

    restart_family = next(
        (row for row in inventory.get("scenario_families", []) if row.get("id") == "RESTART"),
        None,
    )
    require(restart_family is not None, "RESTART family missing")
    require(restart_family.get("case_ids") == EXPECTED_GRST_IDS, "inventory GRST IDs drifted")
    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart_scenarios must be a list")
    require([row.get("id") for row in scenarios] == EXPECTED_GRST_IDS,
            "contract GRST IDs/order drifted")
    require(contract.get("restart_scenario_count") == len(EXPECTED_GRST_IDS),
            "restart scenario count drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b implementation claimed")
    require(contract.get("dispositions") == EXPECTED_DISPOSITIONS,
            "contract disposition vocabulary/order drifted")
    require(contract.get("operational_identity_fields") == EXPECTED_OPERATIONAL_FIELDS,
            "contract operational identity fields/order drifted")
    require(contract.get("status") == "r2_implementation_review_candidate", "R2 status drifted")
    require(contract.get("rejected_r1_commit") == R1_REF, "R1 base binding drifted")

    require(
        rust_enum_variants(source, "Stage5gRestartReconciliationDisposition")
        == EXPECTED_DISPOSITIONS,
        "Rust disposition enum drifted",
    )
    require(
        rust_enum_variants(source, "Stage5gRestartScenarioId") == EXPECTED_GRST_VARIANTS,
        "Rust GRST enum drifted",
    )
    mapping_region = source.split("impl Stage5gRestartScenarioId", 1)[1].split(
        "pub(crate) enum Stage5gFreshBrokerTruthError", 1
    )[0]
    require(re.findall(r'"(GRST\d\d_[A-Z0-9_]+)"', mapping_region) == EXPECTED_GRST_IDS,
            "Rust frozen GRST ID mapping drifted")
    require(rust_struct_fields(source, "Stage5gOperationalIdentityV1")
            == EXPECTED_OPERATIONAL_FIELDS, "validated Rust identity members drifted")
    require(rust_struct_fields(source, "Stage5gOperationalIdentityInput")
            == EXPECTED_OPERATIONAL_FIELDS, "raw Rust identity members drifted")
    require(rust_struct_fields(source, "Stage5gFreshBrokerTruthValidationContext")
            == EXPECTED_VALIDATION_CONTEXT_FIELDS, "replay validation authorities drifted")

    shape = contract.get("contract", {})
    required_true = (
        "package_identity_required", "fresh_snapshot_epoch_required",
        "captured_after_clean_restore_required", "exact_operational_identity_required",
        "orders_completeness_explicit", "trades_completeness_explicit",
        "positions_completeness_explicit", "canonical_rows_required",
        "section_observation_required", "post_restore_row_receipt_required",
        "semantic_position_dedup_required", "validated_identity_constructor_only",
        "explicit_remaining_quantity_required", "filled_requires_complete_fill",
        "replay_lineage_split_required", "changed_fingerprint_conflict_required",
        "canonical_identity_token_required", "chronology_mutation_complete",
    )
    for flag in required_true:
        require(shape.get(flag) is True, f"contract flag {flag} must be true")
    require(shape.get("incomplete_section_means_absent_rows") is False,
            "incomplete section cannot mean absence")
    require(shape.get("validated_package_serializable") is False,
            "validated package became serializable")
    require(shape.get("validated_package_owns_callback_authority") is False,
            "validated package acquired callback authority")

    chronology_tokens = (
        "validate_section_observation(\n        package.orders_observed_at,\n        package.captured_at,",
        "validate_section_observation(\n        package.trades_observed_at,\n        package.captured_at,",
        "validate_section_observation(\n        package.positions_observed_at,\n        package.captured_at,",
        "observed_at <= clean_restore_completed_at",
        "observed_at > captured_at",
        "order.received_ts < clean_restore_completed_at",
        "trade.received_ts < clean_restore_completed_at",
        "position.received_ts < clean_restore_completed_at",
        "order.received_ts > package.orders_observed_at",
        "trade.received_ts > package.trades_observed_at",
        "position.received_ts > package.positions_observed_at",
        "source_ts > order.received_ts",
        "trade.source_ts > trade.received_ts",
        "source_ts > position.received_ts",
    )
    for token in chronology_tokens:
        require(token in source, f"chronology guard missing: {token}")

    identity_tokens = (
        "fn canonical_identity_token(value: &str) -> bool",
        "character.is_whitespace() || character.is_control()",
        "if !canonical_identity_token(&value)",
        "!canonical_identity_token(input.account_id.as_str())",
        "!canonical_identity_token(&input.target_instrument.symbol)",
        "!canonical_identity_token(context.pre_restart_package_id)",
        "fn canonical_identity_token_grammar_is_constructor_and_json_enforced",
    )
    for token in identity_tokens:
        require(token in source, f"canonical token contract missing: {token}")

    test_tokens = (
        "fn source_chronology_is_enforced_for_order_trade_and_position",
        "fn every_section_observation_is_bounded_by_restore_and_capture",
        "fn every_row_receipt_is_bounded_by_its_section_observation",
        "fn stale_pre_restore_order_trade_and_position_rows_fail_closed",
    )
    for token in test_tokens:
        require(token in source, f"focused chronology witness missing: {token}")

    exact_validated_derive = (
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize)]\n"
        "#[serde(deny_unknown_fields)]\n"
        "pub(crate) struct Stage5gOperationalIdentityV1"
    )
    require(exact_validated_derive in source, "validated identity regained Deserialize")
    require("instrument_identity_matches(&previous.instrument, &position.instrument)" in source,
            "semantic position dedup drifted")
    require("matches!(order.status, OrderStatus::Filled) && order.filled_qty != order.qty" in source,
            "filled-order completeness guard drifted")
    require(
        "    } else {\n        Err(Stage5gFreshBrokerTruthError::FreshPackageIdentityConflict)\n"
        "    }\n}\n\nfn validate_replay_ledger(" in source,
        "same-package changed-fingerprint conflict guard drifted",
    )
    require("previous_package_id" not in source and "previous_snapshot_epoch" not in source,
            "ambiguous replay authorities restored")

    require("mod stage5g_fresh_broker_truth;" in lib, "private module missing")
    require("pub mod stage5g_fresh_broker_truth" not in lib, "module leaked publicly")
    require("pub use stage5g_fresh_broker_truth" not in lib, "module re-exported")
    require("pub fn " not in source, "public function introduced")
    for forbidden in (
        "use redis", "redis::", "reqwest::", ".post(", ".delete(", "finam_client",
        "HybridIntradayRuntimeStrategy", "Stage5gCleanRestartedCapability", "on_bar(", "on_timer(",
    ):
        require(forbidden not in source, f"forbidden surface/authority: {forbidden}")

    required_mutations = (
        "drop-json-disposition", "rename-rust-disposition",
        "drop-json-operational-field", "drop-rust-operational-field",
        "remove-order-source-chronology", "remove-trade-source-chronology",
        "remove-position-source-chronology", "remove-orders-section-observation",
        "remove-section-post-restore-bound", "remove-section-captured-at-bound",
        "restore-trim-only-identity-grammar", "public-module-leak", "open-reducer",
        "runtime-callback-surface",
    )
    for mutation in required_mutations:
        require(mutation in negative, f"required R2 mutation missing: {mutation}")

    require(f'r1_ref="{R1_REF}"' in gate, "detached R1 source binding drifted")
    require('git worktree add --detach "$snapshot_root" "$r1_ref"' in gate,
            "R1 gate must run in detached source")
    require("bash scripts/stage5g_eda_r1_gate.sh" in gate, "inherited R1 gate missing")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth" in gate,
            "focused release test missing")
    require("cargo test -p strategy-runtime-core --lib" in gate, "full core test missing")
    require("Stage 5G-e-d-a R2" in status and "Stage 5G-e-d-b remains closed" in status,
            "current status does not preserve R2 boundary")
    require("Stage 5G-e-d-a R2" in onboarding, "reviewer onboarding target is stale")
    require("canonical identity-token grammar" in design, "design lacks identity grammar")
    require("Primary current-HEAD gate: `bash scripts/stage5g_eda_r2_gate.sh`" in design,
            "design lacks one primary R2 gate")
    require("PREDECESSOR-ONLY" in historical_check and "EXPECTED_SNAPSHOT_REF" in historical_check,
            "historical e-d checker lacks predecessor-only diagnostic")
    require("PREDECESSOR-ONLY" in historical_negative
            and "EXPECTED_SNAPSHOT_REF" in historical_negative,
            "historical e-d negative harness lacks predecessor-only diagnostic")

    closed = contract.get("closed_surfaces", {})
    require(closed and all(value is False for value in closed.values()),
            "all closed surfaces must remain false")
    print("stage5g-eda-r2-check: PASS")


if __name__ == "__main__":
    main()
