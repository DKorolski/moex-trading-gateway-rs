#!/usr/bin/env python3
"""Verify accepted Stage 5G-e-a detached from current Stage 5G-e-b work."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "0c1f1ce61c11c311e5df42edd4ed8c35beb838d2"


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-eb-predecessor-gate: FAIL: accepted Stage 5G-e-a missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-predecessor-") as raw:
        root = Path(raw)
        archive = root / "source.tar"
        with archive.open("wb") as handle:
            subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=handle, check=True)
        source = root / "source"
        source.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(source)
        for command in (
            ["python3", "scripts/stage5g_e_check.py", "--skip-git"],
            ["python3", "scripts/stage5g_e_negative_harness.py"],
        ):
            subprocess.run(command, cwd=source, check=True)
    # The nested accepted Stage 5G-d proof needs repository history, so run its
    # detached verifier from the real repository after the 0c1 source snapshot
    # itself has passed.
    subprocess.run(["python3", "scripts/stage5g_e_predecessor_gate.py"], cwd=ROOT, check=True)
    print(f"stage5g-eb-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
