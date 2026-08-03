#!/usr/bin/env python3
"""Verify the self-attesting Stage 5G-e-b R2 handoff archive."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_c_r2ca_r1_handoff_safety_check as base

STAGE = "5G-e-b-R2"
BRANCH = "stage5g-lifecycle"
BASE_REF = "1621307a6012fa1f9dcbc89a59651c801f6cc26f"
SOURCE_MANIFEST = "stage5g-eb-r2-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-eb-r2-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-eb-r2-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-eb-r2-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-eb-r2-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-eb-r2-archive-safety.stderr.txt"
EVIDENCE_PREFIX = "stage5g-eb-r2-evidence/"
EXPECTED_COMMANDS: dict[str, list[str]] = {
    "stage5g-eb-r2-check": ["python3", "scripts/stage5g_eb_r2_check.py"],
    "stage5g-eb-r2-negative": ["python3", "scripts/stage5g_eb_r2_negative_harness.py"],
    "detached-predecessor": ["python3", "scripts/stage5g_eb_r2_predecessor_gate.py"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug-r2": ["cargo", "test", "-p", "strategy-runtime-core", "stage5ge_b_r2", "--quiet"],
    "inherited-debug-eb": ["cargo", "test", "-p", "strategy-runtime-core", "stage5ge_b_", "--quiet"],
    "focused-release-r2": ["cargo", "test", "-p", "strategy-runtime-core", "--release", "stage5ge_b_r2", "--quiet"],
    "inherited-release-eb": ["cargo", "test", "-p", "strategy-runtime-core", "--release", "stage5ge_b_", "--quiet"],
    "timer-doctests": ["cargo", "test", "-p", "strategy-runtime-core", "--doc", "stage5g_timer", "--quiet"],
    "stage5c-api-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings"],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}
EXPECTED_CHANGED_PATHS = sorted({
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "docs/current-status.md",
    "docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.json",
    "docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.md",
    "scripts/make_stage5g_eb_r2_handoff_archive.py",
    "scripts/stage5g_eb_r2_check.py",
    "scripts/stage5g_eb_r2_gate.sh",
    "scripts/stage5g_eb_r2_handoff_safety_check.py",
    "scripts/stage5g_eb_r2_negative_harness.py",
    "scripts/stage5g_eb_r2_predecessor_gate.py",
})
REQUIRED_SOURCE_FILES = set(EXPECTED_CHANGED_PATHS) | {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5d_persistence.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "docs/stage-5/5g-lifecycle-design-and-implementation-plan.md",
    "docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.json",
    "docs/stage-5/stage5g-e-b-owned-candidate-application.json",
    "scripts/stage5c_api_freeze_check.py",
    "scripts/stage5g_eb_r1_check.py",
    "scripts/stage5g_eb_r1_negative_harness.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
}
CLOSED_SURFACES = {
    "stage5g_e_clean_process_restart", "grst01_grst12_full_matrix", "stage5g_f",
    "redis_live_consumer_groups", "finam_transport", "http_post_delete",
    "broker_dispatch_execution", "runtime_live", "real_orders", "stage6",
    "main_merge", "deployment",
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
        "stage5g-eb-r2-check": "stage5g-eb-r2-check: PASS",
        "stage5g-eb-r2-negative": "stage5g-eb-r2-negative-harness: PASS 13/13",
        "detached-predecessor": "stage5g-eb-r2-predecessor-gate: PASS",
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
        print(f"stage5g-eb-r2-handoff-safety: FAIL: {error}", file=sys.stderr)
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
        "stage5g-eb-r2-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
