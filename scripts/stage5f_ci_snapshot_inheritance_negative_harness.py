#!/usr/bin/env python3
"""Negative coverage for the Stage 5F CI/snapshot inheritance closure."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5f_ci_snapshot_inheritance_check.py"
WRAPPER = "scripts/stage5f_b3f_snapshot_provenance_gate.sh"
CI = ".github/workflows/ci.yml"
IGNORED = shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__", ".DS_Store")


@dataclass(frozen=True)
class Case:
    name: str
    expected_marker: str
    mutate: Callable[[Path], None]


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if source.count(old) != 1:
        raise AssertionError(f"{label}: expected one replacement anchor")
    return source.replace(old, new, 1)


def run_checker_case(case: Case) -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-negative-") as directory:
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


def run_missing_snapshot_case() -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5f-ci-missing-snapshot-") as directory:
        candidate = Path(directory) / "candidate"
        shutil.copytree(ROOT, candidate, ignore=IGNORED)
        completed = subprocess.run(
            ["bash", str(candidate / WRAPPER)],
            cwd=candidate,
            capture_output=True,
            text=True,
        )
        output = completed.stdout + completed.stderr
        if completed.returncode == 0:
            print("expected failure missing for accepted-snapshot-unavailable", file=sys.stderr)
            return False
        if "accepted B3F snapshot commit unavailable" not in output:
            print(
                "expected missing-snapshot marker absent for accepted-snapshot-unavailable:\n"
                + output,
                file=sys.stderr,
            )
            return False
    print("PASS accepted-snapshot-unavailable")
    return True


def main() -> int:
    cases = [
        Case(
            "accepted-snapshot-ref-rebound",
            "accepted B3F snapshot pin drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    'accepted_b3f_ref="e14654f7129aa61011931306140a3bfefe2fcfbc"',
                    'accepted_b3f_ref="0000000000000000000000000000000000000000"',
                    "accepted snapshot ref",
                )
            ),
        ),
        Case(
            "legacy-stage5e-gate-restored",
            "legacy Stage 5E gate runs on Stage5F head",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "- name: Stage 5F atomic Hybrid semantics gate\n"
                    "        run: bash scripts/stage5f_atomic_hybrid_semantics_gate.sh",
                    "- name: Stage 5E lifecycle event-time gate\n"
                    "        run: bash scripts/stage5e_lifecycle_event_time_gate.sh",
                    "legacy Stage 5E gate",
                )
            ),
        ),
        Case(
            "provenance-redirected-to-head",
            "accepted B3F snapshot checkout drift",
            lambda root: (root / WRAPPER).write_text(
                replace_once(
                    (root / WRAPPER).read_text(),
                    'git -C "$snapshot_root" checkout --quiet --detach "$accepted_b3f_ref"',
                    'git -C "$snapshot_root" checkout --quiet --detach HEAD',
                    "snapshot checkout",
                )
            ),
        ),
        Case(
            "stage5f-negative-harness-omitted",
            "Stage 5F negative harness omitted from CI",
            lambda root: (root / CI).write_text(
                replace_once(
                    (root / CI).read_text(),
                    "      - name: Stage 5F atomic Hybrid negative harness\n"
                    "        run: python3 scripts/stage5f_atomic_hybrid_semantics_negative_harness.py\n"
                    "        timeout-minutes: 5\n\n",
                    "",
                    "Stage 5F negative CI step",
                )
            ),
        ),
    ]
    failures = [case.name for case in cases if not run_checker_case(case)]
    if not run_missing_snapshot_case():
        failures.append("accepted-snapshot-unavailable")
    if failures:
        print("FAIL " + ", ".join(failures), file=sys.stderr)
        return 1
    print("stage5f-ci-snapshot-inheritance-negative-harness: ok cases=5")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
