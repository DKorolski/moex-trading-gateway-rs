#!/usr/bin/env python3
"""Verify accepted Stage 5G-d R5 detached from current Stage 5G-e work."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "54e26c886afd97cd443fd8b0728fe180ff4793b5"


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-e-predecessor-gate: FAIL: accepted Stage 5G-d missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-e-predecessor-") as raw:
        root = Path(raw)
        archive = root / "source.tar"
        with archive.open("wb") as handle:
            subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=handle, check=True)
        source = root / "source"
        source.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(source)
        for command in (
            ["python3", "scripts/stage5g_d_check.py", "--skip-git"],
            ["python3", "scripts/stage5g_d_negative_harness.py"],
        ):
            subprocess.run(command, cwd=source, check=True)
    print(f"stage5g-e-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
