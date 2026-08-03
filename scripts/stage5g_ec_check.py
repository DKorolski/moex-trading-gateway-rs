#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-c."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

BASE = "6995f8dd2ac226eff33b781f575927361fdc2c45"
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
    ):
        require(descriptor[field] is True, f"invariant lost: {field}")
    require(descriptor["focused_test_count"] == 15, "focused test count drift")
    require(descriptor["supported_lifecycle_kinds"] == [
        "timer_ready", "order_position_awaiting", "exact_replay_synchronized",
        "new_package_awaiting",
    ], "lifecycle set drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()),
            "closed surface opened")
    require(f"Base commit: `{BASE}`." in contract, "contract base drift")
    require("Stage 5G-e-c is the current implementation review" in status,
            "status drift")

    required_restart = (
        "pub enum Stage5gCleanRestartSource",
        "TimerReady(Stage5gTimerReadyPaperStrategy)",
        "OrderPositionAwaiting(Stage5gOrderPositionSession)",
        "ExactReplaySynchronized(Stage5gCommittedExactReplaySession)",
        "NewPackageAwaiting(Stage5gCommittedAwaitingOrderPosition)",
        "let bytes = stage5d_export_canonical_restart_bytes_with_stage5g_extension(",
        "let decoded = stage5d_decode_canonical_restart_bytes_requiring_stage5g(bytes)?;",
        "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;",
        "drop(source);",
        "pub struct Stage5gCleanRestartedCapability",
        "crate::validate_stage5g_timer_checkpoint(&projection.checkpoint)",
        "StrategyStateFingerprintMismatch",
    )
    for token in required_restart:
        require(token in restart, f"clean restart authority drift: {token}")
    for forbidden in ("reqwest", "redis::", ".post(", ".delete(", "tokio::spawn"):
        require(forbidden not in restart, f"forbidden e-c surface: {forbidden}")

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
