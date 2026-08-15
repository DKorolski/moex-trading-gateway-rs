#!/usr/bin/env python3
"""Fail-closed checker for Stage 8A-4 durable composition implementation spec R1."""

from __future__ import annotations

import csv
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "6ddf54ef9d7f740dc59cd2450e78301be3d068cb"
BRANCH = "stage8a4-durable-composition-implementation-spec"
REVIEW_SHA256 = "160b674d661982b6dbaa6248c2c4acaf883543cb8be99318ef04b0787492f4ba"
REDUCER = "4caf07c16ddad021add7cffe6e887165e49e1bf0"
RECONCILIATION_DESIGN = "cc58c10d22db312cd83640f1c1e7fd86861a4594"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-implementation-spec-authority.json")
TZ = Path("docs/stage-8/TZ_STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_2026-08-15.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_NEGATIVE_INVENTORY_2026-08-15.md")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

V2_FIELDS = [
    "stable_transition_key_sha256", "durable_request_binding_sha256",
    "private_authoritative_outcome_binding_sha256", "endpoint_kind",
    "transition_kind", "exact_lookup_evidence", "broker_order_fact",
    "material_trade_facts", "fill_effect", "account_safety_binding",
    "pre_append_precondition_evidence", "deterministic_suffix_manifest",
]
QUERY_FIELDS = [
    "state", "account_id", "queried_broker_order_id",
    "durable_request_binding_sha256", "request_started_at",
    "response_received_at", "response_status_category",
]
STABLE_KEY_FIELDS = [
    "durable_request_binding",
    "private_authoritative_reconciliation_outcome_binding",
    "transition_kind",
]
CAS_FIELDS = [
    "expected_stage6_checkpoint_or_frontier_fingerprint",
    "expected_recovery_seal_generation", "expected_recovery_seal_fingerprint",
    "expected_request_state_fingerprint",
]
PLACE_MATRIX = {
    "ExactWorking": "broker_order_and_trade_facts_then_request_finalized_completed_ack_recovered",
    "ExactTerminalFilled": "broker_order_and_trade_facts_then_request_finalized_completed_ack_recovered",
    "ExactTerminalRejected": "broker_order_fact_then_request_finalized_rejected_ack_rejected",
    "ExactTerminalCancelled": "broker_order_and_trade_facts_then_request_finalized_completed_ack_recovered",
    "ExactTerminalExpired": "broker_order_and_trade_facts_then_request_finalized_completed_ack_recovered",
    "ReconciliationConflictHold": "transition_only_no_finalization_no_ack_no_xack",
    "ReconciliationStillUnknownHold": "transition_only_no_finalization_no_ack_no_xack",
}
CANCEL_MATRIX = {
    "ExactWorking": "transition_only_unresolved_no_finalization_no_ack_no_xack",
    "ExactTerminalFilled": "cancel_execution_observed_then_request_finalized_completed_ack_recovered",
    "ExactTerminalRejected": "cancel_already_terminal_non_execution_then_request_finalized_completed_ack_recovered",
    "ExactTerminalCancelled": "cancel_canceled_then_request_finalized_completed_ack_recovered",
    "ExactTerminalExpired": "cancel_already_terminal_non_execution_then_request_finalized_completed_ack_recovered",
    "ReconciliationConflictHold": "transition_only_no_finalization_no_ack_no_xack",
    "ReconciliationStillUnknownHold": "transition_only_no_finalization_no_ack_no_xack",
}
SLICES = [
    "I1_additive_schema_canonical_codec_and_mixed_replay_no_writer",
    "I2_private_linear_composition_and_transition_builder_no_append",
    "I3_compare_append_batch_covering_seal_and_crash_recovery_no_transport",
    "I4_derived_ack_readiness_facade_no_redis_live",
]

ALLOWED_CHANGED_PATHS = {
    "README.md", str(STATUS), str(ROADMAP), str(AUTHORITY), str(TZ), str(MATRIX), str(NEGATIVE),
    "scripts/stage8a4_durable_composition_implementation_spec_check.py",
    "scripts/stage8a4_durable_composition_implementation_spec_negative_harness.py",
    "scripts/stage8a4_durable_composition_implementation_spec_proof_map.py",
    "scripts/stage8a4_durable_composition_implementation_spec_gate.sh",
    "scripts/stage8a4_durable_composition_implementation_spec_handoff_safety_check.py",
    "scripts/make_stage8a4_durable_composition_implementation_spec_handoff.py",
}


class CheckFailure(RuntimeError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def changed_paths() -> set[str]:
    tracked = subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True).splitlines()
    untracked = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
    return {item for item in tracked + untracked if item}


