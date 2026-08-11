#!/usr/bin/env python3
"""Static, semantic and compatibility checks for Stage 6D."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

BASE = "e10d8fb0f9e095a849b1e56779a0597606d22111"
ACCEPTED_STAGE6B = "f0d5e3912243ba85c6f372722c97e815f254a962"
BRANCH = "stage6-durable-chain"
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5_RESTART = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
STAGE5_TRUTH = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
STAGE5_APPLICATION = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs")
DESCRIPTOR = Path("docs/stage-6/stage6d-integration-descriptor.json")

UNCHANGED_FROM_BASE = (
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
    "fixtures/stage6a/place-request-accepted-v1.json",
    "fixtures/stage6a/cancel-request-accepted-v1.json",
    "fixtures/stage6b/place-one-frame-v1.hex",
    "fixtures/stage6c/replay-fingerprint-v1.txt",
)


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def git_bytes(ref: str, path: str) -> bytes:
    return subprocess.check_output(["git", "show", f"{ref}:{path}"])


def extract_block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening:index + 1]
    raise CheckFailure(f"unterminated block: {needle}")


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1, "descriptor schema drift")
    require(value.get("stage") == "6D", "descriptor stage drift")
    require(value.get("status") == "implementation_candidate", "descriptor status drift")
    require(value.get("accepted_stage6c_ref") == BASE, "Stage 6C ref drift")
    require(value.get("accepted_stage6b_ref") == ACCEPTED_STAGE6B, "Stage 6B ref drift")
    require(value.get("required_branch") == BRANCH, "branch drift")
    require(value.get("boot_modes") == ["first_boot", "restart"], "boot modes drift")
    require(value.get("durable_before_effect") is True, "durable-before-effect disabled")
    require(value.get("restart_scenario_count") == 10, "restart matrix drift")
    require(value.get("focused_test_count") == 32, "focused test count drift")
    require(value.get("negative_case_minimum") == 72, "negative minimum drift")
    require(value.get("soak_format") == "ndjson", "soak format drift")
    require(value.get("stage6a_bytes_unchanged") is True, "Stage 6A compatibility drift")
    require(value.get("stage6b_backend_unchanged") is True, "Stage 6B compatibility drift")
    require(value.get("stage6c_replay_unchanged") is True, "Stage 6C compatibility drift")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "closed surface opened")
    require(value.get("stage6e_open") is False, "Stage 6E opened early")


def validate_core(source: str) -> None:
    required = (
        "pub enum Stage6dBootMode",
        "FirstBoot",
        "Restart",
        "pub struct Stage6dFirstBootAuthorization",
        "pub struct Stage6dOperationalIdentityConfig",
        "struct Stage6dAuthenticatedRestartPackageV1",
        "stage5g_restart_package_sha256",
        "stage6_checkpoint_bytes_sha256",
        "operational_identity_sha256",
        "restart_commitment_hmac_sha256",
        "pub struct Stage6dDurableRuntimeRecovered",
        "existing_journal_framed_bytes.ok_or(Stage6dLiveCoreError::RestartJournalMissing)",
        "journal.validate_checkpoint(&authenticated_checkpoint)?",
        "Stage6ReplayEngineV1::replay(journal.records())?",
        "pub struct Stage6dPaperDispatchReceipt",
        "struct Stage6dAcceptedBrokerTruth",
        "pub fn prepare_stage6d_paper_dispatch(",
        "pub fn execute_stage6d_paper_outcome(",
        "Stage6JournalEventKind::RequestAccepted",
        "Stage6JournalEventKind::DispatchAttemptRecorded",
        "pub fn apply_stage6d_restart_fresh_truth(",
        "stage5g_review_operational_identity_for_stage6d",
        "authorize_stage5g_fresh_truth_operational_identity",
        "validate_stage5g_fresh_broker_truth_package",
        "bind_stage5g_fresh_truth_to_clean_restart",
        "reduce_stage5g_fresh_broker_truth",
        "apply_stage5g_fresh_truth_reduction",
        "Stage5gFreshTruthApplicationResult::Applied",
        "Stage5gFreshTruthApplicationResult::Continued",
        "Stage5gFreshTruthApplicationResult::Blocked",
        "pub fn to_ndjson_line(&self)",
        "runtime_pre_fingerprint_sha256",
        "runtime_post_fingerprint_sha256",
        "journal_frontier_sha256",
        "restart_recovery_marker",
    )
    for token in required:
        require(token in source, f"required core token absent: {token}")

    restart = extract_block(source, "pub fn restart_stage6d_paper(")
    require(restart.index("RestartJournalMissing") < restart.index("decode_and_authenticate_restart_package"), "missing journal checked too late")
    prepare = extract_block(source, "pub fn prepare_stage6d_paper_dispatch(")
    require(prepare.index("append(&accepted)") < prepare.index("append(&dispatch_attempt)"), "durable ordering drift")
    execute = extract_block(source, "pub fn execute_stage6d_paper_outcome(")
    require(execute.index("Stage6dAcceptedBrokerTruth") < execute.index("for record in records"), "typed truth issued after record append")
    require("source_evidence" not in source.split("#[cfg(test)]", 1)[0], "public/caller evidence digest surface introduced")
    require("raw_status" not in source and "status: String" not in source, "raw broker status surface introduced")
    tests = [line for line in source.splitlines() if line.startswith("    fn stage6d_") and "fixture" not in line]
    require(len(tests) == 32, f"focused test count drift: {len(tests)}")
    for scenario in ("d1_", "d2_", "d3_", "d5_", "d6_", "d7_", "d8_", "d9_"):
        require(any(scenario in line for line in tests), f"restart scenario witness absent: {scenario}")
    require("restart_missing_journal" in source, "D10 missing-journal witness absent")
    require("same_length_checkpoint_hash_mismatch" in source, "same-length hash mismatch witness absent")
    require("already_applied_terminal_truth_is_noop" in source, "exact already-applied no-op witness absent")


def validate_stage5_bridges(restart: str, truth: str, application: str) -> None:
    require("stage6d_hmac_sha256" in restart and "stage6d_verify_hmac_sha256" in restart, "Stage 6D HMAC bridge absent")
    require("stage5g_review_operational_identity_for_stage6d" in truth, "reviewed operational identity bridge absent")
    block = extract_block(truth, "pub(crate) fn stage5g_review_operational_identity_for_stage6d(")
    for token in ("projection.account_id", "projection.strategy_id", "projection.config_fingerprint_sha256", "projection.instrument_id"):
        require(token in block, f"operational restart binding absent: {token}")
    require("pub(crate) fn into_restart(self)" in application, "blocked/continued authority recovery absent")


def validate_compatibility(root: Path) -> None:
    for path in UNCHANGED_FROM_BASE:
        require((root / path).read_bytes() == git_bytes(BASE, path), f"accepted bytes changed: {path}")


def check(root: Path) -> None:
    for path in (CORE, LIB, STAGE5_RESTART, STAGE5_TRUTH, STAGE5_APPLICATION, DESCRIPTOR):
        require((root / path).is_file(), f"missing file: {path}")
    core = (root / CORE).read_text()
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_core(core)
    validate_stage5_bridges(
        (root / STAGE5_RESTART).read_text(),
        (root / STAGE5_TRUTH).read_text(),
        (root / STAGE5_APPLICATION).read_text(),
    )
    lib = (root / LIB).read_text()
    for token in ("mod stage6d_live_core;", "apply_stage6d_restart_fresh_truth", "Stage6dDurableRuntimeRecovered", "Stage6dPaperExecutionReport"):
        require(token in lib, f"lib export absent: {token}")
    validate_compatibility(root)


def main() -> None:
    root = Path.cwd().resolve()
    try:
        check(root)
    except (CheckFailure, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage6d-check: FAIL: {error}") from error
    print("stage6d-check: PASS tests=32 restart=D1-D10 stage5g_boundary=accepted compatibility=unchanged")


if __name__ == "__main__":
    main()
