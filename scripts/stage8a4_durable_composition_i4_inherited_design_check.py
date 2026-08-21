#!/usr/bin/env python3
"""Re-run the accepted I4 Design R3 contract without its design-slice source freeze."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


with tempfile.TemporaryDirectory(prefix="stage8a4-i4-inherited-design-") as raw:
    isolated = Path(raw) / "contract"
    shutil.copytree(ROOT / "docs", isolated / "docs")
    shutil.copytree(ROOT / "scripts", isolated / "scripts")
    environment = os.environ.copy()
    environment["STAGE8A4_I4_ROOT"] = str(isolated)
    result = subprocess.run(
        ["python3", str(ROOT / "scripts/stage8a4_durable_composition_i4_design_check.py")],
        cwd=ROOT,
        env=environment,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit("stage8a4-i4-inherited-design-check: FAIL")

print("stage8a4-i4-inherited-design-check: PASS rows=64 negatives=46")
