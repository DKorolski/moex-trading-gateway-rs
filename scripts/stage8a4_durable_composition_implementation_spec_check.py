#!/usr/bin/env python3
"""Fail-closed checker for Stage 8A-4 durable composition implementation spec R2."""

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
R1 = "e3d0ac39dcff25439a7e78f51142b852d8347a2f"
R1_REVIEW_SHA256 = "968f2c61f9c9b01a56e1f8950664d46000b15e038abab74a11089bd91988996b"
REDUCER = "4caf07c16ddad021add7cffe6e887165e49e1bf0"
RECONCILIATION_DESIGN = "cc58c10d22db312cd83640f1c1e7fd86861a4594"

AUTHORITY = Path("docs/stage-8/stage8a4-durable-composition-implementation-spec-authority.json")
TZ = Path("docs/stage-8/TZ_STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R2_2026-08-15.md")
MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R2_ACCEPTANCE_MATRIX_2026-08-15.csv")
NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R2_NEGATIVE_INVENTORY_2026-08-15.md")
OLD_TZ = Path("docs/stage-8/TZ_STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_2026-08-15.md")
OLD_MATRIX = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_ACCEPTANCE_MATRIX_2026-08-15.csv")
OLD_NEGATIVE = Path("docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_IMPLEMENTATION_R1_NEGATIVE_INVENTORY_2026-08-15.md")
STATUS = Path("docs/current-status.md")
ROADMAP = Path("docs/roadmap.md")

