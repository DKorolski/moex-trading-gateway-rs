#!/usr/bin/env python3
"""Controlling production-frozen checker for Stage 5G-e-d-a R4."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

import stage5g_eda_r3_check as r3


SOURCE = r3.SOURCE
LIB = r3.LIB
CONTRACT = r3.CONTRACT
DESIGN = r3.DESIGN
INVARIANTS = r3.INVARIANTS
LIFECYCLE_INVENTORY = r3.LIFECYCLE_INVENTORY
STATUS = r3.STATUS
ONBOARDING = r3.ONBOARDING
FREEZE = Path("docs/stage-5/stage5g-e-d-a-r4-production-freeze.json")
GATE = Path("scripts/stage5g_eda_r4_gate.sh")
NEGATIVE = Path("scripts/stage5g_eda_r4_negative_harness.py")
PRESEAL = Path("scripts/stage5g_eda_r4_preseal_check.py")
BUILDER = Path("scripts/make_stage5g_ed_handoff_archive.py")
R3_NEGATIVE = Path("scripts/stage5g_eda_r3_negative_harness.py")
R3_REF = "2ebb097eab73708b142c0bc26da217f1404a81aa"
PRODUCTION_MARKER = "#[cfg(test)]\nmod tests {"
PRODUCTION_PREFIX_LINES = 766
PRODUCTION_PREFIX_SHA256 = "f2c1d9d104e3351e5d3c0eef300ca8e27081cb7568bd32e6eac8e0f421bd359f"

EXPECTED_FREEZE = {
    "schema_version": 1,
    "stage": "5G-e-d-a-r4",
    "base_commit": R3_REF,
    "source_path": str(SOURCE),
    "production_test_boundary_marker": PRODUCTION_MARKER,
    "boundary_marker_occurrences": 1,
    "accepted_production_prefix_line_count": PRODUCTION_PREFIX_LINES,
    "accepted_production_prefix_sha256": PRODUCTION_PREFIX_SHA256,
    "production_validator_frozen": True,
    "test_only_successor_changes_allowed": True,
    "implemented_restart_case_ids": [],
    "stage5g_e_d_b_open": False,
}

EXPECTED_CONTRACT_SHAPE = {
    "package_schema_version": 1,
    "package_identity_required": True,
    "fresh_snapshot_epoch_required": True,
    "captured_after_clean_restore_required": True,
    "section_observation_required": True,
    "post_restore_row_receipt_required": True,
    "exact_operational_identity_required": True,
    "validated_identity_constructor_only": True,
    "orders_completeness_explicit": True,
    "trades_completeness_explicit": True,
    "positions_completeness_explicit": True,
    "incomplete_section_means_absent_rows": False,
    "canonical_rows_required": True,
    "semantic_position_dedup_required": True,
    "explicit_remaining_quantity_required": True,
    "filled_requires_complete_fill": True,
    "replay_lineage_split_required": True,
    "changed_fingerprint_conflict_required": True,
    "canonical_identity_token_required": True,
    "chronology_mutation_complete": True,
    "validated_package_serializable": False,
    "validated_package_owns_callback_authority": False,
}

EXPECTED_CLOSED_SURFACES = {
    "reconciliation_reducer": False,
    "strategy_callback": False,
    "runtime_mutation": False,
    "stage5g_f": False,
    "redis_live_consumer_groups": False,
    "finam_transport": False,
    "http_post_delete": False,
    "broker_dispatch_execution": False,
    "runtime_live": False,
    "real_orders": False,
    "stage6": False,
    "deployment": False,
}

R4_INVARIANT_IDS = [
    "account_token_application",
    "target_symbol_token_application",
    "pre_restart_token_application",
    "order_quantity_positive",
    "order_filled_nonnegative",
    "order_not_overfilled",
    "order_remaining_nonnegative",
    "orphan_order_identity_unique",
    "fresh_snapshot_epoch_not_reused",
    "known_historical_requires_acceptance",
    "market_data_generation_nonzero",
    "command_generation_nonzero",
    "instrument_map_sha256_valid",
]

R4_MUTATIONS = {
    "contract-orders-completeness-false",
    "contract-trades-completeness-false",
    "contract-positions-completeness-false",
    "contract-incomplete-means-absence",
    "contract-validated-package-serializable",
    "contract-package-callback-authority",
    "remove-closed-surface-key",
    "add-unreviewed-closed-surface-key",
    "remove-account-token-application",
    "remove-target-symbol-token-application",
    "remove-pre-restart-token-application",
    "remove-order-positive-qty",
    "remove-order-negative-filled",
    "remove-order-overfilled",
    "remove-negative-remaining",
    "remove-orphan-order-duplicate",
    "remove-reused-snapshot-epoch",
    "remove-market-data-generation-validation",
    "remove-command-generation-validation",
    "remove-instrument-map-sha-validation",
    "append-alias-based-source-reducer",
    "change-production-freeze-hash",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-eda-r4-check: FAIL: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-eda-r4-check: FAIL: cannot load {path}: {error}") from error
    require(isinstance(value, dict), f"{path} must contain an object")
    return value


def production_prefix(source: str) -> str:
    require(source.count(PRODUCTION_MARKER) == 1, "production/test marker must occur once")
    prefix = source.split(PRODUCTION_MARKER, 1)[0]
    require(prefix.count("\n") + 1 == PRODUCTION_PREFIX_LINES,
            "production prefix line count drifted")
    require(hashlib.sha256(prefix.encode()).hexdigest() == PRODUCTION_PREFIX_SHA256,
            "accepted production prefix SHA-256 drifted")
    return prefix


def assert_alias_aware_authority_closed(prefix: str) -> None:
    protected = {
        "Stage5gValidatedFreshBrokerTruthPackage",
        "Stage5gRestartReconciliationDisposition",
    }
    aliases: dict[str, str] = {}
    for match in re.finditer(r"(?m)^\s*(?:pub(?:\(crate\))?\s+)?type\s+([A-Za-z0-9_]+)\s*=\s*([^;]+);", prefix):
        name, rhs = match.group(1), match.group(2)
        aliases[name] = rhs
        require(not any(item in rhs for item in protected),
                f"protected authority type alias introduced: {name}")

    def expands_to(value: str, protected_type: str) -> bool:
        if protected_type in value:
            return True
        return any(name in value and protected_type in rhs for name, rhs in aliases.items())

    signatures = re.finditer(
        r"(?:pub(?:\(crate\))?\s+)?(?:const\s+)?fn\s+([A-Za-z0-9_]+)\s*"
        r"(?:<[^>{}]*>)?\s*\((.*?)\)\s*(?:->\s*([^\{]+))?\{",
        prefix,
        re.DOTALL,
    )
    allowed_classifier_names = {"classify_lineage"}
    for match in signatures:
        name, parameters, returns = match.group(1), match.group(2), match.group(3) or ""
        require(not expands_to(parameters, "Stage5gValidatedFreshBrokerTruthPackage"),
                f"function {name} consumes validated truth")
        require(not expands_to(returns, "Stage5gRestartReconciliationDisposition"),
                f"function {name} produces reconciliation disposition")
        if any(token in name.lower() for token in ("reducer", "classif", "reconcile", "apply", "callback")):
            require(name in allowed_classifier_names, f"reducer-like production function: {name}")

    require(not re.search(r"(?s)\b(?:const|static)\b[^;=]*=\s*\|", prefix),
            "closure authority stored in const/static")
    for match in re.finditer(r"(?s)\bimpl\b([^\{]+)\{", prefix):
        header = " ".join(match.group(1).split())
        if any(item in header for item in protected):
            require(header == "Stage5gValidatedFreshBrokerTruthPackage",
                    "trait/impl authority mentions protected type")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    root = parser.parse_args().root.resolve()
    files = (
        SOURCE, LIB, CONTRACT, DESIGN, INVARIANTS, LIFECYCLE_INVENTORY, STATUS,
        ONBOARDING, FREEZE, GATE, NEGATIVE, PRESEAL, BUILDER, R3_NEGATIVE,
    )
    for relative in files:
        require((root / relative).is_file(), f"missing {relative}")

    source = (root / SOURCE).read_text()
    prefix = production_prefix(source)
    assert_alias_aware_authority_closed(prefix)
    freeze = load_json(root / FREEZE)
    require(freeze == EXPECTED_FREEZE, "production freeze manifest drifted")

    contract = load_json(root / CONTRACT)
    require(contract.get("status") == "r4_gate_only_review_candidate", "R4 status drifted")
    require(contract.get("rejected_r3_commit") == R3_REF, "R3 base binding drifted")
    require(contract.get("contract") == EXPECTED_CONTRACT_SHAPE, "exact contract map drifted")
    require(contract.get("closed_surfaces") == EXPECTED_CLOSED_SURFACES,
            "exact closed-surface map drifted")
    require(contract.get("implemented_restart_case_ids") == [], "e-d-b implementation claimed")
    require(contract.get("dispositions") == r3.EXPECTED_DISPOSITIONS,
            "contract dispositions drifted")
    require(contract.get("operational_identity_fields") == r3.EXPECTED_OPERATIONAL_FIELDS,
            "contract operational fields drifted")
    scenarios = contract.get("restart_scenarios")
    require(isinstance(scenarios, list), "restart scenarios missing")
    require([row.get("id") for row in scenarios] == r3.EXPECTED_GRST_IDS,
            "contract GRST IDs/order drifted")
    require(contract.get("restart_scenario_count") == 12, "restart scenario count drifted")

    lifecycle = load_json(root / LIFECYCLE_INVENTORY)
    restart_family = next(
        (row for row in lifecycle.get("scenario_families", []) if row.get("id") == "RESTART"),
        None,
    )
    require(restart_family is not None, "RESTART lifecycle family missing")
    require(restart_family.get("case_ids") == r3.EXPECTED_GRST_IDS,
            "lifecycle GRST IDs/order drifted")

    invariants = load_json(root / INVARIANTS)
    rows = invariants.get("invariants")
    require(isinstance(rows, list), "invariant inventory must be a list")
    expected_ids = r3.EXPECTED_INVARIANT_IDS + R4_INVARIANT_IDS
    require([row.get("invariant_id") for row in rows] == expected_ids,
            "R4 invariant inventory drifted")
    require(invariants.get("stage") == "5G-e-d-a-r4", "invariant stage drifted")
    require(invariants.get("base_commit") == R3_REF, "invariant base drifted")
    negative = (root / NEGATIVE).read_text()
    r3_negative = (root / R3_NEGATIVE).read_text()
    inherited = set(re.findall(r'\(\"([^\"]+)\", lambda root:', r3_negative))
    current_only = set(re.findall(r'\(\"([^\"]+)\", lambda root:', negative))
    current = inherited | current_only
    for row in rows:
        invariant_id = row["invariant_id"]
        require(row.get("production_anchor") in prefix,
                f"production anchor missing for {invariant_id}")
        require(row.get("focused_rust_witness") in source,
                f"focused witness missing for {invariant_id}")
        require(row.get("negative_mutation_id") in current,
                f"negative mutation missing for {invariant_id}")

    require(len(inherited) == 56, "R3 mutation inventory no longer has 56 cases")
    require(R4_MUTATIONS <= current_only, "mandatory R4 mutation missing")
    require(len(current) >= 76, "R4 negative matrix has fewer than 76 cases")

    lib = (root / LIB).read_text()
    require("mod stage5g_fresh_broker_truth;" in lib, "private module missing")
    require("pub mod stage5g_fresh_broker_truth" not in lib, "module leaked publicly")
    require("pub use stage5g_fresh_broker_truth" not in lib, "module re-exported")
    for forbidden in (
        "use redis", "redis::", "reqwest::", ".post(", ".delete(", "finam_client",
        "HybridIntradayRuntimeStrategy", "Stage5gCleanRestartedCapability", "on_bar(",
        "on_timer(",
    ):
        require(forbidden not in prefix, f"forbidden production surface: {forbidden}")

    gate = (root / GATE).read_text()
    require(f'r3_ref="{R3_REF}"' in gate, "detached R3 binding drifted")
    require('git worktree add --detach "$snapshot_root" "$r3_ref"' in gate,
            "R3 gate must run detached")
    require("bash scripts/stage5g_eda_r3_gate.sh" in gate, "inherited R3 gate missing")
    require("python3 scripts/stage5g_eda_r4_preseal_check.py" in gate, "R4 preseal missing")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_fresh_broker_truth" in gate,
            "focused release tests missing")
    require("cargo test -p strategy-runtime-core --lib" in gate, "full core tests missing")

    builder = (root / BUILDER).read_text()
    require(f'REQUIRED_PARENT = "{R3_REF}"' in builder, "R4 builder parent drifted")
    require('STAGE = "5G-e-d-a-r4"' in builder, "R4 builder stage drifted")
    require('["bash", "scripts/stage5g_eda_r4_gate.sh"]' in builder, "R4 builder gate drifted")

    design = (root / DESIGN).read_text()
    status = (root / STATUS).read_text()
    onboarding = (root / ONBOARDING).read_text()
    require("Primary current-HEAD gate: `bash scripts/stage5g_eda_r4_gate.sh`" in design,
            "design primary R4 gate missing")
    require("Production fresh BrokerTruth validator remains frozen" in design,
            "design production freeze statement missing")
    require("implemented_restart_case_ids remains empty" in design,
            "design empty GRST implementation statement missing")
    require("Stage 5G-e-d-a R4" in status and "Stage 5G-e-d-b remains closed" in status,
            "status R4 boundary missing")
    require("Stage 5G-e-d-a R4" in onboarding, "onboarding R4 target missing")
    print("stage5g-eda-r4-check: PASS")


if __name__ == "__main__":
    main()
