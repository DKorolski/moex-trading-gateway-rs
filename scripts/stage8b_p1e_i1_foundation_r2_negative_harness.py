#!/usr/bin/env python3
"""Mutation harness for the Foundation R2 fatal owner-loss contract."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p1e_i1_foundation_r2_check.py")
SOURCE = Path("crates/runtime-durable-service/src/stage8b_p1_supervisor.rs")
LIB = Path("crates/runtime-durable-service/src/lib.rs")
BOUNDARY = Path("docs/stage-8/stage8b-p1e-i1-supervisor-foundation-r2-review-boundary.md")
FILES = (CHECKER, SOURCE, LIB, BOUNDARY)


def copy_fixture(target: Path) -> None:
    for relative in FILES:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)


def replace(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if old not in text:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(text.replace(old, new, 1))


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / CHECKER)],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )


def main() -> None:
    cases: list[tuple[str, Callable[[Path], None]]] = [
        ("owner-loss-exit-zero", lambda root: replace(root, SOURCE, "                    Some(70),", "                    Some(0),")),
        ("owner-loss-outcome-removed", lambda root: replace(root, SOURCE, "                    Some(Stage8bP1eTerminalFailureV1::OwnerLost),", "                    None,")),
        ("terminal-field-removed", lambda root: replace(root, SOURCE, "    pub terminal_failure: Option<Stage8bP1eTerminalFailureV1>,\n", "")),
        ("terminal-export-removed", lambda root: replace(root, LIB, " Stage8bP1eTelemetryEnvelopeV1, Stage8bP1eTerminalFailureV1,", " Stage8bP1eTelemetryEnvelopeV1,")),
        ("external-panic-test-removed", lambda root: replace(root, SOURCE, "coordinator_external_signal_then_owner_panic_is_fatal_without_losing_diagnostics", "removed_external_panic_case")),
        ("external-ownerless-test-removed", lambda root: replace(root, SOURCE, "coordinator_external_signal_then_ownerless_return_is_fatal", "removed_external_ownerless_case")),
        ("telemetry-panic-test-removed", lambda root: replace(root, SOURCE, "coordinator_telemetry_failure_then_owner_panic_retains_initiating_diagnostics", "removed_telemetry_panic_case")),
        ("grace-precedence-test-removed", lambda root: replace(root, SOURCE, "coordinator_observed_owner_loss_precedes_unprocessed_elapsed_grace", "removed_grace_precedence_case")),
        ("redis-race-regression", lambda root: replace(root, SOURCE, "cleanup_atomically_retains_consumer_that_gains_pending_after_discovery", "removed_atomic_race_case")),
        ("boundary-precedence-removed", lambda root: replace(root, BOUNDARY, "takes precedence over a grace deadline", "is subordinate to a grace deadline")),
    ]

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-foundation-r2-") as temporary:
        base = Path(temporary) / "base"
        copy_fixture(base)
        baseline = run(base)
        if baseline.returncode != 0:
            raise SystemExit(f"baseline checker failed:\n{baseline.stdout}")
        for index, (name, mutate) in enumerate(cases, start=1):
            case = Path(temporary) / f"case-{index:02d}"
            copy_fixture(case)
            mutate(case)
            completed = run(case)
            if completed.returncode == 0:
                raise SystemExit(f"mutation unexpectedly passed: {name}")
            print(f"PASS {name}")
    print(f"stage8b-p1e-i1-foundation-r2-negative-harness: ok cases={len(cases)}")


if __name__ == "__main__":
    main()
