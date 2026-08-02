#!/usr/bin/env python3
"""Verify the self-attesting Stage 5G-d R1-a R1 handoff archive."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_c_r2ca_r1_handoff_safety_check as base

STAGE = "5G-d-R1-a-R1-complete-precallback-transactional-admission"
BRANCH = "stage5g-lifecycle"
BASE_REF = "0f72478123c8ddf90c5368ce0cef7867257087c3"
SOURCE_MANIFEST = "stage5g-d-r1a-r1-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-d-r1a-r1-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-d-r1a-r1-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-d-r1a-r1-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-d-r1a-r1-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-d-r1a-r1-archive-safety.stderr.txt"
EVIDENCE_PREFIX = "stage5g-d-r1a-r1-evidence/"

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "r1-authority-check": ["python3", "scripts/stage5g_d_r1a_r1_authority_check.py"],
    "r1-negative": ["python3", "scripts/stage5g_d_r1a_r1_negative_harness.py"],
    "predecessor-authority": ["python3", "scripts/stage5g_d_r1a_r1_predecessor_gate.py"],
    "stage5c-api-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": ["cargo", "test", "-p", "strategy-runtime-core", "stage5gd_r1a", "--quiet"],
    "focused-release": ["cargo", "test", "-p", "strategy-runtime-core", "--release", "stage5gd_r1a", "--quiet"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--quiet", "--", "-D", "warnings"],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
    "diff-check": ["git", "diff", "--check", BASE_REF],
}
EXPECTED_CHANGED_PATHS = sorted({
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "docs/stage-5/stage5g-d-r1a-r1-transactional-admission.json",
    "docs/stage-5/stage5g-d-r1a-r1-transactional-admission.md",
    "scripts/make_stage5g_d_r1a_r1_handoff_archive.py",
    "scripts/stage5g_d_r1a_r1_authority_check.py",
    "scripts/stage5g_d_r1a_r1_gate.sh",
    "scripts/stage5g_d_r1a_r1_handoff_safety_check.py",
    "scripts/stage5g_d_r1a_r1_negative_harness.py",
    "scripts/stage5g_d_r1a_r1_predecessor_gate.py",
})
REQUIRED_SOURCE_FILES = set(EXPECTED_CHANGED_PATHS) | {
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs",
    "crates/strategy-runtime-core/src/stage5d_persistence.rs",
    "crates/broker-core/src/lib.rs",
    "docs/stage-5/stage5g-d-r1a-deterministic-bar-authority.json",
    "scripts/stage5g_d_r1a_authority_check.py",
    "scripts/stage5c_api_freeze_check.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
}
CLOSED_SURFACES = {
    "stage5g_d_r1b", "stage5g_e", "stage5g_f", "redis_live_consumer",
    "finam_transport", "http_post_delete", "broker_execution", "runtime_live",
    "real_orders", "stage6",
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
        "r1-authority-check": "stage5g-d-r1a-r1-authority-check: PASS",
        "r1-negative": "stage5g-d-r1a-r1-negative-harness: PASS 12/12",
        "predecessor-authority": "stage5g-d-r1a-r1-predecessor-gate: PASS",
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
        print(f"stage5g-d-r1a-r1-handoff-safety: FAIL: {error}", file=sys.stderr)
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
    print(f"stage5g-d-r1a-r1-handoff-safety: PASS source_ref={marker['source_ref']} members={member_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
