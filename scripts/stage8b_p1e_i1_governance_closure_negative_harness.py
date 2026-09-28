#!/usr/bin/env python3
"""Mutations that the I1 governance-closure checker must reject."""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1e_i1_governance_closure_check.py"


def load(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_i1_closure_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load closure checker")
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


def duplicate_key(root: Path, module: Any) -> None:
    replace(root, module.INVENTORY, '  "schema_version": 1,', '  "schema_version": 1,\n  "schema_version": 1,')


CASES: list[tuple[str, Callable[[Path, Any], None]]] = [
    ("duplicate-json-key", duplicate_key),
    ("candidate-commit", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["accepted_candidate"].update(commit="0" * 40))),
    ("candidate-tree", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["accepted_candidate"].update(tree="0" * 40))),
    ("archive-digest", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["accepted_candidate"].update(archive_sha256="0" * 64))),
    ("review-digest", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["independent_review"].update(sha256="0" * 64))),
    ("status-reopened", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value.update(status="OPEN"))),
    ("production-change", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["scope"].update(production_source_changed_by_closure=True))),
    ("i1-not-closed", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["transition"].update(i1_closed=False))),
    ("implementation-authorized", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["transition"].update(p1f_implementation_authorized=True))),
    ("activation-authorized", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["transition"].update(p1f_operational_activation_authorized=True))),
    ("redis-opened", lambda root, module: edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(operational_redis_db15=True))),
    ("document-activation", lambda root, module: replace(root, module.DOCUMENT, "No operational action is performed", "Operational action is performed")),
    ("status-activation", lambda root, module: replace(root, module.STATUS, "Design authority is not activation authority", "Design authority is activation authority")),
    ("matrix-optional", lambda root, module: replace(root, module.MATRIX, ",REQUIRED\nI1CLOSE-008", ",OPTIONAL\nI1CLOSE-008")),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-i1-governance-closure-") as directory:
        baseline = Path(directory) / "baseline"
        shutil.copytree(
            ROOT,
            baseline,
            ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", ".env", "*.log", "__pycache__"),
        )
        load(0).validate(baseline, verify_lineage=False)
        for index, (name, mutate) in enumerate(CASES, start=1):
            root = Path(directory) / f"case-{index:02}"
            shutil.copytree(baseline, root)
            module = load(index)
            mutate(root, module)
            try:
                module.validate(root, verify_lineage=False)
            except module.CheckFailure:
                print(f"PASS {name}")
                continue
            print(f"FAIL {name}: mutation accepted")
            return 1
    print(f"stage8b-p1e-i1-governance-closure-negative-harness: PASS {len(CASES)}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
