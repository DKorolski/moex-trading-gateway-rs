#!/usr/bin/env python3
"""Isolated negative matrix for the Stage 5F-b fixture contract."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER_PATH = ROOT / "scripts/stage5f_fixture_contract_check.py"
FILES = [
    "docs/stage-5/5f-b-fixture-input-fingerprint-contract.md",
    "docs/stage-5/5f-b0-source-reachability-fingerprint-audit.md",
    "docs/stage-5/stage5f-b-fixture-inventory.json",
    "docs/stage-5/stage5f-b0-source-reachability-inventory.json",
    "docs/stage-5/stage5f-controlled-observation-extension.json",
    "tests/fixtures/stage5/stage5f/v1/scenarios/atomic-hybrid-scenarios.json",
    "tests/fixtures/stage5/stage5f/v1/states/imoexf-hybrid-state-seeds.json",
    "tests/fixtures/stage5/stage5f/v1/riskgate/imoexf-high180-riskgate-seeds.json",
]


def load_checker(case_index: int):
    spec = importlib.util.spec_from_file_location(
        f"stage5f_fixture_contract_check_case_{case_index}", CHECKER_PATH
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load Stage 5F fixture checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_json(root: Path, relative: str) -> dict[str, Any]:
    value = json.loads((root / relative).read_text())
    if not isinstance(value, dict):
        raise RuntimeError(f"expected object: {relative}")
    return value


def write_json(root: Path, relative: str, value: object) -> None:
    (root / relative).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def digest(root: Path, relative: str) -> str:
    return hashlib.sha256((root / relative).read_bytes()).hexdigest()


def rebind_scenario(module: Any, root: Path) -> None:
    value = digest(root, module.SCENARIO_PATH)
    module.SCENARIO_SHA256 = value
    inventory = read_json(root, module.INVENTORY_PATH)
    inventory["fixture_catalogs"]["scenarios"]["sha256"] = value
    write_json(root, module.INVENTORY_PATH, inventory)


def rebind_state(module: Any, root: Path) -> None:
    value = digest(root, module.STATE_PATH)
    module.STATE_SHA256 = value
    scenarios = read_json(root, module.SCENARIO_PATH)
    for record in scenarios["records"]:
        record["pre_state"]["catalog_sha256"] = value
    write_json(root, module.SCENARIO_PATH, scenarios)
    inventory = read_json(root, module.INVENTORY_PATH)
    inventory["fixture_catalogs"]["states"]["sha256"] = value
    write_json(root, module.INVENTORY_PATH, inventory)
    rebind_scenario(module, root)


def rebind_riskgate(module: Any, root: Path) -> None:
    value = digest(root, module.RISKGATE_PATH)
    module.RISKGATE_SHA256 = value
    scenarios = read_json(root, module.SCENARIO_PATH)
    for record in scenarios["records"]:
        record["riskgate"]["catalog_sha256"] = value
    write_json(root, module.SCENARIO_PATH, scenarios)
    inventory = read_json(root, module.INVENTORY_PATH)
    inventory["fixture_catalogs"]["riskgate"]["sha256"] = value
    write_json(root, module.INVENTORY_PATH, inventory)
    rebind_scenario(module, root)


def rebind_observation(module: Any, root: Path) -> None:
    value = digest(root, module.OBSERVATION_PATH)
    module.OBSERVATION_SHA256 = value
    inventory = read_json(root, module.INVENTORY_PATH)
    inventory["fixture_catalogs"]["observation_design"]["sha256"] = value
    write_json(root, module.INVENTORY_PATH, inventory)


def mutate_scenarios(
    module: Any, root: Path, mutation: Callable[[dict[str, Any]], None]
) -> None:
    scenarios = read_json(root, module.SCENARIO_PATH)
    mutation(scenarios)
    write_json(root, module.SCENARIO_PATH, scenarios)
    rebind_scenario(module, root)


def mutate_state(
    module: Any, root: Path, mutation: Callable[[dict[str, Any]], None]
) -> None:
    state = read_json(root, module.STATE_PATH)
    mutation(state)
    write_json(root, module.STATE_PATH, state)
    rebind_state(module, root)


def mutate_riskgate(
    module: Any, root: Path, mutation: Callable[[dict[str, Any]], None]
) -> None:
    riskgate = read_json(root, module.RISKGATE_PATH)
    mutation(riskgate)
    write_json(root, module.RISKGATE_PATH, riskgate)
    rebind_riskgate(module, root)


def scenario_record(value: dict[str, Any], row_id: str) -> dict[str, Any]:
    return next(record for record in value["records"] if record["row_id"] == row_id)


def duplicate_scenario_json_key(module: Any, root: Path) -> None:
    path = root / module.SCENARIO_PATH
    text = path.read_text()
    path.write_text(text.replace('  "schema_version": 1,', '  "schema_version": 1,\n  "schema_version": 1,', 1))
    rebind_scenario(module, root)


def extra_fixture_file(_module: Any, root: Path) -> None:
    path = root / "tests/fixtures/stage5/stage5f/v1/scenarios/unbound.json"
    path.write_text("{}\n")


def observer_module_added(_module: Any, root: Path) -> None:
    path = root / "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("// forbidden during Stage 5F-b\n")


def b0_disposition_drift(module: Any, root: Path) -> None:
    b0 = read_json(root, module.B0_INVENTORY_PATH)
    b0["rows"][0]["matrix_disposition"] = "terminal_after_callback"
    write_json(root, module.B0_INVENTORY_PATH, b0)
    inventory = read_json(root, module.INVENTORY_PATH)
    inventory["source_reachability_inventory_sha256"] = digest(
        root, module.B0_INVENTORY_PATH
    )
    write_json(root, module.INVENTORY_PATH, inventory)


def observation_control_flow(module: Any, root: Path) -> None:
    design = read_json(root, module.OBSERVATION_PATH)
    design["invariants"]["observer_controls_runtime_flow"] = True
    write_json(root, module.OBSERVATION_PATH, design)
    rebind_observation(module, root)


def plan_contract_removed(module: Any, root: Path) -> None:
    path = root / module.PLAN_PATH
    path.write_text(path.read_text().replace("Those requirements are circular", "Those requirements differ"))


Case = tuple[str, Callable[[Any, Path], None]]


CASES: list[Case] = [
    ("duplicate-json-key", duplicate_scenario_json_key),
    (
        "bool-record-schema-version",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01").__setitem__("schema_version", True)),
    ),
    (
        "unknown-record-field",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01").__setitem__("unknown", 1)),
    ),
    (
        "missing-scenario-row",
        lambda m, r: mutate_scenarios(m, r, lambda v: v["records"].pop()),
    ),
    (
        "duplicate-row-id",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F02").__setitem__("row_id", "F01")),
    ),
    (
        "target-symbol-drift",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["target"]["instrument"].__setitem__("symbol", "RI")),
    ),
    (
        "non-final-bar",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["bar"].__setitem__("is_final", False)),
    ),
    (
        "bool-timeframe-smuggling",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["bar"].__setitem__("timeframe_sec", True)),
    ),
    (
        "non-live-bar",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["bar"].__setitem__("origin", "Replay")),
    ),
    (
        "negative-zero-input",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["bar"].__setitem__("close", "-0.0")),
    ),
    (
        "nonfinite-input",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["bar"].__setitem__("high", "NaN")),
    ),
    (
        "clock-reversal",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["clock"].__setitem__("callback_ts_utc", "2026-01-06T06:09:59Z")),
    ),
    (
        "unknown-state-seed",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["pre_state"].__setitem__("seed_id", "forged")),
    ),
    (
        "state-catalog-hash-tamper",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["pre_state"].__setitem__("catalog_sha256", "0" * 64)),
    ),
    (
        "unknown-riskgate-seed",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["riskgate"].__setitem__("seed_id", "forged")),
    ),
    (
        "pending-output-smuggling",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["expected"].__setitem__("pre_state_fingerprint", "a" * 64)),
    ),
    (
        "disposition-drift",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["expected"].__setitem__("disposition", "terminal_after_callback")),
    ),
    (
        "bool-callback-count-smuggling",
        lambda m, r: mutate_scenarios(m, r, lambda v: scenario_record(v, "F01")["expected"].__setitem__("callback_count", True)),
    ),
    (
        "inventory-binding-drift",
        lambda m, r: (lambda v: (v["row_bindings"][0].__setitem__("owning_test", "forged"), write_json(r, m.INVENTORY_PATH, v)))(read_json(r, m.INVENTORY_PATH)),
    ),
    (
        "pending-marked-acceptance-evidence",
        lambda m, r: (lambda v: (v.__setitem__("current_outputs_are_acceptance_evidence", True), write_json(r, m.INVENTORY_PATH, v)))(read_json(r, m.INVENTORY_PATH)),
    ),
    ("observation-controls-flow", observation_control_flow),
    (
        "partial-pending-entry-seed",
        lambda m, r: mutate_state(m, r, lambda v: next(s for s in v["seeds"] if s["seed_id"] == "pending_entry")["pending_entry"].pop("request_id")),
    ),
    (
        "duplicate-state-seed-id",
        lambda m, r: mutate_state(m, r, lambda v: v["seeds"][1].__setitem__("seed_id", "flat_ready")),
    ),
    (
        "riskgate-enforcement-opened",
        lambda m, r: mutate_riskgate(m, r, lambda v: v["seeds"][0].__setitem__("risk_gate_mode", "enforced")),
    ),
    (
        "riskgate-bool-ledger-count",
        lambda m, r: mutate_riskgate(m, r, lambda v: v["seeds"][0].__setitem__("ledger_rows_count", True)),
    ),
    ("unbound-fixture-file", extra_fixture_file),
    ("observer-implemented-during-contract", observer_module_added),
    ("b0-disposition-drift", b0_disposition_drift),
    ("contract-rationale-removed", plan_contract_removed),
    (
        "inventory-unknown-field",
        lambda m, r: (lambda v: (v.__setitem__("unknown", 1), write_json(r, m.INVENTORY_PATH, v)))(read_json(r, m.INVENTORY_PATH)),
    ),
]


def prepare_root(destination: Path) -> None:
    for relative in FILES:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def main() -> int:
    passed = 0
    for index, (name, mutation) in enumerate(CASES):
        with tempfile.TemporaryDirectory(prefix=f"stage5f-fixture-{index:02d}-") as temp:
            case_root = Path(temp)
            prepare_root(case_root)
            checker = load_checker(index)
            mutation(checker, case_root)
            try:
                checker.check(case_root)
            except Exception:
                print(f"PASS {name}")
                passed += 1
            else:
                print(f"FAIL {name}: mutation was accepted")
                return 1
    print(f"stage5f-fixture-contract-negative-harness: ok cases={passed}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