def check(root: Path = ROOT, *, git_scope: bool = True, changed_paths_override: set[str] | None = None) -> None:
    authority = json.loads((root / AUTHORITY).read_text())
    require(authority["schema_version"] == 1, "schema drift")
    require(authority["stage"] == "8A-4-durable-composition-implementation-spec-R1", "stage drift")
    require(authority["status"] == "implementation_spec_r1_independent_acceptance_pending", "status drift")
    require(authority["branch"] == BRANCH, "branch drift")
    require(authority["accepted_durable_design_ref"] == BASE, "accepted design drift")
    require(authority["accepted_durable_design_review_sha256"] == REVIEW_SHA256, "review hash drift")
    require(authority["accepted_reducer_ref"] == REDUCER, "reducer drift")
    require(authority["accepted_reconciliation_design_ref"] == RECONCILIATION_DESIGN, "reconciliation design drift")
    require(authority["spec_only"] is True, "spec-only disabled")
    require(authority["production_rust_changed"] is False, "production Rust enabled")

    schema = authority["schema_decision"]
    require(schema == {
        "decision": "additive_stage6_journal_record_v2_required",
        "stage6_v1_bytes_immutable": True,
        "stage6_v1_semantics_immutable": True,
        "source_evidence_digest_smuggling_forbidden": True,
        "unknown_v2_skip_allowed": False,
        "historical_rewrite_or_migration_allowed": False,
        "mixed_v1_v2_replay_required": True,
        "canonical_golden_and_restart_compatibility_required": True,
    }, "schema decision drift")
    require(authority["v2_event_kind"] == "ReconciliationTransitionApplied", "V2 event drift")
    require(authority["v2_required_fields"] == V2_FIELDS, "V2 fields drift")
    require(authority["exact_lookup_query_binding_fields"] == QUERY_FIELDS, "query binding drift")
    require(authority["stable_key_fields"] == STABLE_KEY_FIELDS, "stable key drift")
    require(authority["pre_append_cas_fields"] == CAS_FIELDS, "CAS fields drift")

    batch = authority["append_batch"]
    require(batch == {
        "transition_v2_is_first_record": True,
        "deterministic_suffix_manifest_persisted": True,
        "each_record_append_is_durable": True,
        "covering_seal_after_complete_batch_only": True,
        "restart_finds_transition_by_persisted_stable_key": True,
        "restart_appends_only_missing_verified_suffix": True,
        "second_transition_append_allowed": False,
        "same_key_same_payload": "resume_or_return_idempotent_existing_batch",
        "same_key_different_payload": "hard_conflict",
    }, "append batch drift")
    require(authority["endpoint_disposition_matrix"]["place"] == PLACE_MATRIX, "PLACE disposition drift")
    require(authority["endpoint_disposition_matrix"]["cancel"] == CANCEL_MATRIX, "CANCEL disposition drift")

    ack = authority["canonical_ack"]
    require(ack == {
        "recovered_success_status": "Recovered",
        "recovered_success_reason": "RecoveredByBrokerTruth",
        "place_terminal_rejected_status": "Rejected",
        "place_terminal_rejected_reason": "BrokerRejected",
        "terminal_ack_requires_request_finalized": True,
        "terminal_ack_requires_covering_seal": True,
        "hold_ack_or_xack_allowed": False,
        "public_diagnostic_is_ack_authority": False,
    }, "canonical ACK drift")
    controls = authority["post_effect_controls"]
    require(controls == {
        "expired_operator_arm_blocks_reconciliation_append": False,
        "stop_requested_blocks_reconciliation_append": False,
        "stale_or_unreadable_kill_switch_blocks_reconciliation_append": False,
        "stop_or_unreadable_blocks_new_send_and_readiness": True,
        "replay_recreates_arm": False,
        "reconciliation_can_send": False,
    }, "post-effect control drift")
    seal = authority["covering_seal_protocol"]
    require(seal == {
        "pre_append_seal_must_cover_f0": True,
        "post_batch_seal_must_cover_f1": True,
        "post_batch_seal_reread_hmac_canonical_checkpoint_validation": True,
        "publication_before_validated_s1_allowed": False,
        "seal_failure_keeps_batch_durable_and_settlement_pending": True,
    }, "covering seal drift")
    require(authority["implementation_slices"] == SLICES, "slice order drift")
    require(authority["next_after_acceptance"] == "I1 additive schema canonical codec and mixed replay only", "next slice drift")
    require(all(authority["closed"].values()), "closed surface opened")

    tz = (root / TZ).read_text()
    for marker in (
        BASE, REVIEW_SHA256, "specification-only artifact", "Stage6JournalRecordV2",
        "ReconciliationTransitionApplied", "source_evidence_sha256", "mixed V1/V2",
        "deterministic suffix manifest", "different payload is a hard conflict",
        "kill-switch state do not block reconciliation append",
        "ExactWorking", "ExecutionObserved", "AlreadyTerminalNonExecution",
        "RecoveredByBrokerTruth", "I1 — additive schema/codec/replay",
        "FINAM POST/DELETE", "Stage 8A-5", "Stage 8B",
    ):
        require(marker in tz, f"TZ marker missing: {marker}")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 84, "matrix count drift")
    require([row["id"] for row in rows] == [f"S{i:03d}" for i in range(1, 85)], "matrix IDs drift")
    require(len(re.findall(r"^\d+\.", (root / NEGATIVE).read_text(), re.MULTILINE)) == 40, "negative count drift")

    leading = (root / STATUS).read_text().split("## Current accepted boundary", 1)[1].split("\n## ", 1)[0]
    for marker in (BASE, REVIEW_SHA256, "ACCEPTED and CLOSED", "implementation specification R1", "acceptance is pending", "Stage 8A-5"):
        require(marker in leading, f"status marker missing: {marker}")
    roadmap = (root / ROADMAP).read_text()
    require("6ddf54e" in roadmap and "implementation specification R1" in roadmap, "roadmap drift")

    paths = changed_paths() if git_scope else changed_paths_override
    if paths is not None:
        require(paths == ALLOWED_CHANGED_PATHS, f"changed-path drift: {sorted(paths ^ ALLOWED_CHANGED_PATHS)}")
        require(not any(path.startswith(("crates/", "src/", "tests/", ".github/")) for path in paths), "production/test/workflow path changed")
        require("Cargo.toml" not in paths and "Cargo.lock" not in paths, "Cargo changed")
    if git_scope:
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=ROOT, text=True).strip()
        require(branch == BRANCH, "wrong branch")


def main() -> None:
    try:
        check()
    except (CheckFailure, KeyError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage8a4-durable-composition-implementation-spec-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-implementation-spec-check: PASS rows=84 negatives=40 spec-only=true")


if __name__ == "__main__":
    main()
