#!/usr/bin/env python3
"""Build the push-bound self-attesting Stage 5G-c R2-b review archive."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_stage5g_b_r1_handoff_archive as inherited
import stage5g_c_r2b_handoff_safety_check as stage5g_c_r2b

inherited.safety = stage5g_c_r2b.inherited
inherited.CONTRACT_PATH = "docs/stage-5/stage5g-c-contract.json"
_run = inherited.subprocess.run


def run_stage5g_c_r2b_verifier(command, *args, **kwargs):
    command = [
        "scripts/stage5g_c_r2b_handoff_safety_check.py"
        if item == "scripts/stage5g_b_r1_handoff_safety_check.py"
        else item
        for item in list(command)
    ]
    return _run(command, *args, **kwargs)


inherited.subprocess.run = run_stage5g_c_r2b_verifier


def main() -> int:
    head = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=inherited.ROOT, text=True
    ).strip()
    origin = subprocess.check_output(
        ["git", "rev-parse", "origin/stage5g-lifecycle"],
        cwd=inherited.ROOT,
        text=True,
    ).strip()
    if head != origin:
        raise SystemExit(
            "refusing Stage 5G-c R2-b handoff: origin/stage5g-lifecycle does not equal HEAD"
        )
    return inherited.main()


if __name__ == "__main__":
    raise SystemExit(main())