OUTER_FIELDS = [
    "schema_version", "journal_record_id", "lifecycle_sequence",
    "previous_record_id", "causal_parent_id", "durable_request_identity",
    "event_kind", "payload", "canonical_payload_sha256",
    "source_evidence_sha256",
]
V2_DTOS = [
    "Stage6ReconciliationEndpointKindV2",
    "Stage6ReconciliationTransitionKindV2",
    "Stage6ReconciliationLifecycleV2",
    "Stage6ReconciliationFillEffectV2",
    "Stage6ExactLookupEvidenceV2",
    "Stage6BrokerOrderFactV2",
    "Stage6MaterialTradeFactV2",
    "Stage6AccountSafetySummaryV2",
    "Stage6PreAppendPreconditionV2",
    "Stage6SuffixManifestV2",
    "Stage6SuffixManifestEntryV2",
]
V2_PAYLOAD_FIELDS = [
    "stable_transition_key_sha256", "durable_request_binding_sha256",
    "private_authoritative_outcome_binding_sha256", "endpoint_kind",
    "transition_kind", "exact_lookup_evidence", "broker_order_fact",
    "material_trade_facts", "fill_effect", "account_safety_summary",
    "pre_append_precondition", "deterministic_suffix_manifest",
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
LOOKUP_UNION = {
    "NotAttempted": ["state"],
    "Succeeded": ["state", "account_id", "queried_broker_order_id", "durable_request_binding_sha256", "request_started_at", "response_received_at", "exact_order_observation_v2"],
    "DocumentedNotFound": ["state", "account_id", "queried_broker_order_id", "durable_request_binding_sha256", "request_started_at", "response_received_at", "documented_status_category"],
    "Unavailable": ["state", "account_id", "queried_broker_order_id", "durable_request_binding_sha256", "request_started_at", "response_received_at", "failure_category"],
    "DecodeFailure": ["state", "account_id", "queried_broker_order_id", "durable_request_binding_sha256", "request_started_at", "response_received_at", "response_status_category", "response_binding_sha256"],
    "Stale": ["state", "account_id", "queried_broker_order_id", "durable_request_binding_sha256", "request_started_at", "response_received_at", "stale_observation_binding_sha256"],
}
PENDING_FIELDS = [
    "stable_transition_key_sha256", "transition_kind",
    "canonical_v2_record_sha256", "deterministic_suffix_manifest",
    "verified_suffix_prefix_length", "batch_completion_state",
    "last_mixed_record_id", "last_mixed_lifecycle_sequence",
]
MANIFEST_FIELDS = [
    "ordinal", "event_kind", "journal_record_id", "lifecycle_sequence",
    "canonical_payload_sha256", "canonical_record_sha256",
]
GOLDENS = [
    "PlaceExactWorkingBrokerOrderIdPresent",
    "PlaceExactWorkingBrokerOrderIdAbsent",
    "PlaceExactTerminalRejected",
    "PlacePartialFillTradeBrokerOrderIdPresent",
    "PlacePartialFillClientLinkedTradeBrokerOrderIdAbsent",
    "CancelExactWorking", "CancelTerminalCancelled", "ConflictHold",
    "StillUnknownHold", "ExactLookupNotAttempted",
    "ExactLookupSucceededWithObservation", "ExactLookupDocumentedNotFound",
    "ExactLookupUnavailable", "ExactLookupDecodeFailure", "ExactLookupStale",
    "MixedV1V2", "MixedV1V2PartialV1Suffix",
    "MixedV1V2CompleteV1Suffix", "UnknownRecordSchemaVersionFailClosed",
    "V1GoldenBytesUnchanged",
]
SLICES = [
    "I1_additive_schema_canonical_codec_and_mixed_replay_no_writer",
    "I2_private_linear_composition_and_transition_builder_no_append",
    "I3_compare_append_batch_covering_seal_and_crash_recovery_no_transport",
    "I4_derived_ack_readiness_facade_no_redis_live",
]

ALLOWED_CHANGED_PATHS = {
    "README.md", str(STATUS), str(ROADMAP), str(AUTHORITY), str(TZ), str(MATRIX),
    str(NEGATIVE),
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
    a = json.loads((root / AUTHORITY).read_text())
    require(a["schema_version"] == 2, "schema drift")
    require(a["stage"] == "8A-4-durable-composition-implementation-spec-R2", "stage drift")
    require(a["status"] == "implementation_spec_r2_independent_acceptance_pending", "status drift")
    require(a["branch"] == BRANCH, "branch drift")
    require(a["accepted_durable_design_ref"] == BASE, "accepted design drift")
    require(a["accepted_durable_design_review_sha256"] == REVIEW_SHA256, "design review drift")
    require(a["rejected_implementation_spec_r1_ref"] == R1, "R1 lineage drift")
    require(a["rejected_implementation_spec_r1_review_sha256"] == R1_REVIEW_SHA256, "R1 review drift")
    require(a["accepted_reducer_ref"] == REDUCER, "reducer drift")
    require(a["accepted_reconciliation_design_ref"] == RECONCILIATION_DESIGN, "reconciliation design drift")
    require(a["spec_only"] is True and a["production_rust_changed"] is False, "spec-only boundary drift")

    require(a["schema_decision"] == {
        "decision": "additive_stage6_journal_record_v2_required",
        "stage6_v1_bytes_immutable": True,
        "stage6_v1_record_ids_immutable": True,
        "stage6_v1_semantics_immutable": True,
        "source_evidence_digest_smuggling_forbidden": True,
        "unknown_v2_skip_allowed": False,
        "historical_rewrite_or_migration_allowed": False,
        "mixed_v1_v2_replay_required": True,
        "canonical_golden_and_restart_compatibility_required": True,
    }, "schema decision drift")
    require(a["versioned_record"] == {
        "variants": ["V1(Stage6JournalRecordV1)", "V2(Stage6JournalRecordV2)"],
        "supported_record_schema_versions": [1, 2],
        "unknown_or_ambiguous_version_action": "fail_closed",
        "failed_v2_decode_falls_back_to_v1": False,
    }, "version dispatch drift")
    require(a["v2_record"] == {
        "schema_version": 2, "outer_fields": OUTER_FIELDS,
        "event_kind_type": "Stage6JournalEventKindV2",
        "event_kind_variants": ["ReconciliationTransitionApplied"],
        "event_kind_type_is_separate_from_v1": True,
        "record_id_rule": "reuse_stage6_journal_record_v1_domain_sha256_strategy_request_id_and_lifecycle_sequence",
        "record_id_domain": "stage6-journal-record-v1",
        "sequence_rule": "exact_next_request_lifecycle_sequence",
        "previous_record_rule": "exact_current_request_last_record_id",
        "causal_parent_rule": "same_as_previous_record_id",
        "durable_identity_rule": "exact_existing_stage6_durable_request_identity_v1",
        "payload_digest_rule": "sha256_exact_canonical_v2_payload_bytes",
        "source_evidence_rule": "stage6_sha256_digest_bound_to_authoritative_acquisition_evidence",
    }, "V2 envelope drift")
    require(a["framed_journal_dispatch"] == {
        "storage_schema_version": 1, "frame_magic": "S6F1", "frame_version": 1,
        "storage_or_frame_schema_changed": False,
        "dispatch_rule": "inspect_exact_top_level_record_schema_version_inside_verified_frame_body",
        "schema_1_decoder": "existing_stage6_journal_record_v1_decode_canonical_byte_identical",
        "schema_2_decoder": "stage6_journal_record_v2_decode_canonical",
        "unknown_version_action": "fail_closed",
        "malformed_or_ambiguous_discriminator_action": "fail_closed",
        "v2_decode_failure_fallback_allowed": False,
        "v1_only_binary_encountering_v2": "fail_closed",
    }, "framed dispatch drift")
    require(a["dedicated_v2_dtos"] == V2_DTOS, "V2 DTO drift")
    require(a["dto_policy"] == {
        "owner": "strategy_runtime_core_stage6_persistence",
        "serializes_private_finam_gateway_types": False,
        "serializes_unversioned_live_domain_structs_wholesale": False,
        "deny_unknown_fields_equivalent": True,
        "canonical_decimal_string_time_encoding": True,
        "bounded_collections_required": True,
        "transport_tokens_urls_or_send_capability_allowed": False,
    }, "DTO policy drift")
    require(a["v2_payload_fields"] == V2_PAYLOAD_FIELDS, "V2 payload drift")
    require(a["stable_key_fields"] == STABLE_KEY_FIELDS, "stable key drift")
    require(a["pre_append_cas_fields"] == CAS_FIELDS, "CAS drift")
    require(a["exact_lookup_union"] == LOOKUP_UNION, "lookup union drift")

    replay = a["mixed_replay"]
    require(replay == {
        "v2_validates_exact_durable_request_identity": True,
        "v2_requires_next_sequence_and_exact_previous_link": True,
        "v2_advances_last_sequence_and_record_id": True,
        "v2_registers_pending_reconciliation_batch": True,
        "v2_itself_applies_v1_suffix_semantics": False,
        "v2_authorizes_send_retry_or_settlement": False,
        "v2_after_finalized_request_allowed": False,
        "pending_batch_fields": PENDING_FIELDS,
        "partial_suffix_rule": "verify_exact_manifest_prefix_and_retain_missing_suffix_pending",
        "complete_suffix_rule": "apply_v1_semantics_in_order_and_mark_batch_complete",
        "unexpected_record_inside_batch": "hard_conflict",
        "same_key_same_canonical_v2_record": "idempotent_existing_batch",
        "same_key_different_payload_or_record": "hard_conflict",
        "process_local_outcome_required_after_restart": False,
    }, "mixed replay drift")
    require(a["future_append_batch"] == {
        "transition_v2_is_first_record": True,
        "each_record_append_is_durable": True,
        "covering_seal_after_complete_batch_only": True,
        "restart_finds_transition_by_persisted_stable_key": True,
        "restart_appends_only_missing_verified_suffix": True,
        "second_transition_append_allowed": False,
        "same_key_same_record": "resume_or_return_idempotent_existing_batch",
        "same_key_different_payload_or_record": "hard_conflict",
    }, "future append-batch drift")
    require(a["suffix_manifest_entry_fields"] == MANIFEST_FIELDS, "suffix fields drift")
    require(a["suffix_manifest_policy"] == {
        "canonical_record_sha256_rule": "sha256_exact_stage6_journal_record_v1_encode_canonical_bytes",
        "binds_previous_causal_identity_payload_source_and_sequence": True,
        "exact_full_record_marks_entry_complete": True,
        "same_record_id_different_record_hash": "hard_conflict",
        "same_payload_different_full_record": "hard_conflict",
        "missing_next_entry": "append_exact_reconstructed_record_in_later_writer_slice",
        "unexpected_extra_record": "hard_conflict",
    }, "suffix policy drift")
    require(a["fact_projection"] == {
        "v2_is_complete_durable_reconciliation_fact": True,
        "v1_suffix_is_lossless_compatibility_projection": True,
        "broker_order_id_may_be_absent_in_v2": True,
        "v1_broker_order_observed_requires_real_broker_order_id": True,
        "v1_broker_trade_observed_requires_real_compatible_broker_order_id": True,
        "missing_broker_order_id_is_never_fabricated": True,
        "client_linked_trade_without_broker_id_retained_in_v2": True,
        "request_finalization_depends_on_endpoint_disposition_not_fabricated_id": True,
        "suffix_manifest_describes_exact_representable_subset": True,
    }, "fact projection drift")
    require(a["endpoint_disposition_matrix"] == {
        "place": {
            "ExactWorking": "request_finalized_completed_ack_recovered",
            "ExactTerminalFilled": "request_finalized_completed_ack_recovered",
            "ExactTerminalRejected": "request_finalized_rejected_ack_rejected",
            "ExactTerminalCancelled": "request_finalized_completed_ack_recovered",
            "ExactTerminalExpired": "request_finalized_completed_ack_recovered",
            "ReconciliationConflictHold": "no_finalization_no_ack_no_xack",
            "ReconciliationStillUnknownHold": "no_finalization_no_ack_no_xack",
        },
        "cancel": {
            "ExactWorking": "unresolved_no_finalization_no_ack_no_xack",
            "ExactTerminalFilled": "cancel_execution_observed_request_finalized_completed_ack_recovered",
            "ExactTerminalRejected": "cancel_already_terminal_non_execution_request_finalized_completed_ack_recovered",
            "ExactTerminalCancelled": "cancel_canceled_request_finalized_completed_ack_recovered",
            "ExactTerminalExpired": "cancel_already_terminal_non_execution_request_finalized_completed_ack_recovered",
            "ReconciliationConflictHold": "no_finalization_no_ack_no_xack",
            "ReconciliationStillUnknownHold": "no_finalization_no_ack_no_xack",
        },
    }, "endpoint disposition drift")
    require(a["canonical_ack"] == {
        "recovered_success_status": "Recovered",
        "recovered_success_reason": "RecoveredByBrokerTruth",
        "place_terminal_rejected_status": "Rejected",
        "place_terminal_rejected_reason": "BrokerRejected",
        "terminal_ack_requires_request_finalized": True,
        "terminal_ack_requires_covering_seal": True,
        "hold_ack_or_xack_allowed": False,
        "public_diagnostic_is_ack_authority": False,
    }, "canonical ACK drift")
    require(a["post_effect_controls"] == {
        "expired_operator_arm_blocks_reconciliation_append": False,
        "stop_requested_blocks_reconciliation_append": False,
        "stale_or_unreadable_kill_switch_blocks_reconciliation_append": False,
        "stop_or_unreadable_blocks_new_send_and_readiness": True,
        "replay_recreates_arm": False,
        "reconciliation_can_send": False,
    }, "post-effect controls drift")
    require(a["covering_seal_protocol"] == {
        "pre_append_seal_must_cover_f0": True,
        "post_batch_seal_must_cover_f1": True,
        "post_batch_seal_reread_hmac_canonical_checkpoint_validation": True,
        "publication_before_validated_s1_allowed": False,
        "seal_failure_keeps_batch_durable_and_settlement_pending": True,
    }, "covering seal drift")
    require(a["i1_golden_cases"] == GOLDENS, "golden inventory drift")
    require(a["implementation_slices"] == SLICES, "slice order drift")
    require(a["next_after_acceptance"] == "I1 additive schema canonical codec and mixed replay only", "next slice drift")
    require(all(a["closed"].values()), "closed surface opened")

    tz = (root / TZ).read_text()
    for marker in (BASE, R1, R1_REVIEW_SHA256, "Stage6JournalRecordVersioned",
                   "Stage6JournalRecordV2", "Stage6JournalEventKindV2", "S6F1",
                   "Failed V2 decode never falls back", "exact_order_observation_v2",
                   "pending reconciliation batch", "canonical_record_sha256",
                   "broker_order_id = None", "never fabricate", "I1 — additive",
                   "FINAM POST/DELETE", "Stage 8A-5", "Stage 8B"):
        require(marker in tz, f"TZ marker missing: {marker}")
    require(not (root / OLD_TZ).exists() and not (root / OLD_MATRIX).exists() and not (root / OLD_NEGATIVE).exists(), "stale R1 artifact remains")

    with (root / MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    require(len(rows) == 105, "matrix count drift")
    require([row["id"] for row in rows] == [f"S{i:03d}" for i in range(1, 106)], "matrix ID drift")
    require(len(re.findall(r"^\d+\.", (root / NEGATIVE).read_text(), re.MULTILINE)) == 57, "negative count drift")

    leading = (root / STATUS).read_text().split("## Current accepted boundary", 1)[1].split("\n## ", 1)[0]
    for marker in (BASE, R1, "R1 was not accepted", "specification R2", "acceptance is pending", "Stage 8A-5"):
        require(marker in leading, f"status marker missing: {marker}")
    roadmap = (root / ROADMAP).read_text()
    require("6ddf54e" in roadmap and "implementation specification R2" in roadmap and "e3d0ac3" in roadmap, "roadmap drift")
    readme = (root / "README.md").read_text()
    require("implementation specification R2" in readme and "e3d0ac3" in readme, "README drift")

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
    print("stage8a4-durable-composition-implementation-spec-check: PASS rows=105 negatives=57 spec-only=true")


if __name__ == "__main__":
    main()
