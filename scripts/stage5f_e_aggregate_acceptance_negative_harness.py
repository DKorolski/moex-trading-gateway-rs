#!/usr/bin/env python3
"""Negative mutations for the Stage 5F-e aggregate authority freeze."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import shutil
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage5f_e_aggregate_acceptance_check.py"


@dataclass(frozen=True)
class Case:
    name: str
    mutate: Callable[[Any, Path], None]


def load_checker(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(
        f"stage5f_e_check_case_{index}", CHECKER
    )
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load Stage 5F-e checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def read_json(root: Path, relative: str) -> dict[str, Any]:
    value = json.loads((root / relative).read_text())
    if not isinstance(value, dict):
        raise RuntimeError(f"expected object: {relative}")
    return value


def write_json(root: Path, relative: str, value: object) -> None:
    (root / relative).write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n"
    )


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rebind_inventory(module: Any, root: Path, payload: dict[str, Any]) -> None:
    write_json(root, module.INVENTORY, payload)
    module.EXPECTED_INVENTORY_SHA256 = digest(root / module.INVENTORY)


def replace_plan(module: Any, root: Path, old: str, new: str) -> None:
    path = root / module.PLAN
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"plan anchor drift: {old!r}")
    path.write_text(text.replace(old, new))
    module.EXPECTED_PLAN_SHA256 = digest(path)


def mutate_accepted_ref(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["accepted_stage5f_d"]["source_ref"] = "0" * 40
    rebind_inventory(module, root, payload)


def mutate_status(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["status"] = "accepted_without_review"
    rebind_inventory(module, root, payload)


def mutate_row_count(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["matrix"]["row_count"] = 33
    rebind_inventory(module, root, payload)


def mutate_row_order(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["matrix"]["row_ids"][0:2] = reversed(
        payload["matrix"]["row_ids"][0:2]
    )
    rebind_inventory(module, root, payload)


def mutate_open_surface(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["closure_contract"]["closed_surfaces"]["runtime_live"] = True
    rebind_inventory(module, root, payload)


def mutate_reproducibility_count(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["closure_contract"]["minimum_reproducibility_runs"] = 2
    rebind_inventory(module, root, payload)


def mutate_gate_inventory(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["closure_contract"]["required_gate_labels"].pop()
    rebind_inventory(module, root, payload)


def mutate_report_inventory(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["closure_contract"]["required_reports"].pop()
    rebind_inventory(module, root, payload)


def mutate_allowed_paths(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["allowed_changes_since_accepted_stage5f_d"].append(
        "crates/strategy-runtime-core/src/live_bypass.rs"
    )
    rebind_inventory(module, root, payload)


def mutate_verdict(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["accepted_stage5f_d"]["verdict"] = "PENDING"
    rebind_inventory(module, root, payload)


def mutate_target(module: Any, root: Path) -> None:
    payload = read_json(root, module.INVENTORY)
    payload["target"]["symbol"] = "RTS-9.26"
    rebind_inventory(module, root, payload)


def mutate_duplicate_key(module: Any, root: Path) -> None:
    path = root / module.INVENTORY
    text = path.read_text()
    text = text.replace(
        '  "schema_version": 1,',
        '  "schema_version": 1,\n  "schema_version": 1,',
        1,
    )
    path.write_text(text)
    module.EXPECTED_INVENTORY_SHA256 = digest(path)


def mutate_golden(module: Any, root: Path) -> None:
    path = root / module.D_GOLDEN
    text = path.read_text()
    path.write_text(text.replace('"row_id": "F01"', '"row_id": "F99"', 1))


def mutate_runtime_source(module: Any, root: Path) -> None:
    relative = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
    path = root / relative
    path.write_text(path.read_text() + "\n// unauthorized Stage 5F-e drift\n")


def mutate_plan_boundary(module: Any, root: Path) -> None:
    replace_plan(
        module,
        root,
        "Stage 5G is not authorized",
        "Stage 5G is authorized",
    )


CASES = [
    Case("accepted-stage5f-d-ref-drift", mutate_accepted_ref),
    Case("self-declared-acceptance", mutate_status),
    Case("matrix-row-count-reduction", mutate_row_count),
    Case("matrix-row-order-drift", mutate_row_order),
    Case("runtime-live-surface-opened", mutate_open_surface),
    Case("reproducibility-run-count-reduced", mutate_reproducibility_count),
    Case("required-gate-removed", mutate_gate_inventory),
    Case("required-report-removed", mutate_report_inventory),
    Case("rust-path-added-to-allowlist", mutate_allowed_paths),
    Case("accepted-verdict-rebound", mutate_verdict),
    Case("target-symbol-rebound", mutate_target),
    Case("duplicate-json-key", mutate_duplicate_key),
    Case("frozen-golden-drift", mutate_golden),
    Case("frozen-runtime-source-drift", mutate_runtime_source),
    Case("stage5g-boundary-opened-in-report", mutate_plan_boundary),
]


def copy_baseline(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(
            ".git",
            "target",
            "reports",
            "tmp",
            ".env",
            "*.log",
            "__pycache__",
            "__MACOSX",
        ),
    )


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5f-e-negative-") as temp:
        baseline = Path(temp) / "baseline"
        copy_baseline(baseline)
        baseline_checker = load_checker(0)
        baseline_checker.validate(baseline, verify_lineage=False)
        for index, case in enumerate(CASES, start=1):
            root = Path(temp) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load_checker(index)
            case.mutate(module, root)
            try:
                module.validate(root, verify_lineage=False)
            except module.CheckFailure:
                print(f"PASS {case.name}")
                continue
            print(f"FAIL {case.name}: mutation was accepted")
            return 1
    print(f"stage5f-e-aggregate-negative-harness: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
