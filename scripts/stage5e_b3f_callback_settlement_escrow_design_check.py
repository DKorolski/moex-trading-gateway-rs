#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3f settlement-escrow design."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md"
INVENTORY = (
    ROOT
    / "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
STAGE = "5E-b3f-callback-settlement-escrow-design"
BASELINE_REF = "ee23d6b4231c3c1483bcfacdb9189392183e2963"
EXPECTED_PLAN_SHA256 = (
    "3ef4e2a74591d8df043f3a8ed04f635273aa35bcc6f03b400c27f0a973b2f9b2"
)
EXPECTED_INVENTORY_SHA256 = (
    "3ac04ef999f947f7d9218a1ab7c6052a41bb81bce561ca64201d3d11675b7661"
)
EXPECTED_PROTECTED_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "d7458cc5acb0004c9a82eb42675ca7a3672f7c584cd686a1ddaa0b72d8035e41"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "75e3e30deff70fd58f740361395bb82c32981bd6107831dfb21ff037591c6b7d"
    ),
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
    "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3f_callback_settlement_escrow_design_check.py",
]
EXPECTED_STAGE5C_ERROR_MAPPING = {
    "TooManyIntents": "IntentCapacityExceeded",
    "MissingIntentClass": "Stage5cIntentValidationFailed",
    "InstrumentNamespaceMismatch": "Stage5cIntentValidationFailed",
    "InvalidQuantity": "Stage5cIntentValidationFailed",
    "InvalidPrice": "Stage5cIntentValidationFailed",
    "PriceNotTickAligned": "Stage5cIntentValidationFailed",
    "InvalidStopEnd": "Stage5cIntentValidationFailed",
    "ReplayIntentNotExecutable": "PaperModeMismatch",
    "MissingPendingRequest": "Stage5cPendingRequestMismatch",
    "RequestIdMismatch": "Stage5cPendingRequestMismatch",
    "DuplicateRequestId": "Stage5cIntentValidationFailed",
    "UnsupportedIntentAction": "Stage5cIntentValidationFailed",
}


