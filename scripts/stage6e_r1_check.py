#!/usr/bin/env python3
"""Static, semantic, governance and compatibility checks for Stage 6E-R1."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

BASE = "ec71791563a933889eb825f6f8f0846915ba6415"
BRANCH = "stage6-durable-chain"
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5 = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-6/stage6e-r1-closure-descriptor.json")
DOC = Path("docs/stage-6/stage6e-r1-closure.md")
CURRENT = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")

UNCHANGED_STAGE6 = (
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
    "fixtures/stage6a/place-request-accepted-v1.json",
    "fixtures/stage6a/cancel-request-accepted-v1.json",
    "fixtures/stage6b/place-one-frame-v1.hex",
    "fixtures/stage6c/replay-fingerprint-v1.txt",
)

FORBIDDEN_PRODUCTION = (
    "redis::",
    "XREADGROUP",
    "XAUTOCLAIM",
    "reqwest",
    "broker_finam",
    "finam_gateway",
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "Stage6FileJournalBackend",
    "OpenOptions",
    "TcpStream",
    "tokio::spawn",
    "std::thread::spawn",
    "NativeStopOrder",
    "ProtectiveOrderPayload",
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
    expected = {
        "schema_version": 1,
        "stage": "6E-R1",
        "status": "implementation_candidate",
        "superseded_stage6e_ref": BASE,
        "required_branch": BRANCH,
        "source_ref_bound_by_handoff": True,
        "multi_current_request_issuance": "request_scoped",
        "selected_request_must_be_active_cross_bound_member": True,
        "current_process_restore_epoch": True,
        "restore_epoch_loaded_from_restart_package": False,
        "restore_epoch_loaded_from_broker_input": False,
        "trusted_validation_time_source": "internal_host_clock",
        "collection_interval_explicit": True,
        "section_local_observation_times_explicit": True,
        "integration_fingerprint_schema_version": 3,
        "accepted_fresh_truth_schema_version": 2,
        "focused_test_count": 18,
        "negative_case_minimum": 48,
        "stage6a_b_c_compatibility_unchanged": True,
        "stage6_closed_after_independent_acceptance": True,
        "stage7_open_after_independent_acceptance": True,
    }
    for key, expected_value in expected.items():
        require(value.get(key) == expected_value, f"descriptor drift: {key}")
    require(value.get("closed_surfaces") and not any(value["closed_surfaces"].values()), "execution surface opened")


def validate_core(source: str) -> None:
    production = source.split("#[cfg(test)]", 1)[0]
    required = (
        "STAGE6D_INTEGRATION_FINGERPRINT_SCHEMA_VERSION: u16 = 3",
        "STAGE6E_ACCEPTED_FRESH_TRUTH_SCHEMA_VERSION: u16 = 2",
        "moex.stage6e-r1.durable-runtime-recovered.v3",
        "moex.stage6e-r1.current-process-restore-epoch.v1",
        "struct Stage6RestoreEpoch",
        "fn from_current_host_process()",
        "process_generation_id: Stage6Sha256Digest",
        "restore_completed_at: DateTime<Utc>",
        "restore_epoch: Option<Stage6RestoreEpoch>",
        "current_process_restore_epoch_sha256",
        "pub fn issue_stage6e_paper_fresh_broker_truth_for_request(",
        "request_id: StrategyRequestId",
        "fn stage6d_validate_selected_restart_request(",
        "fn validate_stage6e_temporal_authority(",
        "collection_started_at: DateTime<Utc>",
        "validation_observed_at: DateTime<Utc>",
        "FreshTruthRequestNotCrossBound",
        "FreshTruthTemporalAuthorityMismatch",
        "restore_epoch_fingerprint_sha256",
        "Stage6eAcceptedFreshBrokerTruth",
        "Stage6eFreshBrokerTruthProviderBoundary",
        "stage6e_semantic_cross_bind_restart",
        "stage5g_attribution_fingerprint_sha256",
    )
    for token in required:
        require(token in production, f"required production token absent: {token}")
    forbidden = (
        "pub fn issue_stage6e_paper_fresh_broker_truth(",
        "validation_observed_at = input.captured_at",
        "clean_restore_floor",
        "last_broker_truth_received_at\n        .unwrap_or",
    )
    for token in forbidden:
        require(token not in production, f"superseded authority remains: {token}")
    for token in FORBIDDEN_PRODUCTION:
        require(token not in production, f"closed execution surface opened: {token}")

    recover = extract_block(source, "fn recover_stage6d_restart_from_authorities(")
    ordered = (
        "validate_checkpoint",
        "Stage6ReplayEngineV1::replay",
        "stage6e_semantic_cross_bind_restart",
        "Stage6RestoreEpoch::from_current_host_process",
        "integration_fingerprint",
        "Ok(Stage6dDurableRuntimeRecovered",
    )
    positions = [recover.index(token) for token in ordered]
    require(positions == sorted(positions), "restore epoch/recovery ordering drift")

    issuer = extract_block(source, "pub fn issue_stage6e_paper_fresh_broker_truth_for_request(")
    require("Utc::now()" in issuer, "production issuer does not use host validation clock")
    internal = extract_block(source, "fn issue_stage6e_paper_fresh_broker_truth_for_request_at(")
    issuer_order = (
        "stage6d_validate_selected_restart_request",
        "stage6d_validate_replayed_facts_against_truth",
        "active_cross_bound_request_ids",
        "validate_stage6e_temporal_authority",
        "validate_stage5g_fresh_broker_truth_package",
        "Ok(Stage6eAcceptedFreshBrokerTruth",
    )
    positions = [internal.index(token) for token in issuer_order]
    require(positions == sorted(positions), "request/temporal issuance order drift")
    require("clean_restore_completed_at: restore_epoch.restore_completed_at" in internal, "persisted freshness floor reused")

    temporal = extract_block(source, "fn validate_stage6e_temporal_authority(")
    for token in (
        "collection_started_at <= restore_completed_at",
        "captured_at < input.collection_started_at",
        "captured_at > validation_observed_at",
        "orders_observed_at",
        "trades_observed_at",
        "positions_observed_at",
        "row.received_ts <= restore_completed_at",
        "row.received_ts > validation_observed_at",
    ):
        require(token in temporal, f"temporal invariant absent: {token}")

    application = extract_block(source, "pub fn apply_stage6e_accepted_fresh_truth(")
    require("restore_epoch_fingerprint_sha256" in application, "apply does not recheck process epoch")
    require("Stage6ePaperFreshBrokerTruthInput" not in application, "raw input reaches application")

    accepted_start = source.index("pub struct Stage6eAcceptedFreshBrokerTruth")
    accepted = extract_block(source, "pub struct Stage6eAcceptedFreshBrokerTruth")
    require("pub " not in accepted, "accepted capability exposes public field")
    prefix = source[max(0, accepted_start - 180):accepted_start]
    for derive in ("Clone", "Debug", "Serialize", "Deserialize"):
        require(f"derive({derive}" not in prefix, f"accepted capability derives {derive}")

    tests = [line for line in source.splitlines() if line.startswith("    fn stage6e_r1_")]
    require(len(tests) == 18, f"Stage 6E-R1 focused test count drift: {len(tests)}")
    for witness in (
        "two_active_place_requests_are_cross_bound",
        "request_scoped_issuer_selects_each_of_two_current_requests",
        "current_request_with_finalized_history_can_be_issued",
        "finalized_only_request_cannot_be_selected",
        "mixed_current_place_cancel_exact_target_is_cross_bound",
        "mixed_current_place_cancel_target_mismatch_fails_closed",
        "valid_package_is_strictly_after_current_restore",
        "package_before_current_restore_is_rejected",
        "package_equal_to_current_restore_is_rejected",
        "orders_section_before_current_restore_is_rejected",
        "trades_section_before_current_restore_is_rejected",
        "positions_section_before_current_restore_is_rejected",
        "future_package_beyond_trusted_validation_is_rejected",
        "row_received_in_trusted_future_is_rejected",
        "prior_process_capability_is_rejected_after_new_restart",
    ):
        require(f"fn stage6e_r1_{witness}(" in source, f"focused witness absent: {witness}")


def validate_governance(current: str, roadmap: str, onboarding: str) -> None:
    current_top = current.split("### Historical accepted transition record", 1)[0]
    for token in (
        "Stage 6A, 6B, 6C and 6C-R1 are independently accepted",
        "Stage 6D is",
        "Stage 6E-R1 review candidate",
        "Stage 7 is CLOSED",
        "runtime-live",
        "CLOSED",
    ):
        require(token in current_top, f"current-status top drift: {token}")
    require("Stage 6E-R1 — final durable-chain closure repair" in roadmap, "roadmap active stage drift")
    require("Stage 7 remains closed" in roadmap, "roadmap Stage 7 gate absent")
    require("active review target is **Stage 6E-R1**" in onboarding, "review onboarding drift")


def validate_compatibility(root: Path) -> None:
    for path in UNCHANGED_STAGE6:
        require((root / path).read_bytes() == git_bytes(BASE, path), f"accepted Stage 6 bytes changed: {path}")


def check(root: Path) -> None:
    for path in (CORE, LIB, STAGE5, DESCRIPTOR, DOC, CURRENT, ROADMAP, ONBOARDING):
        require((root / path).is_file(), f"missing file: {path}")
    source = (root / CORE).read_text()
    validate_core(source)
    validate_descriptor(json.loads((root / DESCRIPTOR).read_text()))
    validate_governance(
        (root / CURRENT).read_text(),
        (root / ROADMAP).read_text(),
        (root / ONBOARDING).read_text(),
    )
    lib = (root / LIB).read_text()
    require("issue_stage6e_paper_fresh_broker_truth_for_request" in lib, "request issuer export absent")
    require("issue_stage6e_paper_fresh_broker_truth," not in lib, "ambiguous issuer export remains")
    stage5 = (root / STAGE5).read_text()
    require("stage6e_restored_two_place_fixture_with_attributions" in stage5, "two-place source fixture absent")
    require("stage6e_restored_mixed_place_cancel_fixture_with_attributions" in stage5, "mixed source fixture absent")
    validate_compatibility(root)


def main() -> None:
    root = Path.cwd().resolve()
    try:
        check(root)
    except (CheckFailure, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage6e-r1-check: FAIL: {error}") from error
    print("stage6e-r1-check: PASS focused=18 request_scoped=true restore_epoch=current_process governance=current")


if __name__ == "__main__":
    main()
