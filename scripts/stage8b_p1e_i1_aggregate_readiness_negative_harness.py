#!/usr/bin/env python3
"""Negative mutations for the I1 aggregate-readiness contract."""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1e_i1_aggregate_readiness_check.py"


def load_checker(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_i1_aggregate_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load checker")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def edit_json(root: Path, relative: str, mutate: Callable[[dict[str, Any]], None]) -> None:
    path = root / relative
    value = json.loads(path.read_text())
    mutate(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {old}")
    path.write_text(text.replace(old, new, 1))


def mutate_duplicate_json(root: Path, module: Any) -> None:
    path = root / module.INVENTORY
    path.write_text(path.read_text().replace('  "schema_version": 1,', '  "schema_version": 1,\n  "schema_version": 1,', 1))


def mutate_milestone(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_milestones"][8].update(commit="0" * 40))


def mutate_review(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_source"].update(review_sha256="f" * 64))


def mutate_self_accept(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value.update(status="ACCEPTED"))


def mutate_telemetry_closed(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["implementation_inventory"].update(production_telemetry_composition="ACCEPTED"))


def mutate_systemd_closed(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["implementation_inventory"].update(p1e_systemd_material="ACCEPTED"))


def mutate_operational(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(operational_redis_db15=True))


def mutate_order(root: Path, module: Any) -> None:
    def change(value: dict[str, Any]) -> None:
        value["remaining_slices"][0]["order"] = 2
        value["remaining_slices"][1]["order"] = 1
    edit_json(root, module.INVENTORY, change)


def mutate_matrix(root: Path, module: Any) -> None:
    path = root / module.MATRIX
    lines = path.read_text().splitlines()
    path.write_text("\n".join(lines[:-1]) + "\n")


def mutate_crash_wording(root: Path, module: Any) -> None:
    replace(root, module.PROCESS_MATRIX, "controlled owner drop/restart", "crashes")


def mutate_process_claim(root: Path, module: Any) -> None:
    replace(root, module.DOCUMENT, "does not yet construct and publish", "already constructs and publishes")


def mutate_status_claim(root: Path, module: Any) -> None:
    replace(root, module.STATUS, "I1 remains open", "I1 is closed")


def mutate_source_telemetry(root: Path, module: Any) -> None:
    path = root / module.PROCESS
    path.write_text(path.read_text() + "\n// .publish_health( unauthorized aggregate claim\n")


CASES = [
    ("duplicate-json-key", mutate_duplicate_json),
    ("accepted-milestone-rebound", mutate_milestone),
    ("accepted-review-rebound", mutate_review),
    ("aggregate-self-accepted", mutate_self_accept),
    ("telemetry-falsely-closed", mutate_telemetry_closed),
    ("systemd-falsely-closed", mutate_systemd_closed),
    ("operational-db15-opened", mutate_operational),
    ("remaining-slice-order-drift", mutate_order),
    ("acceptance-row-removed", mutate_matrix),
    ("controlled-restart-called-crash", mutate_crash_wording),
    ("telemetry-composition-overclaimed", mutate_process_claim),
    ("current-status-overclaims-I1", mutate_status_claim),
    ("production-telemetry-added-outside-slice", mutate_source_telemetry),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-i1-aggregate-") as temp:
        baseline = Path(temp) / "baseline"
        shutil.copytree(
            ROOT,
            baseline,
            ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", ".env", "*.log", "__pycache__"),
        )
        load_checker(0).validate(baseline, verify_lineage=False)
        for index, (name, mutate) in enumerate(CASES, start=1):
            root = Path(temp) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load_checker(index)
            mutate(root, module)
            try:
                module.validate(root, verify_lineage=False)
            except module.CheckFailure:
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1e-i1-aggregate-readiness-negative-harness: PASS {len(CASES)}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
