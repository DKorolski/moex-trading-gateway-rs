#!/usr/bin/env python3
"""Verify a commit- and origin-bound Stage 5G-c review handoff."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_b_r3_handoff_safety_check as r3

inherited = r3.inherited
PREDECESSOR = "dba5362444ec279391eed92ff28ebb4ceb729c09"

inherited.STAGE = "5G-c-r1-order-trade-position-convergence"
inherited.BASE_REF = PREDECESSOR
inherited.SOURCE_MANIFEST = "stage5g-c-source-tree-manifest.json"
inherited.EVIDENCE_MANIFEST = "stage5g-c-evidence-manifest.json"
inherited.COMMIT_OBJECT = "stage5g-c-commit-object.txt"
inherited.SAFETY_RESULT = "stage5g-c-archive-safety-result.json"
inherited.SAFETY_STDOUT = "stage5g-c-archive-safety.stdout.txt"
inherited.SAFETY_STDERR = "stage5g-c-archive-safety.stderr.txt"
inherited.EVIDENCE_PREFIX = "stage5g-c-evidence/"
inherited.EXPECTED_COMMANDS = {
    "stage5g-c-predecessor-snapshot": ["bash", "scripts/stage5g_c_predecessor_snapshot_gate.sh"],
    "c-checker": ["python3", "scripts/stage5g_c_check.py"],
    "c-negative": ["python3", "scripts/stage5g_c_negative_harness.py"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": [
        "cargo", "test", "-p", "strategy-runtime-core", "stage5g_order_position", "--quiet",
    ],
    "focused-release": [
        "cargo", "test", "-p", "strategy-runtime-core", "--release",
        "stage5g_order_position", "--quiet",
    ],
    "production-integration": [
        "cargo", "test", "-p", "strategy-runtime-core",
        "stage5gc_r1_public", "--quiet",
    ],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": [
        "cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--quiet",
        "--", "-D", "warnings",
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}
inherited.EXPECTED_CHANGED_PATHS = sorted({
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "docs/current-status.md",
    "docs/stage-5/5g-c-order-trade-position-convergence.md",
    "docs/stage-5/stage5g-c-contract.json",
    "scripts/make_stage5g_c_handoff_archive.py",
    "scripts/make_stage5g_b_r1_handoff_archive.py",
    "scripts/stage5g_c_predecessor_snapshot_gate.sh",
    "scripts/stage5g_c_check.py",
    "scripts/stage5g_c_gate.sh",
    "scripts/stage5g_c_handoff_safety_check.py",
    "scripts/stage5g_c_negative_harness.py",
})
inherited.REQUIRED_SOURCE_FILES = set(inherited.EXPECTED_CHANGED_PATHS) | {
    "scripts/stage5g_b_r3_check.py",
    "scripts/stage5g_b_r3_negative_harness.py",
    "scripts/stage5g_b_r2_snapshot_gate.sh",
}

_validate_inherited_evidence = inherited.validate_evidence


def validate_evidence(files: dict[str, bytes], marker: dict[str, str]) -> None:
    normalized = dict(files)
    manifest = json.loads(normalized[inherited.EVIDENCE_MANIFEST])
    closed = manifest.get("closed_surfaces")
    expected = {
        "stage5g_d", "redis_live_consumer", "redis_consumer_groups",
        "finam_transport", "http_post_delete", "broker_dispatch_execution",
        "runtime_live", "real_orders", "protective_completion", "stage6",
        "main_merge", "deployment",
    }
    if not isinstance(closed, dict) or set(closed) != expected or any(
        value is not False for value in closed.values()
    ):
        inherited.common.fail("Stage 5G-c R1 closed-surface inventory drift")
    manifest["closed_surfaces"] = {
        "redis_live_consumer": False,
        "redis_consumer_groups": False,
        "finam_transport": False,
        "http_post_delete": False,
        "broker_dispatch_execution": False,
        "order_trade_position_events": False,
        "runtime_live": False,
        "real_orders": False,
        "stage5g_c": False,
        "stage6": False,
    }
    normalized[inherited.EVIDENCE_MANIFEST] = (
        json.dumps(manifest, sort_keys=True).encode("utf-8")
    )
    _validate_inherited_evidence(normalized, marker)


inherited.validate_evidence = validate_evidence


def validate_gate_marker(label: str, stdout: str) -> None:
    markers = {
        "stage5g-c-predecessor-snapshot": "stage5g-c-predecessor-snapshot-gate: ok",
        "c-checker": "stage5g-c-r1-check: PASS",
        "c-negative": "stage5g-c-r1-negative-harness: PASS 16/16",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok",
    }
    if label in markers and markers[label] not in stdout:
        inherited.common.fail(f"gate success marker missing: {label}")
    if label in {"focused-debug", "focused-release"} and "23 passed" not in stdout:
        inherited.common.fail(f"focused 23-test marker missing: {label}")
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
        print(f"stage5g-c-handoff-safety: FAIL: {error}", file=sys.stderr)
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
        "stage5g-c-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
