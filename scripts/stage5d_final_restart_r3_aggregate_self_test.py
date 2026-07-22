#!/usr/bin/env python3
"""Self-test aggregate Stage 5D r3 closure checker semantics."""

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
INVENTORY = Path("docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json")
MANIFEST = Path("docs/stage-5/stage-5d-additive-freeze-manifest.json")
GATE = Path("scripts/stage5d_final_restart_r3_aggregate_closure_r2.py")


@dataclass(frozen=True)
class Case:
    name: str
    expected: str
    mutator: str


CASES = [
    Case(
        "missing_positive_group",
        "Stage 5D aggregate closure gate missing required positive group",
        "missing_positive_group",
    ),
    Case(
        "missing_scenario",
        "Stage 5D aggregate closure scenario inventory count mismatch",
        "missing_scenario",
    ),
    Case(
        "forged_executed_count",
        "Stage 5D aggregate closure gate must derive positive count from inventory",
        "forged_executed_count",
    ),
    Case(
        "omitted_stage5c_continuation",
        "Stage 5D aggregate closure scenario Stage 5C continuation missing",
        "omitted_stage5c_continuation",
    ),
    Case(
        "stage5e_opened",
        "Stage 5D aggregate closure closed-surface mismatch",
        "stage5e_opened",
    ),
]


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def refresh_hash_pins(root: Path) -> None:
    manifest_path = root / MANIFEST
    manifest = json.loads(manifest_path.read_text())
    stage5d = root / "crates/strategy-runtime-core/src/stage5d_persistence.rs"
    if stage5d.exists():
        stage5d_sha = sha256_file(stage5d)
        manifest["stage5d_persistence_file"]["current_sha256"] = stage5d_sha
        for extension in manifest.get("controlled_source_semantic_extensions", []):
            if extension.get("stage5d_consumer_path") == str(stage5d.relative_to(root)):
                extension["stage5d_consumer_sha256"] = stage5d_sha
    manifest_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    checker_path = root / CHECKER
    checker_source = checker_path.read_text()
    if stage5d.exists():
        checker_source = re.sub(
            r'("stage5d_consumer_sha256": ")[0-9a-f]{64}(")',
            rf"\g<1>{sha256_file(stage5d)}\2",
            checker_source,
        )
    checker_path.write_text(checker_source)


def mutate(root: Path, kind: str) -> None:
    if kind == "missing_positive_group":
        path = root / GATE
        source = path.read_text()
        source = source.replace(
            '    ("positive_riskgate_recovery", "stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix"),\n',
            "",
        )
        path.write_text(source)
    elif kind == "missing_scenario":
        path = root / INVENTORY
        inventory = json.loads(path.read_text())
        inventory["scenario_rows"] = inventory["scenario_rows"][:-1]
        path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n")
    elif kind == "forged_executed_count":
        path = root / GATE
        source = path.read_text().replace(
            "mandatory_positive_count=21", "positive_cases_executed=21"
        )
        path.write_text(source)
    elif kind == "omitted_stage5c_continuation":
        path = root / INVENTORY
        inventory = json.loads(path.read_text())
        inventory["scenario_rows"][0]["stage5c_continuation_executed"] = False
        path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n")
    elif kind == "stage5e_opened":
        path = root / MANIFEST
        manifest = json.loads(path.read_text())
        manifest["closed_surfaces"]["runtime_live"] = True
        path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
    else:
        raise RuntimeError(f"unknown self-test mutation: {kind}")


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
    print("stage5d-aggregate-closure-r2-checker-self-test: start")
    with tempfile.TemporaryDirectory(prefix="stage5d-aggregate-self-test-") as tmp:
        base = Path(tmp)
        for case in CASES:
            case_root = base / case.name
            copy_review_baseline(ROOT, case_root)
            mutate(case_root, case.mutator)
            refresh_hash_pins(case_root)
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
    print("stage5d-aggregate-closure-r2-checker-self-test: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
