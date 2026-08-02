#!/usr/bin/env python3
"""Run the accepted R1-b R2 predecessor and its authority chain detached."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "e7b133daa73026c0b7d1b82be368013ff9328667"
R2_BASE = "e6d2d94d709ff2f6b589a565e255dbb0049d2705"
R1_BASE = "7724b4472d603b3c2ef7c3ff22aa371aa64d8592"
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
    for label, commit in (
        ("R2 base", R2_BASE),
        ("R1 base", R1_BASE),
        ("Stage 5C authority", AUTHORITY),
    ):
        candidate = subprocess.check_output(
            ["git", "rev-parse", commit], cwd=ROOT, text=True
        ).strip()
        if candidate != commit:
            raise SystemExit(f"stage5g-d-predecessor-gate: FAIL: {label} missing")
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
        r2_base = temp / "r2-base"
        extract_commit(R2_BASE, r2_base)
        for command in commands:
            subprocess.run(command, cwd=r2_base, check=True)
        r1_base = temp / "r1-base"
        extract_commit(R1_BASE, r1_base)
        for command in commands:
            subprocess.run(command, cwd=r1_base, check=True)
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
