#!/usr/bin/env python3
"""Adversarial mutations for Stage 5G-d R1-a R1."""

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
STAGE5G_D = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-d-r1a-r1-transactional-admission.json")
CHECKER = Path("scripts/stage5g_d_r1a_r1_authority_check.py")

CASES = (
    ("future-bar-preflight-removed", STAGE5C, "if explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms {", "if false && explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms {"),
    ("explicit-now-comparison-reversed", STAGE5C, "explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms", "explicit_now_ts_utc_ms > bar_checkpoint_ts_utc_ms"),
    ("instrument-preflight-removed", STAGE5C, "if accepted.bar.instrument != *admission.target_instrument() {", "if false && accepted.bar.instrument != *admission.target_instrument() {"),
    ("tick-preflight-removed", STAGE5C, "if !same_tick_size(accepted.tick_size, admission.tick_size()) {", "if false && !same_tick_size(accepted.tick_size, admission.tick_size()) {"),
    ("preflight-moved-after-delegate", STAGE5C, "let settled = match &settlement.inner {", "let _premature_delegate_marker = \"advance-after-callback\";\n    let settled = match &settlement.inner {"),
    ("blocked-settlement-reconstructed", STAGE5C, "Err(reason) => return Err(stage5cm_block(reason, settlement)),", "Err(reason) => return Err(Stage5cTimerContinuationFailure::Terminal(reason)),"),
    ("callback-invoked-during-preflight", STAGE5C, "let recovery_receipt = &settled.recovery_receipt;", "STAGE5GD_R1A_R1_DELEGATE_COUNT.with(|count| count.set(count.get() + 1));\n    let recovery_receipt = &settled.recovery_receipt;"),
    ("preservation-assertion-removed", STAGE5C, "assert_eq!(stage5gd_r1a_r1_snapshot(&preserved), before);", "let _ = (preserved, before);"),
    ("stage5g-wrapper-changed", STAGE5G_D, "pub fn continue_stage5g_timer_with_bar", "pub fn continue_stage5g_timer_with_bar_r1_changed"),
    ("stage5g-e-opened", DESCRIPTOR, '"stage5g_e": false', '"stage5g_e": true'),
    ("runtime-live-opened", DESCRIPTOR, '"runtime_live": false', '"runtime_live": true'),
)


def sha(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def copy_root(destination: Path) -> None:
    shutil.copytree(ROOT, destination, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "*.zip"))


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(root / CHECKER), "--root", str(root)], cwd=root, text=True, capture_output=True, check=False)


def region_hash(source: str) -> str:
    begin = "// STAGE5G-D-R1A-R1-AUTHORITY-BEGIN: complete-precallback-transactional-admission-v1"
    end = "// STAGE5G-D-R1A-R1-AUTHORITY-END: complete-precallback-transactional-admission-v1"
    match = re.search(rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n", source, re.S)
    if match is None:
        raise RuntimeError("R1 authority region missing")
    return sha(match.group(1).encode())


def main() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-d-r1a-r1-negative-") as raw:
            mutant = Path(raw) / "repo"
            copy_root(mutant)
            path = mutant / relative
            source = path.read_text()
            if source.count(old) < 1:
                raise RuntimeError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(old, new, 1))
            if run_checker(mutant).returncode == 0:
                print(f"FAIL mutation survived: {name}")
                return 1
            print(f"PASS {name}")
            passed += 1

    with tempfile.TemporaryDirectory(prefix="stage5g-d-r1a-r1-rehash-") as raw:
        mutant = Path(raw) / "repo"
        copy_root(mutant)
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        old = "explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms"
        new = "explicit_now_ts_utc_ms <= bar_checkpoint_ts_utc_ms"
        if source.count(old) != 1:
            raise RuntimeError("rehash mutation anchor drift")
        source_path.write_text(source.replace(old, new, 1))
        current_hash = sha(source_path.read_bytes())
        authority_hash = region_hash(source_path.read_text())

        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = current_hash
        descriptor["r1_regions"]["complete-precallback-transactional-admission-v1"] = authority_hash
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = mutant / CHECKER
        checker = checker_path.read_text()
        checker = checker.replace("dc7e0743165bc9995cde5e20531747275faaf6c60a53fc4e2c80a3dbd11d116d", current_hash)
        checker = checker.replace("2288c35e162ce4145133c88f940be790161b786587e4eb3e18f7b105c059e91b", authority_hash)
        checker_path.write_text(checker)
        if run_checker(mutant).returncode == 0:
            print("FAIL rehashed source+descriptor+checker mutation survived")
            return 1
    print("PASS rehash-aware-source-descriptor-checker-mutation")
    passed += 1
    print(f"stage5g-d-r1a-r1-negative-harness: PASS {passed}/{len(CASES) + 1}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
