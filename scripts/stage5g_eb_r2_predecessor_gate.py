#!/usr/bin/env python3
"""Verify accepted Stage 5G-e-b R1 detached from current R2 work."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "1621307a6012fa1f9dcbc89a59651c801f6cc26f"


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-eb-r2-predecessor-gate: FAIL: accepted R1 missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-r2-predecessor-") as raw:
        root = Path(raw)
        archive = root / "source.tar"
        with archive.open("wb") as handle:
            subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=handle, check=True)
        source = root / "source"
        source.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(source)
        for command in (
            ["python3", "scripts/stage5g_eb_r1_check.py", "--skip-git"],
            ["python3", "scripts/stage5g_eb_r1_negative_harness.py"],
        ):
            subprocess.run(command, cwd=source, check=True)
    subprocess.run(["python3", "scripts/stage5g_eb_r1_predecessor_gate.py"], cwd=ROOT, check=True)
    print(f"stage5g-eb-r2-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
