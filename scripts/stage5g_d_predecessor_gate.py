#!/usr/bin/env python3
"""Run accepted R1-b and its authority chain from detached 7724b44 sources."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "7724b4472d603b3c2ef7c3ff22aa371aa64d8592"
AUTHORITY = "d0494537d7c1739a16350b2d28f71b304165c812"


def extract_commit(commit: str, destination: Path) -> None:
    archive = destination.parent / f"{commit[:7]}.tar"
    with archive.open("wb") as handle:
        subprocess.run(["git", "archive", commit], cwd=ROOT, stdout=handle, check=True)
    destination.mkdir()
    with tarfile.open(archive) as bundle:
        # Produced locally by `git archive` from a pinned repository tree.
        bundle.extractall(destination)


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-d-predecessor-gate: FAIL: accepted predecessor missing")
    authority_resolved = subprocess.check_output(
        ["git", "rev-parse", AUTHORITY], cwd=ROOT, text=True
    ).strip()
    if authority_resolved != AUTHORITY:
        raise SystemExit("stage5g-d-predecessor-gate: FAIL: Stage 5C authority missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-d-predecessor-") as raw:
        temp = Path(raw)
        source = temp / "source"
        extract_commit(BASE, source)
        commands = (
            ["python3", "scripts/stage5g_d_check.py", "--skip-git"],
            ["python3", "scripts/stage5g_d_negative_harness.py"],
        )
        for command in commands:
            subprocess.run(command, cwd=source, check=True)
        authority = temp / "authority"
        extract_commit(AUTHORITY, authority)
        for command in (
            ["python3", "scripts/stage5g_d_r1a_r1_authority_check.py"],
            ["python3", "scripts/stage5g_d_r1a_r1_negative_harness.py"],
        ):
            subprocess.run(command, cwd=authority, check=True)
    print(f"stage5g-d-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
