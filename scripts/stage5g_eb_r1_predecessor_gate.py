#!/usr/bin/env python3
"""Verify rejected-base NewPackage work and accepted predecessor chain detached."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "cbe4044bbca8303a7852d225364ec5cf89f02386"


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-eb-r1-predecessor-gate: FAIL: cbe4044 base missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-r1-predecessor-") as raw:
        root = Path(raw)
        archive = root / "source.tar"
        with archive.open("wb") as handle:
            subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=handle, check=True)
        source = root / "source"
        source.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(source)
        for command in (
            ["python3", "scripts/stage5g_eb_check.py", "--skip-git"],
            ["python3", "scripts/stage5g_eb_negative_harness.py"],
        ):
            subprocess.run(command, cwd=source, check=True)
    subprocess.run(["python3", "scripts/stage5g_eb_predecessor_gate.py"], cwd=ROOT, check=True)
    print(f"stage5g-eb-r1-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
