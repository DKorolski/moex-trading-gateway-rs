#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-c."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

BASE = "4296f0621249875f7a2f8cccaa2fbe069cb4bccf"
FILES = {
    "restart": Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs"),
    "stage5d": Path("crates/strategy-runtime-core/src/stage5d_persistence.rs"),
    "order": Path("crates/strategy-runtime-core/src/stage5g_order_position.rs"),
    "lib": Path("crates/strategy-runtime-core/src/lib.rs"),
    "contract": Path("docs/stage-5/stage5g-e-c-clean-process-reconstruction.md"),
    "descriptor": Path("docs/stage-5/stage5g-e-c-clean-process-reconstruction.json"),
    "status": Path("docs/current-status.md"),
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def validate(root: Path, *, check_git: bool = True) -> None:
    for path in FILES.values():
        require((root / path).is_file(), f"missing e-c file: {path}")
    restart = (root / FILES["restart"]).read_text()
    stage5d = (root / FILES["stage5d"]).read_text()
    order = (root / FILES["order"]).read_text()
    lib = (root / FILES["lib"]).read_text()
    contract = (root / FILES["contract"]).read_text()
    status = (root / FILES["status"]).read_text()
    descriptor = json.loads((root / FILES["descriptor"]).read_text())

    require(descriptor["stage"] == "5G-e-c", "descriptor stage drift")
    require(descriptor["accepted_predecessor"] == BASE, "predecessor drift")
    for field in (
        "stage5d_is_single_persistence_authority",
        "source_capability_consumed_before_return",
        "strict_byte_boundary",
        "fresh_runtime_required",
        "semantic_private_riskgate_state_applied",
        "source_owned_cross_binding",
        "projection_validated_before_runtime_mutation",
        "callback_authority_is_type_derived",
        "rehash_aware_semantic_negatives",
    ):
        require(descriptor[field] is True, f"invariant lost: {field}")
    require(descriptor["focused_test_count"] == 27, "focused test count drift")
    require(descriptor["public_clean_process_roundtrips"] == 4,
            "public roundtrip count drift")
    require(descriptor["supported_source_lifecycles"] == [
        "timer_ready", "order_position_awaiting", "exact_replay_synchronized",
        "new_package_awaiting",
    ], "source lifecycle set drift")
    require(descriptor["supported_lifecycle_kinds"] == [
        "timer_ready", "order_position_awaiting_committed",
    ], "lifecycle set drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()),
            "closed surface opened")
    require(f"Base commit: `{BASE}`." in contract, "contract base drift")
    require("Stage 5G-e-c R1 is the current implementation review" in status,
            "status drift")

    export_input = restart.split("pub struct Stage5gCleanRestartExportInput", 1)[1].split("}", 1)[0]
    for caller_owned_identity in ("strategy_id", "account_id", "instrument_id"):
        require(caller_owned_identity not in export_input,
                f"caller regained identity authority: {caller_owned_identity}")

    required_restart = (
        "pub enum Stage5gCleanRestartSource",
        "TimerReady(Stage5gTimerReadyPaperStrategy)",
        "OrderPositionAwaiting(Stage5gOrderPositionSession)",
        "ExactReplaySynchronized(Stage5gCommittedExactReplaySession)",
        "NewPackageAwaiting(Stage5gCommittedAwaitingOrderPosition)",
        "pub(crate) struct Stage5gCleanRestartBindingV1",
        "pub(crate) struct Stage5gCleanRestartLifecycleProofV1",
        "Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted",
        "let (strategy_id, account_id, instrument_id) = source_binding(source);",
        "let bytes = stage5d_export_canonical_restart_bytes_with_stage5g_extension(",
        "let decoded = stage5d_decode_canonical_restart_bytes_requiring_stage5g(bytes)?;",
        "validate_projection(&projection)?;",
        "validate_projection_binding(&projection, &decoded.envelope, &fresh_runtime)?;",
        "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;",
        "drop(source);",
        "pub struct Stage5gCleanRestartedCapability",
        "crate::validate_stage5g_timer_checkpoint(&projection.checkpoint)",
        "StrategyStateFingerprintMismatch",
        "CallbackAuthorityMismatch",
        "ZeroIntentProofMismatch",
        "if !projection.lifecycle_proof.zero_intent_ready {",
        "&projection.checkpoint,\n                projection.lifecycle_proof.authoritative_callback_count,",
        "lifecycle_authority_sha256",
        "next_reconciliation_observation",
    )
    for token in required_restart:
        require(token in restart, f"clean restart authority drift: {token}")
    for forbidden in ("reqwest", "redis::", ".post(", ".delete(", "tokio::spawn"):
        require(forbidden not in restart, f"forbidden e-c surface: {forbidden}")
    projection_at = restart.index("let projection: Stage5gCleanRestartProjectionV1")
    standalone_at = restart.index("validate_projection(&projection)?;", projection_at)
    binding_at = restart.index(
        "validate_projection_binding(&projection, &decoded.envelope, &fresh_runtime)?;",
        standalone_at,
    )
    mutation_at = restart.index(
        "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;",
        binding_at,
    )
    require(projection_at < standalone_at < binding_at < mutation_at,
            "projection/binding must validate before runtime mutation")

    required_stage5d = (
        "stage5g_extension_json: Option<String>",
        "stage5g_extension_sha256: Option<String>",
        "fn validate_stage5g_extension_pair(&self)",
        ".ok_or(Stage5dEnvelopeValidationError::RequiredFieldEmpty)?",
        "stage5d_apply_validated_materialized_riskgate_for_restart",
    )
    for token in required_stage5d:
        require(token in stage5d, f"Stage 5D authority drift: {token}")

    tests = (
        "stage5ge_c_timer_ready_zero_intent_projects_through_canonical_boundary",
        "stage5ge_c_awaiting_order_position_preserves_slots",
        "stage5ge_c_exact_replay_synchronized_projection_roundtrips",
        "stage5ge_c_new_package_awaiting_projection_roundtrips",
        "stage5ge_c_historical_replay_ledger_and_counters_survive_bytes",
        "stage5ge_c_exact_decimal_representation_survives_byte_roundtrip",
        "stage5ge_c_callback_count_and_state_fingerprint_remain_exact",
        "stage5ge_c_missing_replay_projection_fails_closed",
        "stage5ge_c_regressive_continuation_checkpoint_fails_closed",
        "stage5ge_c_unsupported_lifecycle_kind_fails_closed",
        "stage5ge_c_missing_order_position_state_fails_closed",
        "stage5ge_c_conflicting_slot_projection_fails_closed",
        "stage5ge_c_r1_public_timer_ready_clean_process_roundtrip",
        "stage5ge_c_r1_public_awaiting_clean_process_roundtrip",
        "stage5ge_c_r1_public_exact_source_clean_process_roundtrip",
        "stage5ge_c_r1_public_new_package_source_clean_process_roundtrip",
        "stage5ge_c_r1_rehashed_stage5d_account_cross_binding_fails_closed",
        "stage5ge_c_r1_rehashed_stage5d_instrument_cross_binding_fails_closed",
        "stage5ge_c_r1_rehashed_extension_binding_strategy_fails_closed",
        "stage5ge_c_r1_rehashed_timer_summary_fails_closed",
        "stage5ge_c_r1_rehashed_timer_checkpoint_graft_fails_closed",
        "stage5ge_c_r1_rehashed_callback_self_authority_fails_closed",
        "stage5ge_c_r1_rehashed_lifecycle_kind_swap_fails_closed",
        "stage5ge_c_r1_rehashed_order_position_graft_fails_closed",
    )
    require(all(name in order for name in tests), "focused projection witness missing")
    for name in (
        "stage5ge_c_stage5d_package_bytes_restore_fresh_runtime_without_source_capability",
        "stage5ge_c_stage5d_package_without_projection_fails_closed",
        "stage5ge_c_stage5g_extension_checksum_tamper_fails_closed",
    ):
        require(name in stage5d, f"Stage 5D byte-boundary witness missing: {name}")
    require("moved_source_cannot_be_reused" in lib and "let _copy = restored.clone();" in lib
            and "compile_fail,E0382" in lib and "compile_fail,E0599" in lib,
            "compile-fail witness missing")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "base missing")
        head = git(root, "rev-parse", "HEAD")
        if head != BASE:
            require(git(root, "rev-parse", "HEAD^") == BASE,
                    "e-c must be exactly one successor")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root, check_git=not args.skip_git)
    except (CheckFailure, OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-ec-check: FAIL: {error}")
        return 1
    print("stage5g-ec-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
