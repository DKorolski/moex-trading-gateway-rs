#!/usr/bin/env python3
"""Isolated negative checks for the Stage 5F-a entry contract."""

from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5f_atomic_hybrid_semantics_entry_check.py"
INVENTORY = "docs/stage-5/stage5f-a-atomic-hybrid-semantics-entry-inventory.json"
PLAN = "docs/stage-5/5f-a-atomic-hybrid-semantics-entry.md"
IGNORED = shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__", ".DS_Store")


@dataclass(frozen=True)
class Case:
    name: str
    expected_marker: str
    mutate: Callable[[Path], None]


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if source.count(old) != 1:
        raise AssertionError(f"{label}: expected one replacement anchor")
    return source.replace(old, new, 1)


def rebind_inventory(root: Path, mutator: Callable[[dict[str, object]], None]) -> None:
    inventory_path = root / INVENTORY
    payload = json.loads(inventory_path.read_text())
    mutator(payload)
    inventory_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    checker_path = root / CHECKER
    checker = checker_path.read_text()
    checker, count = re.subn(
        r'EXPECTED_INVENTORY_SHA256 = "[0-9a-f]{64}"',
        f'EXPECTED_INVENTORY_SHA256 = "{canonical_sha256(payload)}"',
        checker,
        count=1,
    )
    if count != 1:
        raise AssertionError("inventory rebind: checker hash pin not found")
    checker_path.write_text(checker)


def rebind_plan(root: Path, mutator: Callable[[str], str]) -> None:
    plan_path = root / PLAN
    plan_path.write_text(mutator(plan_path.read_text()))
    digest = hashlib.sha256(plan_path.read_bytes()).hexdigest()
    checker_path = root / CHECKER
    checker = checker_path.read_text()
    checker, count = re.subn(
        r'EXPECTED_PLAN_SHA256 = "[0-9a-f]{64}"',
        f'EXPECTED_PLAN_SHA256 = "{digest}"',
        checker,
        count=1,
    )
    if count != 1:
        raise AssertionError("plan rebind: checker hash pin not found")
    checker_path.write_text(checker)


def mutate_descriptor(root: Path) -> None:
    path = root / "docs/stage-5/stage5e-active-descriptor.json"
    path.write_text(json.dumps({"schema_version": 1, "stage": "5E-b3e-callback-invocation-design"}) + "\n")


def mutate_b3f_source(root: Path, relative: str) -> None:
    path = root / relative
    path.write_text(path.read_text() + "\n// stage5f-negative-mutation\n")


def run_case(case: Case) -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-negative-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        case.mutate(candidate)
        completed = subprocess.run(
            [sys.executable, str(candidate / CHECKER)],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print(f"expected failure missing for {case.name}", file=sys.stderr)
            return False
        if case.expected_marker not in output:
            print(
                f"expected marker {case.expected_marker!r} missing for {case.name}:\n{output}",
                file=sys.stderr,
            )
            return False
    print(f"PASS {case.name}")
    return True


def main() -> int:
    cases = [
        Case(
            "b3f-source-ref-rebound",
            "accepted B3F closure pin drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["accepted_stage5e_b3f_closure"].__setitem__(
                    "source_ref", "0" * 40
                ),
            ),
        ),
        Case(
            "b3f-semantic-digest-rebound",
            "accepted B3F closure pin drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["accepted_stage5e_b3f_closure"].__setitem__(
                    "stage5e_region_semantic_token_sha256", "1" * 64
                ),
            ),
        ),
        Case(
            "target-instrument-rebound",
            "Stage 5F target contract drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["target_contract"].__setitem__(
                    "instrument_symbol", "RI"
                ),
            ),
        ),
        Case(
            "alternate-direct-stage5c-route",
            "Stage 5F sole route drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload.__setitem__(
                    "sole_route", ["Stage5cPaperHost::direct_callback"]
                ),
            ),
        ),
        Case(
            "partial-parity-accepted",
            "Stage 5F atomic contract drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["atomic_transition_contract"].__setitem__(
                    "partial_bo_or_mr_parity_acceptance_allowed", True
                ),
            ),
        ),
        Case(
            "arbitration-scenario-removed",
            "Stage 5F scenario matrix drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["required_atomic_scenarios"].remove(
                    "simultaneous_bo_mr_deterministic_winner"
                ),
            ),
        ),
        Case(
            "transport-surface-opened",
            "Stage 5F closed-surface drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["closed_surfaces"].__setitem__(
                    "finam_transport", True
                ),
            ),
        ),
        Case(
            "stage5g-feedback-opened",
            "Stage 5F later-stage boundary drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload["stage_boundaries"].__setitem__(
                    "stage5g_feedback_lifecycle_allowed", True
                ),
            ),
        ),
        Case(
            "negative-matrix-count-reduced",
            "Stage 5F negative case count drift",
            lambda root: rebind_inventory(
                root,
                lambda payload: payload.__setitem__(
                    "expected_stage5f_negative_case_count", 12
                ),
            ),
        ),
        Case(
            "accepted-b3f-descriptor-repointed",
            "accepted B3F descriptor drift",
            mutate_descriptor,
        ),
        Case(
            "accepted-b3f-stage5c-source-mutated",
            "accepted B3F source drift: crates/strategy-runtime-core/src/stage5c_paper_host.rs",
            lambda root: mutate_b3f_source(
                root, "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
            ),
        ),
        Case(
            "accepted-b3f-provenance-harness-mutated",
            "accepted B3F source drift: scripts/handoff_provenance_negative_harness.py",
            lambda root: mutate_b3f_source(
                root, "scripts/handoff_provenance_negative_harness.py"
            ),
        ),
        Case(
            "plan-direct-route-prohibition-removed",
            "Stage 5F plan authority fragment missing",
            lambda root: rebind_plan(
                root,
                lambda text: replace_once(
                    text,
                    "There is no alternate direct Stage 5C callback route, second orchestrator,",
                    "A direct callback route may be introduced later,",
                    "plan route prohibition",
                ),
            ),
        ),
    ]
    failures = [case.name for case in cases if not run_case(case)]
    if failures:
        print("FAIL " + ", ".join(failures), file=sys.stderr)
        return 1
    print(f"stage5f-atomic-hybrid-semantics-negative-harness: ok cases={len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
