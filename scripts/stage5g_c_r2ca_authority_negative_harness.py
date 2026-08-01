#!/usr/bin/env python3
"""Rehash-aware mutation test for the R2-c-a authority bundle."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2ca-market-terminal-authority.json")
CHECKER = Path("scripts/stage5g_c_r2ca_authority_check.py")
OLD_REGION = "1d98411788ec1e0b331a7377fc8efdc6074afcaac107c99ea30c8aba4e351202"


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, check=False, capture_output=True, text=True)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-negative-") as tmp:
        mutant = Path(tmp) / "repo"
        shutil.copytree(
            ROOT,
            mutant,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"),
        )
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        needle = "/// Canonical broker evidence accepted by the narrow Stage 5C authority"
        replacement = needle + " (mutated)"
        if source.count(needle) != 1:
            print("stage5g-c-r2ca-negative: FAIL: mutation anchor drift", file=sys.stderr)
            return 1
        source_path.write_text(source.replace(needle, replacement, 1))

        # Simulate an attacker rehashing source, descriptor and the local checker.
        import re

        updated = source_path.read_text()
        begin = "// STAGE5G-C-R2CA-AUTHORITY-BEGIN: market-terminal-no-callback-v1"
        end = "// STAGE5G-C-R2CA-AUTHORITY-END: market-terminal-no-callback-v1"
        match = re.search(
            rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n", updated, re.S
        )
        if match is None:
            print("stage5g-c-r2ca-negative: FAIL: region missing", file=sys.stderr)
            return 1
        new_region = sha(match.group(1).encode())
        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = sha(source_path.read_bytes())
        descriptor["regions"]["market-terminal-no-callback-v1"] = new_region
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")
        checker_path = mutant / CHECKER
        checker_text = checker_path.read_text()
        checker_path.write_text(checker_text.replace(OLD_REGION, new_region))

        local = run([sys.executable, str(CHECKER)], mutant)
        if local.returncode != 0:
            print(
                "stage5g-c-r2ca-negative: FAIL: rehashed local bundle did not self-authorize",
                file=sys.stderr,
            )
            print(local.stderr, file=sys.stderr)
            return 1
        detached = run(
            [sys.executable, str(ROOT / "scripts/stage5g_c_r2ca_snapshot_gate.py"), "--root", str(mutant)],
            ROOT,
        )
        if detached.returncode == 0:
            print(
                "stage5g-c-r2ca-negative: FAIL: detached snapshot accepted rehashed authority",
                file=sys.stderr,
            )
            return 1

    print("stage5g-c-r2ca-authority-negative-harness: PASS (1/1)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

