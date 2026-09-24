#!/usr/bin/env python3
"""Semantic mutations that the I1 aggregate checker must reject."""

from __future__ import annotations

import importlib.util
import json
import shutil
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_p1e_i1_aggregate_acceptance_check.py"


def load_checker(index: int) -> Any:
    spec = importlib.util.spec_from_file_location(f"stage8b_i1_acceptance_case_{index}", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load aggregate checker")
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
    path = root / module.INVENTORY
    path.write_text(path.read_text().replace('  "schema_version": 1,', '  "schema_version": 1,\n  "schema_version": 1,', 1))


def milestone_commit(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_milestones"][15].update(commit="0" * 40))


def milestone_review(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_milestones"][14].update(review_sha256="f" * 64))


def milestone_extra_field(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_milestones"][0].update(note="unbound"))


def component_archive(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_components"]["fixed_installation"].update(archive_sha256="0" * 64))


def target_count(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["accepted_components"]["fixed_installation"].update(target_evidence_files=20))


def self_accept(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value.update(status="ACCEPTED"))


def close_i1(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["aggregate_result"].update(i1_closed=True))


def authorize_p1f(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(p1f_authorized=True))


def open_live(root: Path, module: Any) -> None:
    edit_json(root, module.INVENTORY, lambda value: value["closed_surfaces"].update(runtime_live=True))


def remove_matrix_row(root: Path, module: Any) -> None:
    path = root / module.MATRIX
    path.write_text("\n".join(path.read_text().splitlines()[:-1]) + "\n")


def optional_matrix_row(root: Path, module: Any) -> None:
    replace(root, module.MATRIX, "I1ACC-010,redis,P1-c", "I1ACC-010,redis,P1-c")
    path = root / module.MATRIX
    path.write_text(path.read_text().replace(",REQUIRED\nI1ACC-011", ",OPTIONAL\nI1ACC-011", 1))


def mutate_matrix_requirement(root: Path, module: Any) -> None:
    replace(root, module.MATRIX, "only against an isolated subprocess instance", "against any Redis instance")


def document_self_accept(root: Path, module: Any) -> None:
    replace(root, module.DOCUMENT, "REVIEW CANDIDATE — I1 NOT CLOSED", "ACCEPTED — I1 CLOSED")


def remove_review_authority(root: Path, module: Any) -> None:
    replace(root, module.DOCUMENT, "Only independent review", "This commit")


def stale_fixed_count(root: Path, module: Any) -> None:
    replace(root, module.FIXED_DOCUMENT, "34 source/material and 14", "25 source/material and ten")


def status_activation(root: Path, module: Any) -> None:
    replace(root, module.STATUS, "cannot self-close I1", "closes I1")


def roadmap_activation(root: Path, module: Any) -> None:
    replace(root, module.ROADMAP, "I1 remains open until independent aggregate review", "I1 is closed")


CASES = [
    ("duplicate-json-key", duplicate_key),
    ("fixed-milestone-rebound", milestone_commit),
    ("telemetry-review-rebound", milestone_review),
    ("milestone-extra-field", milestone_extra_field),
    ("fixed-archive-rebound", component_archive),
    ("target-evidence-count-reduced", target_count),
    ("aggregate-self-accepted", self_accept),
    ("i1-self-closed", close_i1),
    ("p1f-authorized", authorize_p1f),
    ("runtime-live-opened", open_live),
    ("matrix-row-removed", remove_matrix_row),
    ("matrix-row-optional", optional_matrix_row),
    ("matrix-requirement-drift", mutate_matrix_requirement),
    ("document-self-accepted", document_self_accept),
    ("independent-review-removed", remove_review_authority),
    ("fixed-count-stale", stale_fixed_count),
    ("status-activation", status_activation),
    ("roadmap-activation", roadmap_activation),
]


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage8b-i1-acceptance-") as temp:
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
    print(f"stage8b-p1e-i1-aggregate-acceptance-negative-harness: PASS {len(CASES)}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
