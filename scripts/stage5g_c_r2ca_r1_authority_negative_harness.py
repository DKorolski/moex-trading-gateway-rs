#!/usr/bin/env python3
"""Rehash-aware mutation witness for R2-c-a R1 authority."""

from __future__ import annotations

import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2ca-r1-market-terminal-state-coherence.json")
CHECKER = Path("scripts/stage5g_c_r2ca_r1_authority_check.py")
SNAPSHOT = ROOT / "scripts/stage5g_c_r2ca_r1_snapshot_gate.py"
OLD_REGION = "63c09f197264f144c21fa650e53912b6fe9086a0cc7ceb115cc1cb2b754b709b"


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def run(command: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, check=False, capture_output=True, text=True)


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r1-negative-") as tmp:
        mutant = Path(tmp) / "repo"
        shutil.copytree(
            ROOT,
            mutant,
            ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log"),
        )
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        needle = "if !stage5cj_f64_eq(*last_position_qty, expected_position_qty)"
        replacement = (
            "if !stage5cj_f64_eq(*last_position_qty, expected_position_qty + 1.0)"
        )
        if source.count(needle) != 1:
            print("stage5g-c-r2ca-r1-negative: FAIL: mutation anchor drift", file=sys.stderr)
            return 1
        source_path.write_text(source.replace(needle, replacement, 1))

        begin = "// STAGE5G-C-R2CA-R1-AUTHORITY-BEGIN: market-terminal-state-coherence-v1"
        end = "// STAGE5G-C-R2CA-R1-AUTHORITY-END: market-terminal-state-coherence-v1"
        match = re.search(
            rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n",
            source_path.read_text(),
            re.S,
        )
        if match is None:
            print("stage5g-c-r2ca-r1-negative: FAIL: region missing", file=sys.stderr)
            return 1
        new_region = sha(match.group(1).encode())

        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = sha(source_path.read_bytes())
        descriptor["regions"]["market-terminal-state-coherence-v1"] = new_region
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = mutant / CHECKER
        checker = checker_path.read_text()
        if checker.count(OLD_REGION) != 1:
            print("stage5g-c-r2ca-r1-negative: FAIL: checker hash anchor drift", file=sys.stderr)
            return 1
        checker_path.write_text(checker.replace(OLD_REGION, new_region, 1))

        local = run([sys.executable, str(checker_path), "--root", str(mutant)], mutant)
        if local.returncode != 0:
            print(
                "stage5g-c-r2ca-r1-negative: FAIL: rehashed local bundle did not self-authorize",
                file=sys.stderr,
            )
            print(local.stderr, file=sys.stderr)
            return 1
        detached = run([sys.executable, str(SNAPSHOT), "--root", str(mutant)], ROOT)
        if detached.returncode == 0:
            print(
                "stage5g-c-r2ca-r1-negative: FAIL: detached snapshot accepted mutation",
                file=sys.stderr,
            )
            return 1

    print("stage5g-c-r2ca-r1-authority-negative-harness: PASS (1/1)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
