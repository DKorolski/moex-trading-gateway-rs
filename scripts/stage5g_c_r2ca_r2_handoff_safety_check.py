#!/usr/bin/env python3
"""Verify the self-attesting Stage 5G-c R2-c-a R2 handoff archive."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_c_r2ca_r1_handoff_safety_check as base

STAGE = "5G-c-R2-c-a-R2-deterministic-terminal-fill-boundary"
BRANCH = "stage5g-lifecycle"
BASE_REF = "d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654"
SOURCE_MANIFEST = "stage5g-c-r2ca-r2-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-c-r2ca-r2-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-c-r2ca-r2-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-c-r2ca-r2-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-c-r2ca-r2-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-c-r2ca-r2-archive-safety.stderr.txt"
EVIDENCE_PREFIX = "stage5g-c-r2ca-r2-evidence/"

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "predecessor-gate": ["python3", "scripts/stage5g_c_r2ca_r2_predecessor_gate.py"],
    "authority-check": ["python3", "scripts/stage5g_c_r2ca_r2_authority_check.py"],
    "snapshot-gate": ["python3", "scripts/stage5g_c_r2ca_r2_snapshot_gate.py"],
    "authority-negative": [
        "python3",
        "scripts/stage5g_c_r2ca_r2_authority_negative_harness.py",
    ],
    "semantic-negative": [
        "python3",
        "scripts/stage5g_c_r2ca_r2_semantic_negative_harness.py",
    ],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5g_r2ca_r2_tests",
        "--quiet",
    ],
    "focused-release": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "--release",
        "stage5g_r2ca_r2_tests",
        "--quiet",
    ],
    "source-ack-mapping": [
        "cargo",
        "test",
        "-p",
        "broker-core",
        "trading_window_closed_ack_preserves_confirmed_and_deferred_semantics",
        "--quiet",
    ],
    "stage5g-b-source-path": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "production_public_submitted_then_recovered_resolves_stage5c_once",
        "--quiet",
    ],
    "stage5c-api-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": [
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--quiet",
        "--",
        "-D",
        "warnings",
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}

EXPECTED_CHANGED_PATHS = sorted(
    {
        "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
        "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "docs/adr/adr-stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.md",
        "docs/current-status.md",
        "docs/stage-5/stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.json",
        "scripts/make_stage5g_c_r2ca_r2_handoff_archive.py",
        "scripts/stage5g_c_r2ca_r2_authority_check.py",
        "scripts/stage5g_c_r2ca_r2_authority_gate.sh",
        "scripts/stage5g_c_r2ca_r2_authority_negative_harness.py",
        "scripts/stage5g_c_r2ca_r2_handoff_safety_check.py",
        "scripts/stage5g_c_r2ca_r2_predecessor_gate.py",
        "scripts/stage5g_c_r2ca_r2_semantic_negative_harness.py",
        "scripts/stage5g_c_r2ca_r2_snapshot_gate.py",
    }
)
REQUIRED_SOURCE_FILES = set(EXPECTED_CHANGED_PATHS) | {
    "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/broker-core/src/hybrid_strategy_boundary.rs",
    "scripts/stage5g_c_r2ca_r1_authority_check.py",
    "scripts/stage5g_c_r2ca_r1_snapshot_gate.py",
    "scripts/stage5c_api_freeze_check.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
}
CLOSED_SURFACES = {
    "stage5g_c_r2cb",
    "stage5g_d",
    "redis_live_consumer_groups",
    "finam_transport",
    "http_post_delete",
    "broker_dispatch_execution",
    "runtime_live",
    "real_orders",
    "stage6",
    "main_merge",
    "deployment",
}


def configure_base() -> None:
    base.STAGE = STAGE
    base.BRANCH = BRANCH
    base.BASE_REF = BASE_REF
    base.SOURCE_MANIFEST = SOURCE_MANIFEST
    base.EVIDENCE_MANIFEST = EVIDENCE_MANIFEST
    base.COMMIT_OBJECT = COMMIT_OBJECT
    base.COMMIT_MARKER = COMMIT_MARKER
    base.SAFETY_RESULT = SAFETY_RESULT
    base.SAFETY_STDOUT = SAFETY_STDOUT
    base.SAFETY_STDERR = SAFETY_STDERR
    base.EVIDENCE_PREFIX = EVIDENCE_PREFIX
    base.EXPECTED_COMMANDS = EXPECTED_COMMANDS
    base.EXPECTED_CHANGED_PATHS = EXPECTED_CHANGED_PATHS
    base.REQUIRED_SOURCE_FILES = REQUIRED_SOURCE_FILES
    base.CLOSED_SURFACES = CLOSED_SURFACES


def validate_gate_marker(label: str, stdout: str) -> None:
    markers = {
        "predecessor-gate": "stage5g-c-r2ca-r2-predecessor-gate: PASS",
        "authority-check": "stage5g-c-r2ca-r2-authority-check: PASS",
        "snapshot-gate": "stage5g-c-r2ca-r2-snapshot-gate: PASS",
        "authority-negative": "stage5g-c-r2ca-r2-authority-negative-harness: PASS 11/11",
        "semantic-negative": "stage5g-c-r2ca-r2-semantic-negative-harness: PASS 10/10",
        "stage5c-api-freeze": "stage5c-api-freeze-check: ok",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok",
    }
    if label in markers and markers[label] not in stdout:
        base.common.fail(f"gate success marker missing: {label}")


def configure_validation_callback() -> None:
    configure_base()
    base.validate_gate_marker = validate_gate_marker


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--allow-missing-final-safety", action="store_true")
    parser.add_argument("--result-out", type=Path)
    args = parser.parse_args()
    configure_validation_callback()
    try:
        marker, member_count = base.validate_archive(
            args.archive, args.allow_missing_final_safety
        )
    except base.common.SafetyFailure as error:
        print(f"stage5g-c-r2ca-r2-handoff-safety: FAIL: {error}", file=sys.stderr)
        return 1
    result: dict[str, Any] = {
        "schema_version": 1,
        "stage": STAGE,
        "source_ref": marker["source_ref"],
        "archive_name": args.archive.name,
        "preseal_exit_code": 0,
        "member_count_before_final_safety": member_count,
        "verdict": "PASS",
    }
    if args.result_out is not None:
        args.result_out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        "stage5g-c-r2ca-r2-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
