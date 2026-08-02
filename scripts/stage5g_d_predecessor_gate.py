#!/usr/bin/env python3
"""Run the accepted replay-identity gates against detached d7561e6 sources."""

from __future__ import annotations

import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "d7561e6f36d01aea3d0dd67892800fbb6ac0a716"


def main() -> int:
    resolved = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
    if resolved != BASE:
        raise SystemExit("stage5g-d-predecessor-gate: FAIL: accepted predecessor missing")
    with tempfile.TemporaryDirectory(prefix="stage5g-d-predecessor-") as raw:
        temp = Path(raw)
        archive = temp / "source.tar"
        with archive.open("wb") as handle:
            subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=handle, check=True)
        source = temp / "source"
        source.mkdir()
        with tarfile.open(archive) as bundle:
            # The archive is produced locally by `git archive` from the pinned
            # accepted tree, not from an external input.
            bundle.extractall(source)
        commands = (
            ["python3", "scripts/stage5g_c_replay_identity_authority_check.py"],
            ["python3", "scripts/stage5g_c_replay_identity_negative_harness.py"],
        )
        for command in commands:
            subprocess.run(command, cwd=source, check=True)
    print(f"stage5g-d-predecessor-gate: PASS source_ref={BASE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
