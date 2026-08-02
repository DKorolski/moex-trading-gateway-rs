#!/usr/bin/env python3
"""Build the push-bound self-attesting Stage 5G-c R2-c-b R1 archive."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_stage5g_c_r2ca_r3_handoff_archive as builder
import stage5g_c_r2cb_r1_handoff_safety_check as safety

builder.safety = safety
builder.CONTRACT_PATH = (
    "docs/stage-5/stage5g-c-r2cb-r1-historical-trade-replay-chronology.json"
)

_run = builder.subprocess.run


def _stage_specific_run(command, *args, **kwargs):
    if isinstance(command, list):
        command = [
            "scripts/stage5g_c_r2cb_r1_handoff_safety_check.py"
            if item == "scripts/stage5g_c_r2ca_r3_handoff_safety_check.py"
            else item
            for item in command
        ]
    return _run(command, *args, **kwargs)


builder.subprocess.run = _stage_specific_run

if __name__ == "__main__":
    raise SystemExit(builder.main())
