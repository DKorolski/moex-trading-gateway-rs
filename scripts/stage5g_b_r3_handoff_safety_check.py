#!/usr/bin/env python3
"""Verify a complete, commit- and origin-bound Stage 5G-b R3 handoff."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_b_r2_handoff_safety_check as r2

inherited = r2.inherited
PREDECESSOR = "d03f6e5e88fb853290457d6d6dac08f21c2cf28b"

inherited.STAGE = "5G-b-r3-duplicate-transition-identity"
inherited.BASE_REF = PREDECESSOR
inherited.SOURCE_MANIFEST = "stage5g-b-r3-source-tree-manifest.json"
inherited.EVIDENCE_MANIFEST = "stage5g-b-r3-evidence-manifest.json"
inherited.COMMIT_OBJECT = "stage5g-b-r3-commit-object.txt"
inherited.SAFETY_RESULT = "stage5g-b-r3-archive-safety-result.json"
inherited.SAFETY_STDOUT = "stage5g-b-r3-archive-safety.stdout.txt"
inherited.SAFETY_STDERR = "stage5g-b-r3-archive-safety.stderr.txt"
inherited.EVIDENCE_PREFIX = "stage5g-b-r3-evidence/"
inherited.EXPECTED_COMMANDS = {
    "stage5g-b-r2-snapshot": ["bash", "scripts/stage5g_b_r2_snapshot_gate.sh"],
    "r3-checker": ["python3", "scripts/stage5g_b_r3_check.py"],
    "r3-negative": ["python3", "scripts/stage5g_b_r3_negative_harness.py"],
    "origin-sync": ["bash", "scripts/stage5g_b_r3_origin_sync_gate.sh"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": ["cargo", "test", "-p", "strategy-runtime-core", "stage5g_mock_ack", "--quiet"],
    "focused-release": ["cargo", "test", "-p", "strategy-runtime-core", "--release", "stage5g_mock_ack", "--quiet"],
    "production-integration": ["cargo", "test", "-p", "strategy-runtime-core", "production_public_", "--quiet"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings"],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}
inherited.EXPECTED_CHANGED_PATHS = sorted({
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "docs/current-status.md",
    "docs/stage-5/5g-b-r3-duplicate-transition-identity.md",
    "docs/stage-5/stage5g-b-r3-contract.json",
    "scripts/make_stage5g_b_r3_handoff_archive.py",
    "scripts/stage5g_b_r2_snapshot_gate.sh",
    "scripts/stage5g_b_r3_check.py",
    "scripts/stage5g_b_r3_handoff_safety_check.py",
    "scripts/stage5g_b_r3_negative_harness.py",
    "scripts/stage5g_b_r3_origin_sync_gate.sh",
})
inherited.REQUIRED_SOURCE_FILES = set(inherited.EXPECTED_CHANGED_PATHS) | {
    "scripts/stage5g_a_snapshot_gate.sh",
    "scripts/stage5g_b_r1_snapshot_gate.sh",
    "scripts/stage5g_b_r2_check.py",
    "scripts/stage5g_b_r2_negative_harness.py",
}


def validate_gate_marker(label: str, stdout: str) -> None:
    markers = {
        "stage5g-b-r2-snapshot": "stage5g-b-r2-snapshot-gate: ok",
        "r3-checker": "stage5g-b-r3-check: PASS",
        "r3-negative": "stage5g-b-r3-negative-harness: PASS 6/6",
        "origin-sync": "stage5g-b-r3-origin-sync-gate: PASS",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok",
    }
    if label in markers and markers[label] not in stdout:
        inherited.common.fail(f"gate success marker missing: {label}")
    if label in {"focused-debug", "focused-release"} and "31 passed" not in stdout:
        inherited.common.fail(f"focused 31-test marker missing: {label}")
    if label == "production-integration" and "5 passed" not in stdout:
        inherited.common.fail("production integration 5-test marker missing")


inherited.validate_gate_marker = validate_gate_marker


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--allow-missing-final-safety", action="store_true")
    parser.add_argument("--result-out", type=Path)
    args = parser.parse_args()
    try:
        marker, member_count = inherited.validate_archive(
            args.archive, allow_missing_final_safety=args.allow_missing_final_safety
        )
    except inherited.common.SafetyFailure as error:
        print(f"stage5g-b-r3-handoff-safety: FAIL: {error}", file=sys.stderr)
        return 1
    result: dict[str, Any] = {
        "schema_version": 1,
        "stage": inherited.STAGE,
        "source_ref": marker["source_ref"],
        "archive_name": args.archive.name,
        "preseal_exit_code": 0,
        "member_count_before_final_safety": member_count,
        "verdict": "PASS",
    }
    if args.result_out is not None:
        args.result_out.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    print(
        "stage5g-b-r3-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
