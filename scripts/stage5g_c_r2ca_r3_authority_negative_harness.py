#!/usr/bin/env python3
"""Governance mutations for the R3 exact receipt-clock authority."""

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
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2ca-r3-exact-receipt-clock-bracket-authority.json")
CHECKER = Path("scripts/stage5g_c_r2ca_r3_authority_check.py")
SNAPSHOT = ROOT / "scripts/stage5g_c_r2ca_r3_snapshot_gate.py"

CASES = (
    ("receipt-ms-truncated", STAGE5C, "evidence.truth.received_ts.timestamp_millis()", "evidence.truth.received_ts.timestamp() * 1_000"),
    ("before-timer-check-removed", STAGE5C, "evidence_received_ms < started", "false"),
    ("ack-receipt-check-removed", STAGE5C, "evidence_received_ms < ack_processed_ms", "false"),
    ("grace-clock-rebound", STAGE5C, "stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_received_ms)", "stage5g_r2ca_r2_bracket_reconcile_active_at(facts.lifecycle_event_ts_utc * 1_000)"),
    ("r2-settlement-delegation-removed", STAGE5C, "settle_stage5c_validated_market_terminal_outcome_r2(validated.validated_r2)", "panic!(\"R2 settlement bypass\")"),
    ("same-second-witness-removed", STAGE5C, "r3_same_second_post_start_receipt_uses_inside_grace_policy", "r3_same_second_witness_removed"),
    ("delayed-receipt-witness-removed", STAGE5C, "r3_delayed_receipt_after_grace_escrows_recovery_immediately", "r3_delayed_receipt_witness_removed"),
    ("fresh-retry-witness-removed", STAGE5C, "r3_fresh_snapshot_same_source_later_receipt_unblocks_retry", "r3_fresh_retry_witness_removed"),
    ("clock-domain-weakened", DESCRIPTOR, '"terminal_decision": "broker_truth_package_receipt_clock_milliseconds"', '"terminal_decision": "component_source_seconds"'),
    ("live-surface-opened", DESCRIPTOR, '"runtime_live": false', '"runtime_live": true'),
    ("transport-token-opened", STAGE5C, "/// Captures the exact package receipt timestamp", "// reqwest transport\n/// Captures the exact package receipt timestamp"),
)


def sha(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "*.zip"),
    )


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )


def simple_mutations() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r3-authority-negative-") as raw:
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
    begin = "// STAGE5G-C-R2CA-R3-AUTHORITY-BEGIN: exact-receipt-clock-bracket-authority-v1"
    end = "// STAGE5G-C-R2CA-R3-AUTHORITY-END: exact-receipt-clock-bracket-authority-v1"
    match = re.search(rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n", source, re.S)
    if match is None:
        raise RuntimeError("R3 authority region missing")
    return sha(match.group(1).encode())


def rehash_aware_mutation() -> bool:
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r3-rehash-negative-") as raw:
        mutant = Path(raw) / "repo"
        copy_root(mutant)
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        old = "if is_partial_exit && bracket_started_ms.is_some_and(|started| evidence_received_ms < started) {\n        return Err(stage5c_r2_block(\n            Stage5cMarketTerminalR2Error::EvidenceBeforeBracketTimer,"
        new = "if is_partial_exit && bracket_started_ms.is_some_and(|started| evidence_received_ms < started) {\n        return Err(stage5c_r2_block(\n            Stage5cMarketTerminalR2Error::SourceStateInconsistent,"
        if source.count(old) != 1:
            raise RuntimeError("rehash-aware mutation anchor drift")
        source_path.write_text(source.replace(old, new, 1))
        new_file_hash = sha(source_path.read_bytes())
        new_region_hash = region_hash(source_path.read_text())

        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = new_file_hash
        descriptor["regions"]["exact-receipt-clock-bracket-authority-v1"] = new_region_hash
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = mutant / CHECKER
        checker = checker_path.read_text()
        old_file_hash = "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7"
        old_region_hash = "2d1d530690bfc821c908ce092fec294c3b6a5243cb80cd6ad400e1c3aa57e12e"
        if checker.count(old_file_hash) != 1 or checker.count(old_region_hash) != 1:
            raise RuntimeError("checker rehash anchors drift")
        checker = checker.replace(old_file_hash, new_file_hash, 1)
        checker = checker.replace(old_region_hash, new_region_hash, 1)
        checker_path.write_text(checker)
        if run_checker(mutant).returncode != 0:
            print("FAIL rehashed local R3 bundle did not self-authorize")
            return False
        detached = subprocess.run(
            [sys.executable, str(SNAPSHOT), "--root", str(mutant)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        if detached.returncode == 0:
            print("FAIL detached R3 snapshot accepted rehashed mutation")
            return False
    print("PASS rehash-aware-source-descriptor-checker-mutation")
    return True


def main() -> int:
    passed = simple_mutations()
    if passed < 0 or not rehash_aware_mutation():
        return 1
    passed += 1
    print(f"stage5g-c-r2ca-r3-authority-negative-harness: PASS {passed}/{len(CASES) + 1}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
