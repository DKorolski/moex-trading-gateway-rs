#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 durable-composition design R1 checker."""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "4caf07c16ddad021add7cffe6e887165e49e1bf0"
BRANCH = "stage8a4-durable-composition-design"
REVIEW_SHA256 = "0f8de37819ccc005bbc609bc21f029f5783ccdd43c0a634b4c09614f507c2a0a"
DESIGN_R2 = "cc58c10d22db312cd83640f1c1e7fd86861a4594"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-design-authority.json")
CONTRACT = Path("docs/stage-8/stage8a4-durable-composition-design.md")
MATRIX = Path("docs/stage-8/STAGE8A_4_DURABLE_COMPOSITION_DESIGN_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A_4_DURABLE_COMPOSITION_DESIGN_R1_NEGATIVE_INVENTORY_2026-08-15.md")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

ALLOWED_CHANGED_PATHS = {
    "README.md", str(AUTHORITY), str(CONTRACT), str(MATRIX), str(NEGATIVE),
    str(STATUS), str(ROADMAP),
    "scripts/stage8a4_durable_composition_design_check.py",
    "scripts/stage8a4_durable_composition_design_negative_harness.py",
    "scripts/stage8a4_durable_composition_design_proof_map.py",
    "scripts/stage8a4_durable_composition_design_gate.sh",
    "scripts/stage8a4_durable_composition_design_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_design_handoff.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_paths() -> set[str]:
    tracked = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True
    ).splitlines()
    return {value for value in tracked + untracked if value}


def check(
    root: Path = ROOT,
    *,
    git_scope: bool = True,
    changed_paths_override: set[str] | None = None,
) -> None:
    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "8A-4-durable-composition-design-R1", "stage drift")
    require(authority["status"] == "design_r1_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch authority drift")
    require(authority["accepted_reducer_ref"] == BASE, "accepted reducer drift")
    require(authority["accepted_reducer_review_sha256"] == REVIEW_SHA256, "review hash drift")
    require(authority["accepted_design_r2_ref"] == DESIGN_R2, "Design R2 lineage drift")
    require(authority["design_only"] is True, "design-only disabled")
    require(authority["production_rust_changed"] is False, "production Rust enabled")

    result = authority["authoritative_result"]
    for key in ("private", "opaque", "linear", "public_diagnostic_is_side_evidence_only"):
        require(result[key] is True, f"authoritative result weakened: {key}")
    require(result["caller_constructible"] is False, "caller authority enabled")
    require(result["public_diagnostic_is_authority"] is False, "diagnostic promoted to authority")
    require(authority["partial_identity_policy"] == "conservative_conflict_no_merge", "partial identity merge enabled")
    require(authority["exact_lookup_states"] == [
        "NotAttempted", "Succeeded", "DocumentedNotFound",
        "Unavailable", "DecodeFailure", "Stale",
    ], "exact lookup state drift")
    require(authority["documented_not_found_proves_no_match"] is False, "404 proves no-match")
    require(authority["unavailable_proves_no_match"] is False, "unavailable proves no-match")
    require(authority["proven_no_match_available"] is False, "ProvenNoMatch opened")
    require(authority["account_safety_summary"] == [
        "active_orders", "unknown_status_orders", "orphan_orders",
    ], "account safety summary drift")
    required_revalidation = {
        "durable_request_identity", "stage6_request_state", "stage7_command_identity",
        "journal_generation", "current_recovery_seal", "operator_arm_generation",
        "kill_switch_state", "account_safety_summary",
    }
    require(set(authority["apply_time_revalidation"]) == required_revalidation, "revalidation inventory drift")
    require(len(authority["transition_vocabulary"]) == 7, "transition vocabulary drift")
    require(authority["conflict_or_unknown_advances_order_lifecycle"] is False, "hold advances lifecycle")
    require(authority["crash_points"] == [
        "BeforeDurableTransitionAppend",
        "AfterDurableTransitionAppendBeforeDerivedPublication",
    ], "crash model drift")
    require(authority["replay_is_idempotent"] is True, "replay idempotency disabled")
    require(authority["ack_after_durable_transition_only"] is True, "ACK ordering weakened")
    require(authority["readiness_after_durable_transition_only"] is True, "readiness ordering weakened")
    require(authority["conflict_unknown_forces_hold"] is True, "hold disabled")
    require(authority["conflict_unknown_forces_operator_disarm"] is True, "disarm disabled")
    require(authority["result_grants_retry_rearm_or_resend"] is False, "retry/rearm/resend enabled")
    require(all(authority["closed"].values()), "closed surface opened")

    contract = (root / CONTRACT).read_text()
    for marker in (
        "public informational side evidence", "private,", "opaque", "linear authoritative outcome",
        "no public constructor", "conservative policy", "NotAttempted", "DocumentedNotFound",
        "`ProvenNoMatch`", "unknown-status order count", "orphan order count",
        "Apply-time revalidation", "BeforeDurableTransitionAppend",
        "AfterDurableTransitionAppendBeforeDerivedPublication", "Same-request",
        "FINAM POST/DELETE", "Stage 8A-5", "Stage 8B",
    ):
        require(marker in contract, f"contract marker missing: {marker}")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 60, "acceptance matrix count drift")
    require([row["id"] for row in rows] == [f"D{i:03d}" for i in range(1, 61)], "matrix IDs drift")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\.", negative, re.MULTILINE)) == 24, "negative inventory count drift")

    status = (root / STATUS).read_text()
    leading = status.split("## Current accepted boundary", 1)[1].split("\n## ", 1)[0]
    for marker in (BASE, REVIEW_SHA256, "durable-composition Design R1", "acceptance is pending",
                   "implementation", "FINAM POST/DELETE", "runtime-live"):
        require(marker in leading, f"leading status missing: {marker}")
    roadmap = (root / ROADMAP).read_text()
    require("4caf07c" in roadmap and "durable-composition Design R1" in roadmap, "roadmap drift")

    paths = changed_paths() if git_scope else changed_paths_override
    if paths is not None:
        require(paths == ALLOWED_CHANGED_PATHS, f"design changed-path drift: {sorted(paths ^ ALLOWED_CHANGED_PATHS)}")
        require(not any(path.startswith(("crates/", "src/", "tests/", ".github/")) for path in paths), "production/test/workflow path changed")
        require("Cargo.toml" not in paths and "Cargo.lock" not in paths, "Cargo surface changed")
    if git_scope:
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=ROOT, text=True).strip()
        require(branch == BRANCH, "wrong branch")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage8a4-durable-composition-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-design-check: PASS rows=60 negatives=24 design-only=true")


if __name__ == "__main__":
    main()
