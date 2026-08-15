#!/usr/bin/env python3
"""Fail-closed Stage 8A-4 durable-composition Design R2 checker."""

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
RECONCILIATION_DESIGN = "cc58c10d22db312cd83640f1c1e7fd86861a4594"
REJECTED_R1 = "80fe35ef67e335540e0984781f63a99af794bfe1"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-design-authority.json")
CONTRACT = Path("docs/stage-8/stage8a4-durable-composition-design.md")
MATRIX = Path("docs/stage-8/STAGE8A_4_DURABLE_COMPOSITION_DESIGN_R2_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A_4_DURABLE_COMPOSITION_DESIGN_R2_NEGATIVE_INVENTORY_2026-08-15.md")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

TRANSITIONS = [
    "ExactWorking", "ExactTerminalFilled", "ExactTerminalRejected",
    "ExactTerminalCancelled", "ExactTerminalExpired",
    "ReconciliationConflictHold", "ReconciliationStillUnknownHold",
]
LOOKUP_DISPOSITION = {
    "NotAttempted": "use_other_admitted_sources",
    "Succeeded": "exact_observation_participates_in_reducer",
    "DocumentedNotFound": "conflict_if_exact_contradiction_else_still_unknown_hold",
    "Unavailable": "still_unknown_hold",
    "DecodeFailure": "still_unknown_hold",
    "Stale": "still_unknown_hold",
}
TRANSITION_KEY_FIELDS = [
    "durable_request_binding",
    "private_authoritative_reconciliation_outcome_binding",
    "transition_kind",
]
PRECONDITION_FIELDS = [
    "expected_stage6_checkpoint_or_frontier_fingerprint",
    "expected_recovery_seal_generation",
    "expected_recovery_seal_fingerprint",
    "expected_request_state_fingerprint",
]
CRASH_BOUNDARIES = [
    "BeforeDurableTransitionAppend",
    "AfterTransitionAppendBeforeCoveringSeal",
    "AfterCoveringSealBeforeDerivedPublication",
]

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
    require(authority["schema_version"] == 2, "schema drift")
    require(authority["stage"] == "8A-4-durable-composition-design-R2", "stage drift")
    require(authority["status"] == "design_r2_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch authority drift")
    require(authority["accepted_reducer_ref"] == BASE, "accepted reducer drift")
    require(authority["accepted_reducer_review_sha256"] == REVIEW_SHA256, "review hash drift")
    require(authority["accepted_reconciliation_design_ref"] == RECONCILIATION_DESIGN, "reconciliation design drift")
    require(authority["rejected_durable_design_r1_ref"] == REJECTED_R1, "rejected R1 lineage drift")
    require(authority["design_only"] is True, "design-only disabled")
    require(authority["production_rust_changed"] is False, "production Rust enabled")

    result = authority["authoritative_result"]
    for key in ("private", "opaque", "linear", "public_diagnostic_is_side_evidence_only"):
        require(result[key] is True, f"authoritative result weakened: {key}")
    require(result["caller_constructible"] is False, "caller authority enabled")
    require(result["public_diagnostic_is_authority"] is False, "diagnostic promoted to authority")
    require(authority["partial_identity_policy"] == "conservative_conflict_no_merge", "partial identity merge enabled")
    require(authority["exact_lookup_states"] == list(LOOKUP_DISPOSITION), "exact lookup state drift")
    require(authority["exact_lookup_disposition"] == LOOKUP_DISPOSITION, "exact lookup disposition drift")
    require(authority["attempted_non_success_can_be_downgraded_to_not_attempted"] is False, "attempted failure downgraded")
    require(authority["documented_not_found_proves_no_match"] is False, "404 proves no-match")
    require(authority["unavailable_proves_no_match"] is False, "unavailable proves no-match")
    require(authority["proven_no_match_available"] is False, "ProvenNoMatch opened")
    require(authority["account_safety_summary"] == [
        "active_orders", "unknown_status_orders", "orphan_orders",
    ], "account safety summary drift")

    identity = authority["transition_identity"]
    require(identity["stable_across_append_and_restart"] is True, "unstable transition identity")
    require(identity["fields"] == TRANSITION_KEY_FIELDS, "transition key field drift")
    require(identity["includes_random_nonce"] is False, "random transition nonce enabled")
    require(identity["includes_mutable_post_append_generation"] is False, "mutable generation in transition key")
    precondition = authority["pre_append_compare_and_append"]
    require(precondition["fields"] == PRECONDITION_FIELDS, "pre-append precondition drift")
    require(precondition["mismatch_action"] == "consume_outcome_and_rerun_fresh_reconciliation_without_append", "CAS mismatch action drift")
    require(precondition["same_key_same_payload"] == "idempotent_existing_transition", "idempotent duplicate drift")
    require(precondition["same_key_different_payload"] == "hard_conflict", "duplicate conflict drift")

    require(authority["apply_time_revalidation"] == [
        "durable_request_identity_and_state",
        "stage6_checkpoint_or_frontier_fingerprint",
        "stage7_command_identity_and_payload",
        "recovery_seal_generation_and_fingerprint",
        "historical_operator_arm_provenance_and_scope",
        "kill_switch_for_readiness_and_send_gate",
        "account_safety_summary",
    ], "revalidation inventory drift")
    arm = authority["operator_arm_post_effect"]
    require(arm == {
        "expired_operator_arm_blocks_reconciliation_append": False,
        "replay_recreates_operator_arm": False,
        "operator_arm_can_authorize_resend": False,
        "historical_provenance_and_scope_required": True,
        "identity_substitution_or_conflicting_rearm_allowed": False,
    }, "operator-arm post-effect drift")
    kill = authority["kill_switch_post_effect"]
    require(kill == {
        "stop_requested_blocks_new_send": True,
        "stop_requested_blocks_reconciliation_append": False,
        "stop_requested_forces_disarm_and_readiness_hold": True,
        "stale_or_unreadable_blocks_readiness_and_new_send": True,
        "stale_or_unreadable_converts_truth_to_no_match": False,
        "kill_switch_can_authorize_reconciliation_send": False,
    }, "kill-switch post-effect drift")

    require(authority["transition_vocabulary"] == TRANSITIONS, "transition vocabulary drift")
    require(authority["conflict_or_unknown_advances_order_lifecycle"] is False, "hold advances lifecycle")
    require(authority["transition_durable_before_any_derived_publication"] is True, "publication before transition")
    seal = authority["post_append_recovery_seal"]
    require(seal == {
        "required": True,
        "must_cover_transition_frontier": True,
        "reread_validation_required": True,
        "ack_after_covering_seal_only": True,
        "readiness_after_covering_seal_only": True,
        "settlement_after_covering_seal_only": True,
        "seal_failure_action": "transition_durable_publication_blocked_no_second_append",
    }, "post-append recovery-seal drift")
    require(authority["crash_boundaries"] == CRASH_BOUNDARIES, "crash model drift")
    require(authority["crash_recovery"] == {
        "BeforeDurableTransitionAppend": "reacquire_truth_and_rerun_reducer_no_private_outcome_reuse",
        "AfterTransitionAppendBeforeCoveringSeal": "verify_existing_transition_no_second_append_construct_commit_validate_covering_seal",
        "AfterCoveringSealBeforeDerivedPublication": "resume_publication_idempotently_without_append_or_send",
    }, "crash recovery drift")
    require(authority["publication_eligibility"] == {
        "ExactWorking": "canonical_nonterminal_disposition_after_transition_and_covering_seal",
        "ExactTerminalFilled": "canonical_terminal_disposition_after_transition_and_covering_seal",
        "ExactTerminalRejected": "canonical_terminal_disposition_after_transition_and_covering_seal",
        "ExactTerminalCancelled": "canonical_terminal_disposition_after_transition_and_covering_seal",
        "ExactTerminalExpired": "canonical_terminal_disposition_after_transition_and_covering_seal",
        "ReconciliationConflictHold": "no_terminal_ack_no_xack_no_terminal_settlement_readiness_degraded_disarmed",
        "ReconciliationStillUnknownHold": "no_terminal_ack_no_xack_no_terminal_settlement_readiness_degraded_pending",
    }, "publication eligibility drift")
    for key in (
        "conflict_hold_terminal_ack_allowed", "still_unknown_hold_terminal_ack_allowed",
        "conflict_hold_xack_allowed", "still_unknown_hold_xack_allowed",
        "exact_outcome_alone_implies_account_ready",
    ):
        require(authority[key] is False, f"publication/readiness prohibition weakened: {key}")
    require(authority["replay_is_idempotent"] is True, "replay idempotency disabled")
    require(authority["result_grants_retry_rearm_or_resend"] is False, "retry/rearm/resend enabled")
    require(all(authority["closed"].values()), "closed surface opened")

    contract = (root / CONTRACT).read_text()
    for marker in (
        "durable-composition design R2", "public informational side evidence",
        "private, opaque", "linear authoritative outcome", "no public constructor",
        "DocumentedNotFound", "attempted non-success state cannot be downgraded",
        "stable across append and restart", "expected_recovery_seal_fingerprint",
        "Expiry after possible send does not block", "does not block durable reconciliation append",
        "AfterTransitionAppendBeforeCoveringSeal", "AfterCoveringSealBeforeDerivedPublication",
        "never produce terminal command ACK", "covering recovery seal",
        "FINAM POST/DELETE", "Stage 8A-5", "Stage 8B",
    ):
        require(marker in contract, f"contract marker missing: {marker}")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 76, "acceptance matrix count drift")
    require([row["id"] for row in rows] == [f"D{i:03d}" for i in range(1, 77)], "matrix IDs drift")
    negative = (root / NEGATIVE).read_text()
    require(len(re.findall(r"^\d+\.", negative, re.MULTILINE)) == 38, "negative inventory count drift")

    status = (root / STATUS).read_text()
    leading = status.split("## Current accepted boundary", 1)[1].split("\n## ", 1)[0]
    for marker in (BASE, REVIEW_SHA256, REJECTED_R1, "Durable-composition Design R2",
                   "acceptance is pending", "implementation", "FINAM POST/DELETE", "runtime-live"):
        require(marker in leading, f"leading status missing: {marker}")
    roadmap = (root / ROADMAP).read_text()
    require("4caf07c" in roadmap and "durable-composition Design R2" in roadmap, "roadmap drift")

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
    print("stage8a4-durable-composition-design-check: PASS rows=76 negatives=38 design-only=true")


if __name__ == "__main__":
    main()
