#!/usr/bin/env python3
"""Adversarial mutations for Stage 5G-d R1-a authority."""

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
DESCRIPTOR = Path("docs/stage-5/stage5g-d-r1a-deterministic-bar-authority.json")
CHECKER = Path("scripts/stage5g_d_r1a_authority_check.py")

CASES = (
    ("guard-moved-after-callback", STAGE5C, "if bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms {", "if false && bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms {"),
    ("wall-clock-reintroduced", STAGE5C, "Utc.timestamp_millis_opt(explicit_now_ts_utc_ms).single()", "Some(Utc::now())"),
    ("caller-controlled-bar-checkpoint", STAGE5C, "accepted\n        .bar\n        .close_time_utc\n        .checked_mul(1_000)", "Some(0_i64)"),
    ("saturating-multiplication", STAGE5C, ".checked_mul(1_000)", ".checked_add(0)"),
    ("settlement-not-returned", STAGE5C, "return Err(stage5cm_block(reason, settlement))", "panic!(\"settlement discarded: {reason:?}\")"),
    ("callback-duplicated", STAGE5C, "advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, explicit_now)", "{ let _ = &explicit_now; advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, explicit_now) }"),
    ("wrapper-changed", STAGE5G_D, "pub fn continue_stage5g_timer_with_bar", "pub fn continue_stage5g_timer_with_bar_r1a_changed"),
    ("stage5g-e-opened", DESCRIPTOR, '"stage5g_e": false', '"stage5g_e": true'),
    ("generated-intent-test-removed", STAGE5C, "stage5gd_r1a_generated_bar_intents_remain_in_stage5c_settled_batch", "stage5gd_r1a_generated_bar_intents_removed"),
)


def sha(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def copy_root(destination: Path) -> None:
    shutil.copytree(ROOT, destination, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "*.log", "*.zip"))


def run_checker(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(root / CHECKER), "--root", str(root)], cwd=root, text=True, capture_output=True, check=False)


def region_hash(source: str) -> str:
    begin = "// STAGE5G-D-R1A-AUTHORITY-BEGIN: deterministic-bar-continuation-authority-v1"
    end = "// STAGE5G-D-R1A-AUTHORITY-END: deterministic-bar-continuation-authority-v1"
    match = re.search(rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n", source, re.S)
    if match is None:
        raise RuntimeError("authority region missing")
    return sha(match.group(1).encode())


def main() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-d-r1a-negative-") as raw:
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

    with tempfile.TemporaryDirectory(prefix="stage5g-d-r1a-rehash-") as raw:
        mutant = Path(raw) / "repo"
        copy_root(mutant)
        source_path = mutant / STAGE5C
        source = source_path.read_text()
        old = "bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms"
        new = "bar_checkpoint_ts_utc_ms < previous_continuation_checkpoint_ts_utc_ms"
        if source.count(old) != 1:
            raise RuntimeError("rehash mutation anchor drift")
        source_path.write_text(source.replace(old, new, 1))
        current_hash = sha(source_path.read_bytes())
        authority_hash = region_hash(source_path.read_text())

        descriptor_path = mutant / DESCRIPTOR
        descriptor = json.loads(descriptor_path.read_text())
        descriptor["stage5c_current_sha256"] = current_hash
        descriptor["authority_regions"]["deterministic-bar-continuation-authority-v1"] = authority_hash
        descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")

        checker_path = mutant / CHECKER
        checker = checker_path.read_text()
        checker = checker.replace("6b38e1c145593ef3ea376b1e1ee50832fb10ba79a25f05ca9370f06344f974f5", current_hash)
        checker = checker.replace("d3547534d0767ea91f3e314897d907cb389a4b09a738e9ee53edb7ffe5b22e5d", authority_hash)
        checker_path.write_text(checker)
        if run_checker(mutant).returncode == 0:
            print("FAIL rehashed source+descriptor+checker mutation survived")
            return 1
    print("PASS rehash-aware-source-descriptor-checker-mutation")
    passed += 1
    print(f"stage5g-d-r1a-negative-harness: PASS {passed}/{len(CASES) + 1}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
