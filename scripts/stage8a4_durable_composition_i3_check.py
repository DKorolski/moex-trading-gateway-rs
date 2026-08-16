#!/usr/bin/env python3
"""Fail-closed semantic checker for Stage 8A-4 durable composition I3."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "90f46052cc31cea012437eddb59fb7c3ca5c2320"
REVIEW_SHA256 = "196c2b69161081f9034eb9399f41245f11ccd7eca229fadc3f8ec842cd1231f0"
BRANCH = "stage8a4-durable-composition-i3"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-i3-authority.json")
CONTRACT = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_IMPLEMENTATION_2026-08-16.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_ACCEPTANCE_MATRIX_2026-08-16.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_NEGATIVE_INVENTORY_2026-08-16.md")
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
JOURNAL = Path("crates/strategy-runtime-core/src/stage6_journal_backend.rs")
REPLAY_V2 = Path("crates/strategy-runtime-core/src/stage6_reconciliation_v2.rs")
RUNTIME = Path("crates/runtime-durable-service/src/recovery.rs")
STAGE8A1 = Path("crates/finam-gateway/src/stage8a1_execution_capability.rs")
I2 = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2.rs")
I3 = Path("crates/finam-gateway/src/stage8a4_reconciliation/durable_composition_i2/durable_writer_i3.rs")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

SCRIPT_FILES = {
    "scripts/stage8a4_durable_composition_i3_check.py",
    "scripts/stage8a4_durable_composition_i3_negative_harness.py",
    "scripts/stage8a4_durable_composition_i3_proof_map.py",
    "scripts/stage8a4_durable_composition_i3_gate.sh",
    "scripts/stage8a4_durable_composition_i3_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_i3_handoff.py",
}
REQUIRED = {
    str(AUTHORITY), str(CONTRACT), str(MATRIX), str(NEGATIVE), str(CORE),
    str(JOURNAL), str(REPLAY_V2), str(RUNTIME), str(STAGE8A1), str(I2), str(I3),
    str(STATUS), str(ROADMAP), *SCRIPT_FILES,
}
ALLOWED_CHANGED = REQUIRED | {
    "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
    "crates/runtime-durable-service/src/lib.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def read(root: Path, path: Path) -> str:
    candidate = root / path
    require(candidate.is_file(), f"missing required file: {path}")
    return candidate.read_text(encoding="utf-8")


def git_output(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check(root: Path = ROOT, git_scope: bool = True) -> None:
    for item in REQUIRED:
        require((root / item).is_file(), f"missing required file: {item}")
    authority = json.loads(read(root, AUTHORITY))
    require(authority["stage"] == "8A-4-durable-composition-I3", "stage drift")
    require(authority["status"] == "implementation_candidate_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch drift")
    require(authority["accepted_i2_r3_ref"] == BASE, "I2 predecessor drift")
    require(authority["accepted_i2_r3_review_sha256"] == REVIEW_SHA256, "I2 review hash drift")
    require(authority["sole_writer_owner"] == "Stage7bRecoveryReadyOwner", "writer owner drift")
    require(authority["pre_append_cas_fields"] == [
        "expected_stage6_checkpoint_or_frontier_fingerprint",
        "expected_recovery_seal_generation",
        "expected_recovery_seal_fingerprint",
        "expected_request_state_fingerprint",
    ], "four-field CAS drift")
    for key in (
        "durable_request_binding_recomputed", "cancel_original_target_shape_from_durable_place_history",
        "v2_first", "each_append_fsync_backed", "covering_seal_required",
        "covering_seal_reread_validated", "historical_arm_registry_revalidated",
        "account_safety_revalidated_immediately_before_writer_entry",
    ):
        require(authority[key] is True, f"mandatory authority disabled: {key}")
    for key in (
        "cas_mismatch_mutates_journal", "durable_receipt_grants_ack_or_readiness",
        "ack_readiness_enabled", "redis_live_enabled", "finam_post_delete_enabled",
        "broker_dispatch_enabled", "runtime_live_enabled", "real_orders_enabled",
        "stage8a5_authorized",
    ):
        require(authority[key] is False, f"closed surface opened: {key}")
    require(authority["same_key_same_payload"] == "idempotent_resume_or_existing", "idempotency drift")
    require(authority["same_key_different_payload"] == "hard_conflict_before_append", "collision drift")
    require(authority["partial_suffix_action"] == "append_only_exact_verified_missing_manifest_suffix", "suffix repair drift")

    core = read(root, CORE)
    journal = read(root, JOURNAL)
    replay = read(root, REPLAY_V2)
    runtime = read(root, RUNTIME)
    stage8a1 = read(root, STAGE8A1)
    i2 = read(root, I2)
    i3 = read(root, I3)
    contract = read(root, CONTRACT)
    negative = read(root, NEGATIVE)
    status = read(root, STATUS)
    roadmap = read(root, ROADMAP)

    for marker in (
        "pub struct Stage6DurableRequestAuthorityV1", "durable_request_binding_sha256",
        "authorize_stage8a4_durable_batch_source", "validate_cancel_original_target_shape",
        "durable_cancel_original_shape", "initial_request_state_fingerprint",
        "pub fn append_stage8a4_durable_batch", "append_versioned",
        "stable_transition_key_sha256", "canonical_v2_record_sha256",
        "verified_suffix_prefix_length", "Stage6ReconciliationBatchCompletionV2::Complete",
        "transition.previous_record_id() != Some(authority.dispatch_record_id())",
        "recovered.journal_frontier().last_record_id()",
    ):
        require(marker in core, f"core writer marker missing: {marker}")
    for marker in (
        "same_stable_key_with_different_v2_payload_is_hard_conflict",
        "rejects_stale_frontier_and_request_state_before_append",
        "repairs_only_missing_suffix_after_v2_crash_boundary",
        "cancel_requires_exact_durable_original_place_shape",
        "appends_v2_then_exact_suffix_and_is_idempotent",
    ):
        require(marker in core, f"core I3 test missing: {marker}")
    require("self.file.sync_data()" in journal, "journal fsync missing")
    require(
        "fn append_versioned(" in journal
        and "Stage6JournalRecordVersioned::decode_canonical" in journal
        and "versioned_records" in journal,
        "versioned V1/V2 journal path missing",
    )
    require("canonical_record_sha256" in replay and "matches_record" in replay, "full suffix manifest verification missing")

    for marker in (
        "pub fn append_stage8a4_durable_batch_and_cover", "revalidate_cached_committed_seal",
        "refresh_stage7b_durable_frontier", "precondition_matches_current_seal",
        "advance_recovery_seal", "validate_recovered_binding",
        "fn stage8a4_i3_uncovered_checkpoint", "Stage6JournalRecordVersioned::V2(transition)",
        "any(|record| matches!(record, Stage6JournalRecordVersioned::V2(_)))",
        "commit_recovery_seal", "read_committed_recovery_seal",
        "Stage7bRecoverySealV1::decode_canonical", "validated != committed_seal",
    ):
        require(marker in runtime, f"runtime/seal marker missing: {marker}")
    for marker in (
        "writer_rejects_stale_seal_cas_before_journal_mutation",
        "restart_covers_v2_only_crash_then_repairs_exact_suffix",
        "restart_covers_partial_suffix_then_appends_only_missing_record",
        "restart_rejects_unrelated_record_after_uncovered_v2",
        "writer_commits_covering_s1_and_restarts_from_mixed_journal",
    ):
        require(marker in runtime, f"runtime I3 test missing: {marker}")

    for marker in (
        "struct Stage8a4HistoricalArmProvenance", "struct Stage8a4PostEffectControlEvidence",
        "stage8a4_historical_arm_provenance", "issue_stage8a4_post_effect_control_evidence",
        "read_arm_registration", "Stage8a4PostEffectControlState::StopRequested",
        "Stage8a4PostEffectControlState::StaleOrUnreadable",
    ):
        require(marker in stage8a1, f"post-effect control marker missing: {marker}")
    require("pub struct Stage8a4HistoricalArmProvenance" not in stage8a1, "historical arm exported")
    require("pub struct Stage8a4PostEffectControlEvidence" not in stage8a1, "post-effect control exported")
    for marker in (
        "mod durable_writer_i3;", "struct Stage8a4I2DurableCandidate",
        "cancel_original_target_shape", "accepted_command_payload_sha256",
    ):
        require(marker in i2, f"private I2/I3 composition marker missing: {marker}")
    for marker in (
        "fn append_private_candidate_and_cover", "account_safety_summary",
        "CurrentAccountSafetyMismatch", "Stage6Stage8a4DurableBatch::new",
        "append_stage8a4_durable_batch_and_cover", "Stage8a4PostEffectControlState::RunAllowed",
        "Stage8a4PostEffectControlState::StopRequested",
        "Stage8a4PostEffectControlState::StaleOrUnreadable",
    ):
        require(marker in i3, f"private I3 marker missing: {marker}")
    require("pub fn append_private_candidate_and_cover" not in i3, "private writer exported")
    for forbidden in (
        "reqwest", "Method::POST", "Method::DELETE", ".post(", ".delete(", "redis::",
        "CommandAck", "XACK", "Readiness", "retry", "resend",
    ):
        require(forbidden not in i3, f"forbidden I3 surface: {forbidden}")

    for marker in (
        "four-field CAS", "V2-first", "covering seal", "V2-only crash",
        "partial suffix crash", "StopRequested", "I4 remains separately review-gated",
        "FINAM POST/DELETE",
    ):
        require(marker in contract, f"contract marker missing: {marker}")
    require("38." in negative and "raw transport" in negative, "negative inventory incomplete")
    for document_name, document in (("status", status), ("roadmap", roadmap)):
        for marker in ("90f4605", "I3", "I4"):
            require(marker in document, f"{document_name} marker missing: {marker}")

    with (root / MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 45, "acceptance row count drift")
    require([row["id"] for row in rows] == [f"I3-{index:03d}" for index in range(1, 46)], "acceptance IDs drift")
    require(all(row["requirement"].strip() and row["evidence"].strip() for row in rows), "acceptance evidence empty")

    if git_scope and (root / ".git").exists():
        require(git_output(root, "merge-base", "--is-ancestor", BASE, "HEAD") == "", "I2 predecessor not ancestor")
        require(git_output(root, "branch", "--show-current") == BRANCH, "wrong branch")
        changed = set(filter(None, git_output(root, "diff", "--name-only", BASE, "--").splitlines()))
        untracked = set(filter(None, git_output(root, "ls-files", "--others", "--exclude-standard").splitlines()))
        candidate = {path for path in changed | untracked if not path.startswith(("reports/", "tmp/", "target/"))}
        require(candidate <= ALLOWED_CHANGED, f"out-of-scope paths: {sorted(candidate - ALLOWED_CHANGED)}")
        require(not any(path.startswith(".github/") or path in {"Cargo.toml", "Cargo.lock"} for path in candidate), "Cargo/workflow drift")
        production_diff = git_output(
            root,
            "diff",
            "--unified=0",
            BASE,
            "--",
            "crates/strategy-runtime-core/src",
            "crates/runtime-durable-service/src",
            "crates/finam-gateway/src",
        )
        added_production = "\n".join(
            line[1:]
            for line in production_diff.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )
        for forbidden in (
            "reqwest",
            "Method::POST",
            "Method::DELETE",
            ".post(",
            ".delete(",
            "redis::",
            "CommandAck",
            "XACK",
        ):
            require(forbidden not in added_production, f"new execution surface in production diff: {forbidden}")


def main() -> None:
    root = ROOT
    git_scope = True
    args = sys.argv[1:]
    if args and args[0] == "--root":
        root = Path(args[1]).resolve()
        args = args[2:]
    if args == ["--no-git"]:
        git_scope = False
    elif args:
        raise SystemExit("usage: stage8a4_durable_composition_i3_check.py [--root PATH] [--no-git]")
    try:
        check(root, git_scope=git_scope)
    except (CheckFailure, KeyError, ValueError, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i3-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-i3-check: PASS rows=45 append=true ack=false execution=false")


if __name__ == "__main__":
    main()
