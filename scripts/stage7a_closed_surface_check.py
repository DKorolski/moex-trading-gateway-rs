#!/usr/bin/env python3
"""Exact Stage 7A delta and closed live-surface checker."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage7a_check as checker

EXACT = {
    "Cargo.lock",
    "Cargo.toml",
    "crates/runtime-command-bridge/Cargo.toml",
    "crates/runtime-command-bridge/src/lib.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "docs/roadmap.md",
    "docs/stage-7/STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv",
    "docs/stage-7/TZ_STAGE7A_REDIS_COMMAND_CONSUMER_PAPER_MOCK_2026-08-11.md",
    "docs/stage-7/stage7a-entry-descriptor.json",
    "docs/stage-7/stage7a-implementation.md",
    "docs/stage-7/stage7a-slice-plan.md",
    "scripts/make_stage7a_handoff_archive.py",
    "scripts/stage7a_check.py",
    "scripts/stage7a_closed_surface_check.py",
    "scripts/stage7a_gate.sh",
    "scripts/stage7a_negative_harness.py",
    "scripts/stage7a_preseal_check.py",
}


def main() -> None:
    root = Path.cwd().resolve()
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", checker.BASE], cwd=root, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
    ).splitlines()
    touched = set(changed + untracked)
    if touched != EXACT:
        raise SystemExit(
            "stage7a-closed-surface: FAIL: "
            f"missing={sorted(EXACT - touched)} extra={sorted(touched - EXACT)}"
        )
    checker.check(root)
    print(
        "stage7a-closed-surface: PASS "
        f"changed={len(touched)} paper_redis=true finam_post_delete=false "
        "broker_network=false runtime_live=false real_orders=false protective=false"
    )


if __name__ == "__main__":
    main()
