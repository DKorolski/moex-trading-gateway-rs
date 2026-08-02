#!/usr/bin/env python3
"""Governance mutations for the R2 deterministic terminal-fill authority."""

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
RUNTIME = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.json")
CHECKER = Path("scripts/stage5g_c_r2ca_r2_authority_check.py")
SNAPSHOT = ROOT / "scripts/stage5g_c_r2ca_r2_snapshot_gate.py"

CASES = (
    ("full-fill-guard-removed", STAGE5C, "FullFillStatusContradiction", "FullFillStatusBypass"),
    ("wall-clock-reintroduced", STAGE5C, "/// Adds the frozen terminal status/fill matrix", "// Utc::now()\n/// Adds the frozen terminal status/fill matrix"),
    ("transaction-candidate-removed", STAGE5C, "stage5g_r2ca_r2_transaction_candidate", "stage5g_r2ca_r2_unreviewed_candidate"),
    ("source-preflight-removed", STAGE5C, "stage5g_r2ca_r2_source_payload", "stage5g_r2ca_r2_request_only_payload"),
    ("timer-sync-removed", RUNTIME, "// STAGE5G-C-R2CA-R2-TIMER-SYNC-BEGIN", "// STAGE5G-C-R2CA-R2-TIMER-SYNC-BYPASS"),
    ("source-witness-removed", STAGE5C, "apply_stage5c_semantic_bar_at", "apply_unreviewed_semantic_bar_at"),
    ("public-authority-opened", STAGE5C, "pub(crate) struct Stage5cValidatedMarketTerminalOutcomeR2", "pub struct Stage5cValidatedMarketTerminalOutcomeR2"),
    ("transport-token-opened", STAGE5C, "/// Runs every callback and escrow check", "// reqwest transport\n/// Runs every callback and escrow check"),
    ("closed-surface-opened", DESCRIPTOR, '"stage5g_d": false', '"stage5g_d": true'),
    ("status-policy-weakened", DESCRIPTOR, '"terminal_status_full_fill": "typed_block_preserving_original_capability"', '"terminal_status_full_fill": "accepted"'),
)


def sha(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "*.zip"),
    )


def simple_mutations() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2-authority-negative-") as raw:
            mutant = Path(raw) / "repo"
            copy_root(mutant)
            path = mutant / relative
            source = path.read_text()
            if source.count(old) < 1:
                raise RuntimeError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(old, new, 1))
            if run_checker(mutant).returncode == 0:
                print(f"FAIL mutation survived: {name}")
                return -1
            print(f"PASS {name}")
            passed += 1
    return passed


def region_hash(source: str) -> str:
    begin = "// STAGE5G-C-R2CA-R2-AUTHORITY-BEGIN: deterministic-terminal-fill-boundary-v1"
    end = "// STAGE5G-C-R2CA-R2-AUTHORITY-END: deterministic-terminal-fill-boundary-v1"
    match = re.search(rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n", source, re.S)
    if match is None:
        raise RuntimeError("R2 authority region missing")
    return sha(match.group(1).encode())


def rehash_aware_mutation() -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2-rehash-negative-") as raw:
        mutant = Path(raw) / "repo"
        copy_root(mutant)
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        old = "return Err(Stage5cMarketTerminalR2Error::CandidateAckGeneratedIntent);"
        new = "return Err(Stage5cMarketTerminalR2Error::CandidateAckFailed);"
        if source.count(old) != 1:
            raise RuntimeError("rehash-aware mutation anchor drift")
        source_path.write_text(source.replace(old, new, 1))
        new_file_hash = sha(source_path.read_bytes())
        new_region_hash = region_hash(source_path.read_text())

        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = new_file_hash
        descriptor["regions"]["deterministic-terminal-fill-boundary-v1"] = new_region_hash
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = mutant / CHECKER
        checker = checker_path.read_text()
        old_file_hash = "541b3dfffc838bd939790210c0a63e988a1c1d4a66f69bba52914a494b4cc3ea"
        old_region_hash = "943f7ac92874f3ccc91f13c5dd020806aee953221219202da24af8affa6d9b72"
        if checker.count(old_file_hash) != 1 or checker.count(old_region_hash) != 1:
            raise RuntimeError("checker rehash anchors drift")
        checker = checker.replace(old_file_hash, new_file_hash, 1)
        checker = checker.replace(old_region_hash, new_region_hash, 1)
        checker_path.write_text(checker)

        if run_checker(mutant).returncode != 0:
            print("FAIL rehashed local bundle did not self-authorize")
            return False
        detached = subprocess.run(
            [sys.executable, str(SNAPSHOT), "--root", str(mutant)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        if detached.returncode == 0:
            print("FAIL detached snapshot accepted rehashed mutation")
            return False
    print("PASS rehash-aware-source-descriptor-checker-mutation")
    return True


def main() -> int:
    passed = simple_mutations()
    if passed < 0 or not rehash_aware_mutation():
        return 1
    passed += 1
    print(f"stage5g-c-r2ca-r2-authority-negative-harness: PASS {passed}/{len(CASES) + 1}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
