#!/usr/bin/env python3
"""Exact 48-case mutation harness for the Stage 8A-4 design freeze."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_design_check as scanner


def mutate_json(path: Path, mutation) -> None:
    value = json.loads(path.read_text())
    mutation(value)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def set_path(*path_and_value):
    *path, value = path_and_value

    def apply(document):
        target = document
        for key in path[:-1]:
            target = target[key]
        target[path[-1]] = value

    return apply


def remove_source(name: str):
    return lambda value: value["required_truth_sources"].remove(name)


def remove_outcome(name: str):
    return lambda value: value["outcomes"].remove(name)


def swap_correlation(first: int, second: int):
    def apply(value):
        order = value["correlation_precedence"]
        order[first], order[second] = order[second], order[first]

    return apply


STRUCTURAL_CASES = [
    ("accepted predecessor drift", set_path("accepted_predecessor", "forged")),
    ("accepted review drift", set_path("accepted_predecessor_review_sha256", "0" * 64)),
    ("design-only disabled", set_path("design_only", False)),
    ("implementation predeclared", set_path("production_reconciliation_implemented", True)),
    ("orders source removed", remove_source("orders")),
    ("trades source removed", remove_source("trades")),
    ("positions source removed", remove_source("positions")),
    ("instrument registry removed", remove_source("instrument_registry")),
    ("client identity demoted", swap_correlation(0, 1)),
    ("shape correlation promoted", swap_correlation(0, 2)),
    ("Conflict removed", remove_outcome("Conflict")),
    ("StillUnknown removed", remove_outcome("StillUnknown")),
    ("trades made authoritative", set_path("supporting_evidence_only", ["target_instrument_position"])),
    ("position made authoritative", set_path("supporting_evidence_only", ["trades"])),
    ("ProvenNoMatch opened", set_path("closed", "proven_no_match", False)),
    ("same-request retry opened", set_path("closed", "same_request_retry", False)),
    ("network transport opened", set_path("closed", "network_transport", False)),
    ("runtime-live opened", set_path("closed", "runtime_live", False)),
]

MARKER_CASES = list(scanner.FORBIDDEN_CONTRACT_MARKERS)


def main() -> int:
    if len(STRUCTURAL_CASES) != 18 or len(MARKER_CASES) != 30:
        raise SystemExit("negative case inventory drift")
    copied = scanner.ALLOWED_CHANGED_PATHS
    for index, (name, mutation) in enumerate(STRUCTURAL_CASES, 1):
        with tempfile.TemporaryDirectory(prefix="stage8a4-design-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                source = scanner.ROOT / relative
                if source.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
            mutate_json(root / scanner.AUTHORITY, mutation)
            try:
                scanner.check(root, git_scope=False)
            except Exception:
                print(f"PASS {index:02d} {name}")
            else:
                print(f"FAIL {index:02d} {name}: mutation accepted")
                return 1

    for offset, marker in enumerate(MARKER_CASES, len(STRUCTURAL_CASES) + 1):
        with tempfile.TemporaryDirectory(prefix="stage8a4-design-negative-") as raw:
            root = Path(raw)
            for relative in copied:
                source = scanner.ROOT / relative
                if source.is_file():
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, target)
            contract = root / scanner.CONTRACT
            contract.write_text(contract.read_text() + f"\n{marker}\n")
            try:
                scanner.check(root, git_scope=False)
            except Exception:
                print(f"PASS {offset:02d} {marker}")
            else:
                print(f"FAIL {offset:02d} {marker}: mutation accepted")
                return 1

    print("stage8a4-design-r1-negative: PASS cases=48/48")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