def fail(message: str) -> None:
    print(
        f"stage5e-b3f-callback-settlement-escrow-design-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def require_exact(actual: object, expected: object, message: str) -> None:
    if actual != expected:
        fail(message)


def git_changed_paths() -> list[str]:
    # Negative-harness archive copies intentionally contain no .git metadata;
    # source/hash checks remain authoritative in that environment.
    if not (ROOT / ".git").exists():
        return EXPECTED_ALLOWED_CHANGED_PATHS
    tracked = subprocess.run(
        ["git", "diff", "--name-only", BASELINE_REF, "--"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return sorted(line for line in tracked.stdout.splitlines() if line)


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid inventory: {exc}")

    require_exact(
        canonical_sha256(inventory),
        EXPECTED_INVENTORY_SHA256,
        "design inventory drift",
    )
    require_exact(sha256(PLAN), EXPECTED_PLAN_SHA256, "design plan drift")
    require_exact(
        json.loads(ACTIVE.read_text()),
        {"schema_version": 1, "stage": STAGE},
        "active descriptor drift",
    )
    require_exact(inventory.get("schema_version"), 1, "schema drift")
    require_exact(inventory.get("stage"), STAGE, "stage identity drift")
    require_exact(
        inventory.get("status"),
        "design_r3_cross_contract_closure_pending_review",
        "design status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "baseline drift")
    require_exact(
        inventory.get("expected_provenance_case_count"),
        450,
        "provenance case count drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "allowed changed paths drift",
    )
    require_exact(
        git_changed_paths(),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "design changed-path set drift",
    )

    for relative, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
        require_exact(
            sha256(ROOT / relative),
            expected,
            f"protected B3E implementation source changed: {relative}",
        )
    require_exact(
        inventory.get("protected_b3e_source_sha256"),
        EXPECTED_PROTECTED_SOURCE_SHA256,
        "protected B3E source inventory drift",
    )

    transition = inventory["transition_contract"]
    require_exact(
        transition["only_input"],
        "Stage5ePaperCallbackResultEscrow",
        "settlement sole-input drift",
    )
    require_exact(
        transition["implementation_status"],
        "design_only_not_implemented",
        "settlement implementation opened",
    )
    require_exact(
        transition["borrowed_preflight_before_consume"],
        True,
        "borrowed preflight ordering drift",
    )
    require_exact(transition["consume_count"], 1, "escrow consume-count drift")
    require_exact(
        transition["preflight_decisions"],
        ["ProceedOk", "Terminal"],
        "preflight decision taxonomy drift",
    )
    require_exact(
        transition["consume_after_every_decision"],
        True,
        "terminal preflight ownership conflict reintroduced",
    )
    escrow_bridge = inventory["escrow_bridge_contract"]
    require_exact(
        escrow_bridge["consume_signature"],
        "self,&Stage5ePaperSettlementConsumeSeal->Stage5ePaperSettlementPayload",
        "consume capability must remain borrowable after escrow consume",
    )
    require_exact(
        escrow_bridge["consume_capability_stored_in_payload"],
        False,
        "consume capability escaped into payload",
    )
    require_exact(
        escrow_bridge["consume_capability_stored_in_receipt"],
        False,
        "consume capability escaped into receipt",
    )
    liveness = inventory["consume_capability_liveness_contract"]
    require_exact(liveness["issuance_count"], 1, "consume capability issuance drift")
    require_exact(liveness["storage"], "stack_local_only", "consume capability storage drift")
    require_exact(liveness["consume_borrow_mode"], "shared_borrow", "consume borrow mode drift")
    require_exact(
        liveness["ordered_uses"],
        [
            "borrow_for_sole_escrow_consume",
            "borrow_for_material_seal_after_ProceedOk_classification",
            "borrow_for_settlement_seal_immediately_before_stage5c_bridge",
            "drop_before_success_or_terminal_return",
        ],
        "consume capability liveness order drift",
    )
    require_exact(liveness["material_seal_issuance_count"], 1, "material seal issuance drift")
    require_exact(liveness["settlement_seal_issuance_count"], 1, "settlement seal issuance drift")
    for key in (
        "issuance_before_ProceedOk_classification_allowed",
        "settlement_seal_early_issuance_allowed",
        "capability_return_allowed",
        "capability_reconstruction_allowed",
        "capability_conversion_allowed",
        "second_escrow_consume_possible",
    ):
        require_exact(liveness[key], False, f"consume capability invariant opened: {key}")
    require_exact(
        liveness["compile_fail_cases"],
        [
            "second_consume_after_self_move",
            "consume_capability_clone_or_reconstruction",
            "consume_capability_escape_in_payload_or_receipt",
        ],
        "consume capability compile-fail contract drift",
    )
    require_exact(
        inventory["seal_contract"]["cross_module_visibility"],
        {
            "Stage5ePaperSettlementPreflightSeal": "pub_crate_opaque_private_fields",
            "Stage5ePaperSettlementConsumeSeal": "pub_crate_opaque_private_fields",
        },
        "cross-module seal visibility drift",
    )

    preflight = inventory["preflight_contract"]
    require_exact(
        preflight["ownership"],
        "borrowed_non_decomposable",
        "preflight ownership drift",
    )
    require_exact(
        preflight["raw_intent_export_allowed"],
        False,
        "raw intent export opened",
    )
    require_exact(
        preflight["terminal_decision_still_consumes_escrow"],
        True,
        "terminal preflight consume drift",
    )
    require_exact(
        set(preflight["checks"]),
        {
            "callback_outcome_discriminant",
            "intent_count_lte_u8_max",
            "accepted_bar_origin_live",
            "execution_eligible_true",
            "paper_mode_and_live_orders_disabled",
            "strategy_id_exact_equality",
            "account_id_exact_equality",
            "full_instrument_id_exact_equality",
            "semantic_bar_identity_exact_equality",
            "bar_close_ts_exact_equality",
            "callback_chronology",
            "authority_and_fingerprint_nonzero_equality",
            "no_prior_intent_extraction",
        },
        "preflight check vector drift",
    )
    stage5c_preflight = inventory["stage5c_preflight_bridge_contract"]
    require_exact(
        stage5c_preflight["function"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "Stage 5C preflight bridge drift",
    )
    require_exact(stage5c_preflight["definition_count"], 1, "preflight bridge definition drift")
    require_exact(stage5c_preflight["call_site_count"], 1, "preflight bridge call-site drift")
    require_exact(
        stage5c_preflight["ownership"],
        "immutable_borrows_only",
        "preflight bridge ownership drift",
    )
    require_exact(
        stage5c_preflight["inputs"],
        [
            "&Stage5cPendingRecoveryReceipt",
            "&Stage5ePreCallbackAttributionSnapshot",
            "&Stage5eAcceptedBarSettlementMetadata",
            "&Stage5eB3fStage5cExpectedPreflightBinding",
            "&Stage5ePaperSettlementPreflightSeal",
        ],
        "Stage 5C preflight input topology drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor"],
        "construct_stage5e_b3f_stage5c_expected_preflight_binding",
        "expected-binding constructor drift",
    )
    for key in (
        "intent_extraction_allowed",
        "ownership_mutation_allowed",
        "production_raw_getters_allowed",
        "production_tuple_exports_allowed",
        "test_helper_reuse_allowed",
    ):
        require_exact(stage5c_preflight[key], False, f"preflight privacy opened: {key}")

    relation_matrix = inventory["identity_contract"]["field_source_relation_matrix"]
    require_exact(
        relation_matrix,
        [
            {
                "relation": "strategy_id",
                "sources": ["recovery_admission", "pre_callback_attribution_snapshot"],
                "check": "exact_equality",
            },
            {
                "relation": "account_id",
                "sources": ["recovery_admission", "pre_callback_attribution_snapshot"],
                "check": "exact_equality",
            },
            {
                "relation": "full_instrument_id",
                "sources": [
                    "recovery_admission",
                    "pre_callback_attribution_snapshot",
                    "audit_full_instrument_id",
                    "audit_owned_instrument",
                ],
                "check": "exact_full_InstrumentId_equality",
            },
            {
                "relation": "accepted_semantic_bar_identity",
                "sources": [
                    "pre_callback_attribution_snapshot",
                    "retained_bar_metadata",
                    "audit_accepted_semantic_bar_identity",
                    "audit_owned_bar_identity",
                ],
                "check": "exact_32_byte_equality",
            },
            {
                "relation": "accepted_bar_close_ts",
                "sources": ["pre_callback_attribution_snapshot", "retained_bar_metadata"],
                "check": "exact_i64_equality",
            },
            {
                "relation": "bar_close_to_audit_event_key",
                "sources": [
                    "retained_bar_metadata.accepted_bar_close_ts",
                    "audit_schedule_identity_fingerprint",
                    "audit_full_instrument_id",
                    "audit_sequence_identity_fingerprint",
                    "audit_event_key_fingerprint",
                    "audit_b3b_event_key_fingerprint",
                ],
                "check": "recompute_b3b_event_key_fingerprint_and_exact_compare_to_both_audit_fields",
            },
            {
                "relation": "callback_chronology",
                "sources": [
                    "callback_invoked_at",
                    "retained_bar_metadata.accepted_bar_close_ts",
                    "accepted_b3e_audit_chronology",
                ],
                "check": "callback_not_before_bar_close_and_accepted_b3e_ordering",
            },
            {
                "relation": "callback_authority_and_fingerprints",
                "sources": [
                    "payload_callback_authority_id",
                    "audit_callback_authority_id",
                    "audit_schedule_identity_fingerprint",
                    "audit_sequence_identity_fingerprint",
                    "audit_event_key_fingerprint",
                    "audit_b3b_event_key_fingerprint",
                    "audit_continuation_binding_id",
                    "audit_b3c_continuation_binding_id",
                ],
                "check": "exact_equality_nonzero_and_internal_recomputation",
            },
            {
                "relation": "paper_live_closure",
                "sources": [
                    "recovery_admission",
                    "retained_bar_metadata",
                    "intent_sink_authority",
                ],
                "check": "paper_only_live_orders_absent_origin_Live_execution_eligible",
            },
        ],
        "field/source relation matrix drift",
    )
    require_exact(
        inventory["identity_contract"]["cartesian_source_claim_allowed"],
        False,
        "unsatisfiable Cartesian identity claim reopened",
    )
    require_exact(
        stage5c_preflight["expected_binding_fields"],
        [
            "audit_schedule_identity_fingerprint",
            "audit_sequence_identity_fingerprint",
            "audit_event_key_fingerprint",
            "audit_b3b_event_key_fingerprint",
            "audit_full_instrument_id",
            "audit_owned_instrument",
            "audit_accepted_semantic_bar_identity",
            "audit_owned_bar_identity",
        ],
        "expected-binding field/source schema drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_field_source"],
        "field_for_field_from_Stage5eAuthorizedCallbackAuditLineage",
        "expected-binding field source drift",
    )

    require_exact(
        inventory["capacity_contract"]["maximum_intents"],
        255,
        "Stage 5C intent limit drift",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["canonical_batch_builder"],
        "stage5c_build_paper_intent_batch",
        "canonical Stage 5C builder drift",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["canonical_attribution_builder"],
        "stage5cj_expected_generated_attribution_by_request_from_ledger",
        "canonical Stage 5C attribution builder drift",
    )
    stage5c_bridge = inventory["stage5c_bridge_contract"]
    require_exact(
        stage5c_bridge["seal_issuer"],
        "issue_stage5c_b3f_settlement_seal",
        "Stage 5C settlement seal issuer drift",
    )
    require_exact(stage5c_bridge["seal_issuer_definition_count"], 1, "settlement seal issuer definition drift")
    require_exact(stage5c_bridge["seal_issuer_call_site_count"], 1, "settlement seal issuer call-site drift")
    require_exact(
        stage5c_bridge["canonical_public_entrypoint"],
        "settle_stage5c_semantic_result",
        "canonical public Stage 5C entrypoint drift",
    )
    require_exact(
        stage5c_bridge["canonical_owning_core"],
        "settle_stage5c_semantic_result_owning_core",
        "canonical owning core drift",
    )
    require_exact(stage5c_bridge["canonical_owning_core_definition_count"], 1, "owning core definition drift")
    require_exact(stage5c_bridge["canonical_owning_core_call_site_count"], 2, "owning core call-site drift")
    require_exact(
        stage5c_bridge["existing_public_entrypoint_delegates_to_owning_core"],
        True,
        "legacy canonical entrypoint delegation drift",
    )
    require_exact(
        stage5c_bridge["b3f_bridge_invokes_owning_core_once"],
        True,
        "B3F canonical owning-core call drift",
    )
    require_exact(
        stage5c_bridge["parallel_settlement_algorithm_allowed"],
        False,
        "parallel Stage 5C settlement algorithm opened",
    )
    require_exact(stage5c_bridge["canonical_history_builder"], "stage5ch_batch_summary", "canonical history builder drift")
    require_exact(stage5c_bridge["canonical_initial_history_length"], 1, "canonical initial history length drift")
    require_exact(
        stage5c_bridge["canonical_initial_history_rule"],
        "settled_batch_history_equals_single_stage5ch_batch_summary_of_canonical_batch",
        "canonical settled history rule drift",
    )
    require_exact(
        stage5c_bridge["intent_vector_after_attribution_error"],
        "explicitly_irreversibly_dropped_before_terminal_return",
        "early attribution failure recovered intent vector",
    )
    require_exact(
        inventory["stage5c_bridge_contract"]["stage5e_reimplementation_allowed"],
        False,
        "parallel Stage 5E intent oracle opened",
    )
    material = inventory["stage5c_material_construction_contract"]
    require_exact(
        material["seal_issuer"],
        "issue_stage5c_b3f_settlement_material_seal",
        "Stage 5C material seal issuer drift",
    )
    require_exact(
        material["constructor"],
        "construct_stage5e_stage5c_settlement_material",
        "Stage 5C material constructor drift",
    )
    require_exact(material["constructor_definition_count"], 1, "material constructor count drift")
    require_exact(material["constructor_call_site_count"], 1, "material call-site drift")
    require_exact(
        material["fields"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "pre_callback_attribution_snapshot",
            "retained_bar_metadata",
            "exact_intent_vector",
            "derived_original_intent_count",
        ],
        "Stage 5C material field schema drift",
    )
    require_exact(material["caller_supplied_intent_count_allowed"], False, "caller-supplied intent count reopened")
    require_exact(material["derived_intent_count_type"], "usize", "derived intent count type drift")
    require_exact(
        material["derived_intent_count_source"],
        "exact_intent_vector.len_before_move",
        "intent count/vector binding drift",
    )
    success_return = inventory["stage5c_success_return_contract"]
    require_exact(
        success_return["proof_fields"],
        [
            "strategy_id",
            "account_id",
            "full_instrument_id",
            "accepted_bar_close_timestamp",
            "batch_state_fingerprint",
            "ordered_strategy_request_ids",
            "intent_count_u8",
            "settled_batch_history_length",
            "canonical_first_batch_summary",
        ],
        "Stage 5C success proof schema drift",
    )
    require_exact(
        success_return["history_proof_rule"],
        "length_one_and_first_summary_equals_stage5ch_batch_summary_of_canonical_batch",
        "success history proof drift",
    )
    require_exact(
        success_return["proof_borrow_before_settled_move"],
        True,
        "success proof ordering drift",
    )
    require_exact(success_return["settled_move_count"], 1, "settled strategy move drift")
    terminal_return = inventory["stage5c_terminal_return_contract"]
    require_exact(
        terminal_return["fields"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "pre_callback_attribution_snapshot",
            "retained_bar_metadata",
            "exact_stage5c_intent_settlement_error",
            "derived_original_intent_count",
        ],
        "Stage 5C terminal return schema drift",
    )
    require_exact(terminal_return["mapper_call_count"], 1, "terminal mapper call-count drift")
    require_exact(
        escrow_bridge["payload_consumer_count"],
        1,
        "escrow payload consumer-count drift",
    )
    require_exact(
        escrow_bridge["raw_getters_allowed"],
        False,
        "escrow raw getter opened",
    )
    require_exact(
        inventory["stage5c_error_mapping"],
        EXPECTED_STAGE5C_ERROR_MAPPING,
        "Stage 5C error mapping drift",
    )
    require_exact(
        inventory["stage5c_error_mapping_policy"]["mapping_count"],
        12,
        "Stage 5C mapping cardinality drift",
    )
    require_exact(
        inventory["stage5c_error_mapping_policy"]["wildcard_mapping_allowed"],
        False,
        "Stage 5C wildcard mapping opened",
    )
    require_exact(
        inventory["callback_validation_error_policy"],
        {
            "preflight_decision": "Terminal",
            "disposition": "consume_then_terminal_receipt",
            "reason": "CallbackValidationError",
            "empty_success_batch_allowed": False,
            "callback_retry_allowed": False,
            "escrow_retry_allowed": False,
            "mutated_strategy_retained": True,
            "recovery_ownership_retained": True,
            "audit_lineage_retained": True,
        },
        "callback ValidationError policy drift",
    )
    require_exact(
        inventory["settlement_identity_contract"]["ordered_fields"],
        [
            "callback_authority_id",
            "callback_invocation_timestamp",
            "accepted_semantic_bar_identity",
            "strategy_id",
            "account_id",
            "full_instrument_id",
            "accepted_bar_close_timestamp",
            "stage5c_batch_state_fingerprint",
            "ordered_strategy_request_ids",
            "intent_count_u8",
            "audit_commitment",
        ],
        "settlement identity field vector drift",
    )
    require_exact(
        inventory["canonical_encoding_contract"]["hash"],
        "SHA-256",
        "canonical identity hash drift",
    )
    named = inventory["named_authority_functions"]
    require_exact(
        named["stage5c_settlement_seal_issuer"]["name"],
        "issue_stage5c_b3f_settlement_seal",
        "named settlement seal issuer drift",
    )
    require_exact(
        named["stage5c_settlement_seal_issuer"]["ordering"],
        "immediately_before_settle_stage5e_callback_escrow_material",
        "settlement seal issuer ordering drift",
    )
    require_exact(
        named["stage5c_preflight_bridge"]["name"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "named Stage 5C preflight bridge drift",
    )
    require_exact(
        named["stage5c_error_mapper"]["implementation"],
        "exhaustive_12_arm_match",
        "named error mapper implementation drift",
    )
    require_exact(
        named["stage5c_error_mapper"]["wildcard_arm_allowed"],
        False,
        "named error mapper wildcard opened",
    )
    for authority in named.values():
        require_exact(authority["definition_count"], 1, "named authority definition-count drift")
        require_exact(authority["call_site_count"], 1, "named authority call-site drift")
    terminal_matrix = inventory["terminal_ownership_matrix"]
    required_common = {
        "mutated_strategy",
        "recovery_receipt",
        "audit_lineage",
        "pre_callback_attribution_snapshot",
        "retained_bar_metadata",
        "callback_invoked_at",
        "callback_authority_id",
        "audit_commitment",
    }
    for path_name in (
        "preflight_ok_terminal",
        "callback_validation_error_terminal",
        "stage5c_error_terminal",
    ):
        path = terminal_matrix[path_name]
        if not required_common.issubset(set(path["fields"])):
            fail(f"{path_name} retained ownership drift")
        require_exact(path["retryable"], False, f"{path_name} retry opened")
    if "opaque_exact_ok_callback_outcome_with_intent_vector" not in terminal_matrix[
        "preflight_ok_terminal"
    ]["fields"]:
        fail("preflight Ok callback outcome dropped")
    if "opaque_exact_callback_validation_error" not in terminal_matrix[
        "callback_validation_error_terminal"
    ]["fields"]:
        fail("callback ValidationError ownership dropped")
    if "exact_stage5c_error_some" not in terminal_matrix["stage5c_error_terminal"]["fields"]:
        fail("exact Stage 5C terminal error dropped")
    require_exact(
        inventory["terminal_receipt_contract"]["reasons"],
        [
            "CallbackValidationError",
            "IntentCapacityExceeded",
            "IdentityMismatch",
            "ChronologyMismatch",
            "PaperModeMismatch",
            "Stage5cIntentValidationFailed",
            "Stage5cPendingRequestMismatch",
        ],
        "terminal reason taxonomy drift",
    )
    require_exact(
        set(inventory["terminal_reason_producers"]),
        set(inventory["terminal_receipt_contract"]["reasons"]),
        "terminal reason without exact producer",
    )
    if any(not producers for producers in inventory["terminal_reason_producers"].values()):
        fail("empty terminal reason producer set")
    require_exact(
        inventory["terminal_receipt_contract"]["records_original_intent_count_as_usize"],
        True,
        "terminal original intent count type drift",
    )
    if "true_preflight_intent_count_usize" not in terminal_matrix[
        "preflight_ok_terminal"
    ]["fields"]:
        fail("preflight true intent count dropped")
    if "derived_original_intent_count_usize" not in terminal_matrix[
        "stage5c_error_terminal"
    ]["fields"]:
        fail("Stage 5C derived intent count dropped")
    for contract_name in ("success_receipt_contract", "terminal_receipt_contract"):
        contract = inventory[contract_name]
        require_exact(contract["constructor_count"], 1, f"{contract_name} constructor drift")
        require_exact(
            contract["constructor_call_site_count"],
            1,
            f"{contract_name} call-site drift",
        )
        forbidden = set(contract["forbidden_surfaces"])
        if not {"Debug", "Clone", "From", "Into", "Serialize", "Deserialize"}.issubset(
            forbidden
        ):
            fail(f"{contract_name} forbidden-surface drift")
    if not {
        "settled",
        "into_settled",
        "batch",
        "intent",
        "request_ids",
        "generic_parts",
    }.issubset(set(inventory["success_receipt_contract"]["forbidden_surfaces"])):
        fail("public Stage5c settled inspection escaped success receipt")
    require_exact(
        inventory["exactly_once_contract"]["scope"],
        "process_local_only",
        "exactly-once scope drift",
    )
    require_exact(
        inventory["exactly_once_contract"]["crash_restart_policy_deferred"],
        True,
        "crash/restart policy opened",
    )

    closed = inventory["closed_surfaces"]
    opened_private = {
        "actual_callback_invocation",
        "strategy_state_mutation",
        "in_memory_intent_construction",
    }
    if any(closed[name] is not True for name in opened_private):
        fail("accepted B3E private surface regressed")
    if any(
        value is not False
        for name, value in closed.items()
        if name not in opened_private
    ):
        fail("forbidden B3F surface opened")

    print("stage5e-b3f-callback-settlement-escrow-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
