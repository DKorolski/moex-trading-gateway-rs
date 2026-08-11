#!/usr/bin/env python3
"""Exact changed-path and closed execution-surface gate for Stage 6E-R1."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage6e_r1_check as checker

EXACT = {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "docs/roadmap.md",
    "docs/stage-6/stage6-slice-plan.md",
    "docs/stage-6/stage6e-r1-closure-descriptor.json",
    "docs/stage-6/stage6e-r1-closure.md",
    "scripts/make_stage6e_r1_handoff_archive.py",
    "scripts/stage6e_r1_check.py",
    "scripts/stage6e_r1_closed_surface_check.py",
    "scripts/stage6e_r1_gate.sh",
    "scripts/stage6e_r1_negative_harness.py",
    "scripts/stage6e_r1_preseal_check.py",
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
            "stage6e-r1-closed-surface: FAIL: "
            f"missing={sorted(EXACT-touched)} extra={sorted(touched-EXACT)}"
        )
    checker.check(root)
    print(
        "stage6e-r1-closed-surface: PASS "
        f"changed={len(touched)} redis=false finam_post_delete=false "
        "network_dispatch=false runtime_live=false real_orders=false protective=false"
    )


if __name__ == "__main__":
    main()
