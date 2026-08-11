#!/usr/bin/env python3
"""Static, semantic and compatibility checks for Stage 6E."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

BASE = "8d4c1f437c02cfb023aa75fb4a411b9394d2d293"
ACCEPTED_STAGE6C_R1 = "e10d8fb0f9e095a849b1e56779a0597606d22111"
BRANCH = "stage6-durable-chain"
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5_ORDER_POSITION = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-6/stage6e-closure-descriptor.json")
DOC = Path("docs/stage-6/stage6e-live-durable-chain-closure.md")

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
    require(value.get("stage") == "6E", "descriptor stage drift")
    require(value.get("status") == "implementation_candidate", "descriptor status drift")
    require(value.get("accepted_stage6d_ref") == BASE, "Stage 6D ref drift")
    require(value.get("accepted_stage6c_r1_ref") == ACCEPTED_STAGE6C_R1, "Stage 6C-R1 ref drift")
    require(value.get("required_branch") == BRANCH, "branch drift")
    require(value.get("restart_cross_binding_before_capability") is True, "boot cross-binding disabled")
    require(len(value.get("cross_bound_fields", [])) == 9, "cross-bound field inventory drift")
    require(value.get("unmatched_effect_capable_stage6_request_rejected") is True, "unmatched authority allowed")
    require(value.get("finalized_historical_stage6_request_allowed") is True, "history policy drift")
    require(value.get("accepted_fresh_truth_capability") == "Stage6eAcceptedFreshBrokerTruth", "accepted capability drift")
    require(value.get("raw_truth_application_allowed") is False, "raw truth application opened")
    require(value.get("future_provider_is_authority") is False, "provider became authority")
    require(value.get("integration_fingerprint_schema_version") == 2, "fingerprint schema drift")
    require(value.get("focused_test_count") == 16, "focused test count drift")
    require(value.get("negative_case_minimum") == 48, "negative minimum drift")
    require(value.get("stage6a_bytes_unchanged") is True, "Stage 6A compatibility drift")
    require(value.get("stage6b_backend_unchanged") is True, "Stage 6B compatibility drift")
    require(value.get("stage6c_replay_unchanged") is True, "Stage 6C compatibility drift")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "closed surface opened")


def validate_core(source: str) -> None:
    required = (
        "STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 2",
        "moex.stage6e.durable-runtime-recovered.v2",
        "moex.stage6e.stage5-stage6-semantic-cross-binding.v1",
        "struct Stage6eSemanticCrossBinding",
        "fn stage6e_semantic_cross_bind_restart(",
        "pub struct Stage6eAcceptedFreshBrokerTruth",
        "pub trait Stage6eFreshBrokerTruthProviderBoundary",
        "pub fn issue_stage6e_paper_fresh_broker_truth(",
        "pub fn apply_stage6e_accepted_fresh_truth(",
        "RestartSemanticCrossBindingMismatch",
        "AcceptedFreshTruthBindingMismatch",
        "stage5g_attribution_fingerprint_sha256",
        "request.final_disposition().is_none()",
        "active_cross_bound_request_identity_sha256",
    )
    for token in required:
        require(token in source, f"required core token absent: {token}")

    recover = extract_block(source, "fn recover_stage6d_restart_from_authorities(")
    require(recover.index("validate_checkpoint") < recover.index("Stage6ReplayEngineV1::replay"), "checkpoint validation order drift")
    require(recover.index("Stage6ReplayEngineV1::replay") < recover.index("stage6e_semantic_cross_bind_restart"), "cross-binding before replay")
    require(recover.index("stage6e_semantic_cross_bind_restart") < recover.index("integration_fingerprint"), "cross-binding after capability fingerprint")
    require(recover.index("integration_fingerprint") < recover.index("Ok(Stage6dDurableRuntimeRecovered"), "recovered capability issued too early")

    cross = extract_block(source, "fn stage6e_semantic_cross_bind_restart(")
    for token in (
        "command_request_id",
        "command_client_order_id",
        "projection.account_id",
        "projection.instrument_id",
        "projection.strategy_id",
        "expected_attribution_fingerprint_sha256",
        "expected_action",
        "expected_cancel_target",
        "target_order_client_order_id",
        "final_disposition().is_none()",
    ):
        require(token in cross, f"cross-binding authority absent: {token}")

    issuer = extract_block(source, "pub fn issue_stage6e_paper_fresh_broker_truth(")
    require(issuer.index("stage6d_validate_replayed_facts_against_truth") < issuer.index("validate_stage5g_fresh_broker_truth_package"), "broker correlation after Stage 5 validation")
    require(issuer.index("validate_stage5g_fresh_broker_truth_package") < issuer.index("Ok(Stage6eAcceptedFreshBrokerTruth"), "accepted truth minted before validation")
    application = extract_block(source, "pub fn apply_stage6e_accepted_fresh_truth(")
    require("Stage6ePaperFreshBrokerTruthInput" not in application, "raw truth reaches application")
    require(application.index("AcceptedFreshTruthBindingMismatch") < application.index("bind_stage5g_fresh_truth_to_clean_restart"), "binding recheck after Stage 5 application")

    accepted_start = source.index("pub struct Stage6eAcceptedFreshBrokerTruth")
    accepted = extract_block(source, "pub struct Stage6eAcceptedFreshBrokerTruth")
    require("pub " not in accepted, "accepted capability exposes public field")
    for derive in ("Clone", "Debug", "Serialize", "Deserialize"):
        require(f"derive({derive}" not in source[max(0, accepted_start - 160):accepted_start], f"accepted capability derives {derive}")

    tests = [line for line in source.splitlines() if line.startswith("    fn stage6e_") and "fixture" not in line and "recovery(" not in line]
    require(len(tests) == 14, f"Stage 6E unit-test count drift: {len(tests)}")
    require(source.count("```compile_fail") >= 2, "Stage 6E compile-fail boundary witnesses absent")


def validate_compatibility(root: Path) -> None:
    for path in UNCHANGED_FROM_BASE:
        require((root / path).read_bytes() == git_bytes(BASE, path), f"accepted bytes changed: {path}")


def check(root: Path) -> None:
    for path in (CORE, LIB, STAGE5_ORDER_POSITION, DESCRIPTOR, DOC):
        require((root / path).is_file(), f"missing file: {path}")
    source = (root / CORE).read_text()
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_core(source)
    stage5 = (root / STAGE5_ORDER_POSITION).read_text()
    require("pub(crate) fn stage5g_attribution_fingerprint_sha256" in stage5, "Stage 5 attribution hash authority absent")
    lib = (root / LIB).read_text()
    for token in (
        "apply_stage6e_accepted_fresh_truth",
        "issue_stage6e_paper_fresh_broker_truth",
        "Stage6eAcceptedFreshBrokerTruth",
        "Stage6eFreshBrokerTruthProviderBoundary",
    ):
        require(token in lib, f"lib export absent: {token}")
    require("apply_stage6d_restart_fresh_truth" not in lib, "raw Stage 6D application remains exported")
    validate_compatibility(root)


def main() -> None:
    root = Path.cwd().resolve()
    try:
        check(root)
    except (CheckFailure, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage6e-check: FAIL: {error}") from error
    print("stage6e-check: PASS focused=16 cross_binding=boot accepted_truth=opaque compatibility=unchanged")


if __name__ == "__main__":
    main()
