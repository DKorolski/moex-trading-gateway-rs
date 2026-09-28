#!/usr/bin/env python3
"""Mutation harness for the I1 foundation R1 static contract gate."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage8b_p1e_i1_foundation_r1_check.py")
FILES = (
    CHECKER,
    Path("crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"),
    Path("crates/runtime-durable-service/src/lib.rs"),
    Path("docs/stage-8/stage8b-p1e-redis-runtime-policy-v2.json"),
    Path("docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua"),
    Path("docs/stage-8/stage8b-p1e-i1-supervisor-foundation-review-boundary.md"),
)


def copy_fixture(target: Path) -> None:
    for relative in FILES:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, destination)


def replace(path: Path, old: str, new: str) -> None:
    value = path.read_text()
    if old not in value:
        raise RuntimeError(f"mutation anchor missing in {path}: {old}")
    path.write_text(value.replace(old, new, 1))


def policy_mutation(root: Path, change: Callable[[dict], None]) -> None:
    path = root / "docs/stage-8/stage8b-p1e-redis-runtime-policy-v2.json"
    value = json.loads(path.read_text())
    change(value)
    path.write_text(json.dumps(value, indent=2) + "\n")


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
        (
            "allow-nonzero-pending-delete",
            lambda root: policy_mutation(
                root,
                lambda value: value["stale_consumer_hygiene"].update(
                    nonzero_pending_delete_allowed=True
                ),
            ),
        ),
        (
            "trust-discovery-snapshot",
            lambda root: policy_mutation(
                root,
                lambda value: value["stale_consumer_hygiene"].update(
                    discovery_snapshot_is_authoritative_for_delete=True
                ),
            ),
        ),
        (
            "unsafe-cleanup-fallback",
            lambda root: policy_mutation(
                root,
                lambda value: value["stale_consumer_hygiene"]["atomic_delete"].update(
                    fallback_when_atomic_guarantee_unavailable="delete-from-snapshot"
                ),
            ),
        ),
        (
            "lua-contract-drift",
            lambda root: (root / "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua").write_text(
                "return 1\n"
            ),
        ),
        (
            "non-atomic-command",
            lambda root: replace(
                root / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
                'redis::cmd("EVAL")',
                'redis::cmd("XGROUP")',
            ),
        ),
        (
            "remove-race-test",
            lambda root: replace(
                root / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs",
                "cleanup_atomically_retains_consumer_that_gains_pending_after_discovery",
                "cleanup_race_proof_removed",
            ),
        ),
        (
            "restore-stateless-coordinator",
            lambda root: (
                root / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
            ).write_text(
                (root / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs").read_text()
                + "\nfn stage8b_p1e_coordinate_event_v1() {}\n"
            ),
        ),
        (
            "drop-stateful-export",
            lambda root: replace(
                root / "crates/runtime-durable-service/src/lib.rs",
                "Stage8bP1eCoordinatorDecisionV1, Stage8bP1eCoordinatorV1,",
                "Stage8bP1eCoordinatorDecisionV1,",
            ),
        ),
    ]

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1-foundation-r1-") as temporary:
        base = Path(temporary) / "base"
        copy_fixture(base)
        baseline = run(base)
        if baseline.returncode != 0:
            raise SystemExit(f"baseline checker failed:\n{baseline.stdout}")

        for index, (name, mutate) in enumerate(cases, start=1):
            case = Path(temporary) / f"case-{index:02}"
            copy_fixture(case)
            mutate(case)
            completed = run(case)
            if completed.returncode == 0:
                raise SystemExit(f"mutation unexpectedly passed: {name}")
            print(f"PASS {name}")

    print(f"stage8b-p1e-i1-foundation-r1-negative-harness: ok cases={len(cases)}")


if __name__ == "__main__":
    main()
