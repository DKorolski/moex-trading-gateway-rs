#!/usr/bin/env python3
"""Verify the self-attesting Stage 5G-c R2-c-b R2 archive."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_c_r2ca_r1_handoff_safety_check as base

STAGE = "5G-c-R2-c-b-R2-trade-ledger-watermark-coherence"
BRANCH = "stage5g-lifecycle"
BASE_REF = "8e3a794ea47ff24c9daf3d88b2ef7c3f588e0f01"
SOURCE_MANIFEST = "stage5g-c-r2cb-r2-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-c-r2cb-r2-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-c-r2cb-r2-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-c-r2cb-r2-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-c-r2cb-r2-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-c-r2cb-r2-archive-safety.stderr.txt"
EVIDENCE_PREFIX = "stage5g-c-r2cb-r2-evidence/"

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "authority-check": ["python3", "scripts/stage5g_c_r2cb_r2_authority_check.py"],
    "semantic-negative": ["python3", "scripts/stage5g_c_r2cb_r2_negative_harness.py"],
    "r3-predecessor": ["python3", "scripts/stage5g_c_r2ca_r3_predecessor_gate.py"],
    "r3-authority": ["python3", "scripts/stage5g_c_r2ca_r3_authority_check.py"],
    "r3-snapshot": ["python3", "scripts/stage5g_c_r2ca_r3_snapshot_gate.py"],
    "r3-authority-negative": ["python3", "scripts/stage5g_c_r2ca_r3_authority_negative_harness.py"],
    "r3-semantic-negative": ["python3", "scripts/stage5g_c_r2ca_r3_semantic_negative_harness.py"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": ["cargo", "test", "-p", "strategy-runtime-core", "stage5g_order_position", "--quiet"],
    "focused-release": ["cargo", "test", "-p", "strategy-runtime-core", "--release", "stage5g_order_position", "--quiet"],
    "finam-debug": ["cargo", "test", "-p", "broker-finam", "stage5g_r2cb_finam_full_snapshot_fixture", "--quiet"],
    "finam-release": ["cargo", "test", "-p", "broker-finam", "--release", "stage5g_r2cb_finam_full_snapshot_fixture", "--quiet"],
    "stage5c-api-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings"],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}

EXPECTED_CHANGED_PATHS = sorted({
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "docs/adr/adr-stage5g-c-r2cb-r2-trade-ledger-watermark-coherence.md",
    "docs/stage-5/stage5g-c-r2cb-r2-trade-ledger-watermark-coherence.json",
    "scripts/make_stage5g_c_r2cb_r2_handoff_archive.py",
    "scripts/stage5g_c_r2cb_r2_authority_check.py",
    "scripts/stage5g_c_r2cb_r2_gate.sh",
    "scripts/stage5g_c_r2cb_r2_handoff_safety_check.py",
    "scripts/stage5g_c_r2cb_r2_negative_harness.py",
})
REQUIRED_SOURCE_FILES = set(EXPECTED_CHANGED_PATHS) | {
    "crates/broker-finam/src/mapper.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json",
    "fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json",
    "scripts/stage5g_c_r2ca_r3_authority_check.py",
    "scripts/stage5g_c_r2ca_r3_snapshot_gate.py",
    "scripts/stage5c_api_freeze_check.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
}
CLOSED_SURFACES = {
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
    for name in (
        "STAGE", "BRANCH", "BASE_REF", "SOURCE_MANIFEST", "EVIDENCE_MANIFEST",
        "COMMIT_OBJECT", "COMMIT_MARKER", "SAFETY_RESULT", "SAFETY_STDOUT",
        "SAFETY_STDERR", "EVIDENCE_PREFIX", "EXPECTED_COMMANDS",
        "EXPECTED_CHANGED_PATHS", "REQUIRED_SOURCE_FILES", "CLOSED_SURFACES",
    ):
        setattr(base, name, globals()[name])


def validate_gate_marker(label: str, stdout: str) -> None:
    markers = {
        "authority-check": "stage5g-c-r2cb-r2-authority-check: PASS",
        "semantic-negative": "stage5g-c-r2cb-r2-negative-harness: PASS 12/12",
        "r3-predecessor": "stage5g-c-r2ca-r3-predecessor-gate: PASS",
        "r3-authority": "stage5g-c-r2ca-r3-authority-check: PASS",
        "r3-snapshot": "stage5g-c-r2ca-r3-snapshot-gate: PASS",
        "r3-authority-negative": "stage5g-c-r2ca-r3-authority-negative-harness: PASS 12/12",
        "r3-semantic-negative": "stage5g-c-r2ca-r3-semantic-negative-harness: PASS 6/6",
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
        marker, member_count = base.validate_archive(args.archive, args.allow_missing_final_safety)
    except base.common.SafetyFailure as error:
        print(f"stage5g-c-r2cb-r2-handoff-safety: FAIL: {error}", file=sys.stderr)
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
        f"stage5g-c-r2cb-r2-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
