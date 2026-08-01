#!/usr/bin/env python3
"""Build the deterministic self-attesting Stage 5G-b R2 review archive."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_b_r2_handoff_safety_check  # patches inherited R1 machinery
import make_stage5g_b_r1_handoff_archive as inherited

# The inherited builder uses the already patched shared safety module. Point its
# final verifier at the R2 entrypoint while retaining the reviewed archive
# construction and source-tree/commit-object validation algorithm.
inherited.safety = stage5g_b_r2_handoff_safety_check.inherited
_run = inherited.subprocess.run


def run_r2_verifier(command, *args, **kwargs):
    command = list(command)
    command = [
        "scripts/stage5g_b_r2_handoff_safety_check.py"
        if item == "scripts/stage5g_b_r1_handoff_safety_check.py"
        else item
        for item in command
    ]
    return _run(command, *args, **kwargs)


inherited.subprocess.run = run_r2_verifier

if __name__ == "__main__":
    raise SystemExit(inherited.main())
