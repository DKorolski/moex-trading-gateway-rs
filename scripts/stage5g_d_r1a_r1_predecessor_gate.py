#!/usr/bin/env python3
"""Re-run accepted R1-a authority after stripping only R1 successor regions."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
PREDECESSOR_CHECKER = Path("scripts/stage5g_d_r1a_authority_check.py")
TAG = "complete-precallback-transactional-admission-v1"
PREFIXES = (
    "STAGE5G-D-R1A-R1-AUTHORITY",
    "STAGE5G-D-R1A-R1-AUTHORITY-TESTS",
)


def strip_region(source: str, prefix: str) -> str:
    begin = f"// {prefix}-BEGIN: {TAG}"
    end = f"// {prefix}-END: {TAG}"
    pattern = rf"(?m)^\s*{re.escape(begin)}\n.*?^\s*{re.escape(end)}\n"
    stripped, count = re.subn(pattern, "", source, count=1, flags=re.S)
    if count != 1:
        raise RuntimeError(f"cannot strip exactly one R1 region: {prefix}")
    return stripped


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5g-d-r1a-r1-predecessor-") as raw:
        detached = Path(raw) / "repo"
        shutil.copytree(ROOT, detached, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "*.zip"))
        source_path = detached / STAGE5C
        source = source_path.read_text()
        for prefix in PREFIXES:
            source = strip_region(source, prefix)
        source_path.write_text(source)
        result = subprocess.run(
            [sys.executable, str(detached / PREDECESSOR_CHECKER), "--root", str(detached)],
            cwd=detached,
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            sys.stdout.write(result.stdout)
            sys.stderr.write(result.stderr)
            print("stage5g-d-r1a-r1-predecessor-gate: FAIL", file=sys.stderr)
            return 1
    print("stage5g-d-r1a-r1-predecessor-gate: PASS")
    print("accepted_r1a_authority: exact after stripping 2/2 R1 regions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
