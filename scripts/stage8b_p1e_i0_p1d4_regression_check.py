#!/usr/bin/env python3
"""Reuse accepted P1-d4 semantic validation without its historical diff scope."""

from __future__ import annotations

import pathlib
import subprocess

import stage8b_p1d4_source_check as p1d4


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACCEPTED_DESIGN = "1a1ea05775f1d15b86fcc3495ad6863b851e9212"
IMMUTABLE_DESIGN_ARTIFACTS = (
    "docs/stage-8/fixtures/stage8b-p1d4-command-publication-binding-v1.json",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv",
    "docs/stage-8/stage8b-p1d4-r7-acceptance-amendment.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r7.md",
    "docs/stage-8/stage8b-p1d4-source-shape-r7.json",
    "scripts/make_stage8b_p1d4_r7_design_handoff.py",
    "scripts/stage8b_p1d4_r7_design_check.py",
    "scripts/stage8b_p1d4_r7_design_gate.sh",
    "scripts/stage8b_p1d4_r7_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r7_design_negative_harness.py",
)


def main() -> None:
    baseline = subprocess.run(
        ["git", "diff", "--quiet", ACCEPTED_DESIGN, "--", *IMMUTABLE_DESIGN_ARTIFACTS],
        cwd=ROOT,
    )
    if baseline.returncode != 0:
        raise SystemExit("stage8b-p1e-i0-p1d4-regression-check: FAIL immutable P1-d4 design artifact drift")
    try:
        p1d4.validate_content(p1d4.load_content())
    except (p1d4.CheckFailure, OSError, ValueError, KeyError, TypeError) as error:
        print(f"stage8b-p1e-i0-p1d4-regression-check: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1e-i0-p1d4-regression-check "
        f"immutable_design_artifacts={len(IMMUTABLE_DESIGN_ARTIFACTS)} content_validator=reused historical_scope_gate_invoked=false"
    )


if __name__ == "__main__":
    main()
