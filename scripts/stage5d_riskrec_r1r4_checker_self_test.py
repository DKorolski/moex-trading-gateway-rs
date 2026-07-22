#!/usr/bin/env python3
"""Self-test the Stage 5D riskgate-recovery r1-r4 scoped checker."""
from __future__ import annotations

import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

sys.dont_write_bytecode = True

from copy_review_baseline import copy_review_baseline

ROOT = Path(__file__).resolve().parents[1]
CHECKER = Path("scripts/stage5d_additive_freeze_check.py")
STAGE5D = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")


@dataclass(frozen=True)
class SelfTestCase:
    name: str
    old: str
    new: str
    expected: str


CASES = [
    SelfTestCase(
        "recovery_transition_removed",
        "let step = stage5d_apply_next_riskgate_recovery_action(ready)",
        "let step = stage5d_riskrec_r1r4_noop_action(ready)",
        "Stage 5D r1-r4 recovery call graph missing: recovery-step call",
    ),
    SelfTestCase(
        "checkpoint_persistence_removed",
        "stage5d_riskrec_store_commit_and_reload(&mut store, step.ready, step.checkpoint)",
        "stage5d_riskrec_r1r4_skip_commit_and_reload(&mut store, step.ready, step.checkpoint)",
        "Stage 5D r1-r4 recovery call graph missing: checkpoint commit reload",
    ),
    SelfTestCase(
        "stage5c_warmup_removed",
        "crate::stage5c_paper_host::stage5d_test_warmup_stage5c_history_at(",
        "crate::stage5c_paper_host::stage5d_test_warmup_stage5c_history_bypassed(",
        "Stage 5D r1-r4 recovery call graph missing: Stage 5C warmup",
    ),
    SelfTestCase(
        "receipt_envelope_comparison_removed",
        "receipt.envelope_sha256 != envelope_sha256",
        "receipt.envelope_sha256 != receipt.envelope_sha256",
        "Stage 5D r1-r4 receipt validation missing: envelope sha comparison",
    ),
    SelfTestCase(
        "receipt_checkpoint_action_comparison_removed",
        "receipt.checkpoint_action != checkpoint_action",
        "receipt.checkpoint_action != receipt.checkpoint_action",
        "Stage 5D r1-r4 receipt validation missing: checkpoint-action comparison",
    ),
]


def replace_once(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"pattern not found for self-test: {old}")
    path.write_text(source.replace(old, new, 1))


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def refresh_stage5d_hash_pins(root: Path) -> None:
    stage5d_sha = sha256_file(root / STAGE5D)
    manifest_path = root / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["stage5d_persistence_file"]["current_sha256"] = stage5d_sha
    for extension in manifest.get("controlled_source_semantic_extensions", []):
        if extension.get("stage5d_consumer_path") == str(STAGE5D):
            extension["stage5d_consumer_sha256"] = stage5d_sha
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    checker_path = root / CHECKER
    checker_source = checker_path.read_text()
    checker_source = re.sub(
        r'("stage5d_consumer_sha256": ")[0-9a-f]{64}(")',
        rf"\g<1>{stage5d_sha}\2",
        checker_source,
    )
    checker_path.write_text(checker_source)


def run_checker(root: Path) -> tuple[int, str]:
    process = subprocess.Popen(
        [sys.executable, str(root / CHECKER), "--root", str(root)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=20)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
        return 124, stdout + stderr + "\nchecker timed out\n"
    return process.returncode, stdout + stderr


def main() -> int:
    print("stage5d-riskrec-r1r4-checker-self-test: start")
    with tempfile.TemporaryDirectory(prefix="stage5d-riskrec-r1r4-self-test-") as tmp:
        base = Path(tmp)
        clean = base / "clean"
        copy_review_baseline(ROOT, clean)
        for case in CASES:
            case_root = base / case.name
            copy_review_baseline(ROOT, case_root)
            if case.name == "stage5c_warmup_removed":
                source_path = case_root / STAGE5D
                source_path.write_text(source_path.read_text().replace(case.old, case.new))
            else:
                replace_once(case_root / STAGE5D, case.old, case.new)
            refresh_stage5d_hash_pins(case_root)
            returncode, diagnostics = run_checker(case_root)
            if returncode == 0:
                print(f"FAIL {case.name}: checker accepted mutation")
                print(diagnostics)
                return 1
            if case.expected not in diagnostics:
                print(f"FAIL {case.name}: expected diagnostic not found: {case.expected}")
                print(diagnostics)
                return 1
            print(f"PASS {case.name}: {case.expected}")
    print("stage5d-riskrec-r1r4-checker-self-test: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
