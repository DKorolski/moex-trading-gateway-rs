#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3e callback invocation design."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3e-callback-invocation-design.md"
INVENTORY = (
    ROOT / "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
STAGE = "5E-b3e-callback-invocation-design"
BASELINE_REF = "175b172b61e580d4db81aad8182020fabd38e482"
EXPECTED_PLAN_SHA256 = (
    "55e2f080cc6fe7aa0fb91c53fa524cf8b385d49d7716afeff82df6af4ee8a849"
)
EXPECTED_INVENTORY_SHA256 = (
    "87e7ef104787c4632279f80e76fefd75fefd1064ab02fc1a376d0f9c34b15b03"
)
EXPECTED_B3D_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "9637a6065452b7b46581601bbee8c0270f65dc04207f15b530d3531a36872d1c"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "30d87cb4313b961f3159d2ca4e5ef214ee2009d0358a96ace945e6794b41ae6c"
    ),
    "docs/stage-5/5e-b3d-callback-authority-design.md": (
        "ae58b3b839782fa4f899079bb19edcac84840246f415d753314a1ad19e5476b6"
    ),
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json": (
        "97800a8d0de98266cdc26ebc85bdee224c7ab4ff6705636a9136eda8a6a54037"
    ),
    "scripts/stage5e_b3d_callback_authority_design_check.py": (
        "143a719d916ca2f539fa1faf0849f69d1780a9ba86cd78120b9808cdf9747ff0"
    ),
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3e-callback-invocation-design.md",
    "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3e_callback_invocation_design_check.py",
]


def fail(message: str) -> None:
    print(
        f"stage5e-b3e-callback-invocation-design-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def require_exact(value: object, expected: object, message: str) -> None:
    if value != expected:
        fail(message)


def check_inventory(inventory: dict[str, object]) -> None:
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
        "design_only_r7_pending_review",
        "design-only status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "baseline drift")
    require_exact(
        inventory.get("accepted_b3e_design_ref"),
        "135c3d1ed923d80c0c3de03e9d9e9d4a279985d3",
        "accepted B3E design ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r1_ref"),
        "06107da3bf5809e34504f740e5c260b29a315b9c",
        "accepted B3E-r1 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r2_ref"),
        "c134da8da519dd09473699e1c23c2bc9d96a5a2f",
        "accepted B3E-r2 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r3_ref"),
        "a41ec4420736c66b92425f0f977fc9957d611df3",
        "accepted B3E-r3 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r4_ref"),
        "135c3d1ed923d80c0c3de03e9d9e9d4a279985d3",
        "accepted B3E-r4 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r5_ref"),
        "4378f576a7da6389b219de18340f69949fb76625",
        "accepted B3E-r5 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3e_r6_ref"),
        BASELINE_REF,
        "accepted B3E-r6 ref drift",
    )
    require_exact(
        inventory.get("accepted_b3d_implementation_ref"),
        "93d365ae51f2f6ad94954782a27bc49857fe21ff",
        "accepted B3D implementation ref drift",
    )
    require_exact(
        inventory.get("expected_provenance_case_count"),
        318,
        "negative-matrix count drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "design changed-path contract drift",
    )
    require_exact(
        inventory.get("accepted_b3d_source_sha256"),
        EXPECTED_B3D_SOURCE_SHA256,
        "accepted B3D source hash contract drift",
    )

    transition = inventory["invocation_transition_contract"]
    require_exact(
        transition,
        {
            "function": "invoke_stage5e_authorized_paper_callback",
            "implementation_status": "design_only_not_implemented",
            "only_input": "Stage5eCallbackAuthorityReadyPaperStrategy",
            "success": "Stage5ePaperCallbackResultEscrow",
            "blocked": "Stage5eCallbackInvocationTerminalBlock",
            "production_clock": "captured_inside_transition",
            "caller_supplied_production_clock_allowed": False,
            "test_clock_seam": "cfg_test_only",
            "preflight_before_consume": True,
            "callback_after_all_checks_only": True,
            "callback_count_on_blocked_path": 0,
            "callback_count_on_success_path": 1,
            "callback_invocation_implies_in_memory_intent_construction": True,
            "legacy_stage5c_route_allowed": False,
        },
        "invocation transition contract drift",
    )
    consume_context = inventory["invocation_consume_context_contract"]
    require_exact(consume_context["owner"], "callback_authority", "invocation context owner drift")
    require_exact(
        consume_context["visibility"],
        "pub_crate_opaque_private_fields",
        "invocation context visibility drift",
    )
    require_exact(consume_context["constructor_count"], 1, "invocation context constructor count drift")
    require_exact(consume_context["consumer_count"], 1, "invocation context consumer count drift")
    require_exact(
        consume_context["access_method"],
        "Stage5eB3eInvocationConsumeContext::consume_for_nested_b3c",
        "invocation context nested access seam drift",
    )
    require_exact(consume_context["access_method_call_site_count"], 1, "context access call-site drift")
    require_exact(
        consume_context["access_capability"],
        "&Stage5eB3eNestedConsumeSeal",
        "context access capability drift",
    )
    require_exact(
        consume_context["nested_output_owner"],
        "b3c_evidence",
        "nested invocation material owner drift",
    )
    require_exact(
        consume_context["nested_output_visibility"],
        "pub_crate_opaque_private_fields",
        "nested invocation material fields widened",
    )
    require_exact(
        consume_context["nested_output_constructor"],
        "b3c_evidence::construct_nested_invocation_material",
        "nested invocation material constructor drift",
    )
    require_exact(
        consume_context["nested_output_constructor_count"],
        1,
        "nested invocation material constructor count drift",
    )
    require_exact(
        consume_context["nested_output_consumer_count"],
        1,
        "nested invocation material consumer count drift",
    )
    require_exact(
        consume_context["fields"],
        [
            "callback_now",
            "callback_authority_id",
            "issued_at",
            "effective_observed_at",
            "authority_expires_at",
            "full_instrument_id",
            "accepted_semantic_bar_identity",
            "b3b_event_key_fingerprint",
            "b3c_continuation_binding_id",
            "sequence_identity_fingerprint",
        ],
        "invocation context field vector drift",
    )
    require_exact(
        consume_context["clock_flow"],
        [
            "callback_now_to_stage5c_materialization",
            "same_callback_now_to_payload_callback_invoked_at",
            "stage5c_strategy_now_equals_escrow_callback_invoked_at",
        ],
        "callback clock flow drift",
    )
    require_exact(
        consume_context["authority_audit_flow"],
        [
            "callback_authority_id_to_audit_lineage",
            "issued_at_to_audit_lineage",
            "effective_observed_at_to_audit_lineage",
            "authority_expires_at_to_audit_lineage",
        ],
        "outer authority audit flow drift",
    )
    for forbidden in (
        "raw_getters",
        "partial_extraction",
        "Copy",
        "From",
        "Into",
        "alternate_constructor",
        "second_consumer",
        "caller_clock",
        "issued_at_as_callback_now",
    ):
        if forbidden not in consume_context["forbidden_surfaces"]:
            fail(f"invocation context surface weakened: {forbidden}")

    preflight = inventory["callback_time_preflight_contract"]
    require_exact(
        preflight["checks_in_order"],
        [
            "authority_effective_observed_at_equals_owned_b3c_effective_observed_at",
            "owned_b3c_effective_observed_at_not_after_issued_at",
            "issued_at_not_after_now",
            "now_not_after_authority_expires_at",
            "issued_at_not_after_authority_expires_at",
            "authority_expiry_equals_owned_b3c_effective_expiry",
            "accepted_bar_close_not_after_issued_at",
            "accepted_bar_close_not_future",
            "full_instrument_identity_complete",
            "all_frozen_identity_fields_present_and_nonzero",
            "owned_identity_fields_match_b3c",
            "callback_authority_id_recomputed_and_equal",
        ],
        "callback-time check order drift",
    )
    for field in (
        "grace_period_allowed",
        "expiry_extension_allowed",
        "refresh_allowed",
        "ledger_lookup_allowed",
        "persisted_capability_allowed",
        "surrogate_ownership_id_allowed",
    ):
        require_exact(preflight[field], False, f"forbidden preflight surface opened: {field}")

    topology = inventory["module_and_consume_topology"]
    require_exact(
        topology["orchestrator_owner"],
        "strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority",
        "callback orchestrator owner drift",
    )
    require_exact(
        topology["nested_b3c_owner"],
        "strategy_runtime_core::stage5e_no_io_lifecycle::schedule_window_evidence::b3c_evidence",
        "nested B3C owner drift",
    )
    require_exact(
        topology["stage5c_callback_material_owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "Stage5C callback material owner drift",
    )
    require_exact(
        topology["authority_consume_method"],
        "Stage5eCallbackAuthorityReadyPaperStrategy::consume_for_callback",
        "authority consume method drift",
    )
    require_exact(
        topology["authority_consume_input"],
        "Stage5eCallbackInvocationSeal_and_Stage5eB3eInvocationConsumeContext",
        "authority consume context transport drift",
    )
    require_exact(
        topology["authority_consume_output"],
        "Result<Stage5eAuthorizedPaperCallbackPayload,Stage5eCallbackInvocationTerminalBlock>",
        "authority consume fallibility drift",
    )
    require_exact(topology["authority_consume_call_site_count"], 1, "second authority consumer opened")
    require_exact(
        topology["nested_consume_method"],
        "Stage5eBoundSessionCalendarSequenceForObservedLiveBar::consume_for_authorized_callback_with_nested_seal_and_invocation_context",
        "nested consume method drift",
    )
    require_exact(
        topology["nested_consume_output"],
        "Result<Stage5eAuthorizedPaperCallbackPayload,Stage5eCallbackInvocationTerminalBlock>",
        "nested consume fallibility drift",
    )
    require_exact(
        topology["nested_consume_seal"],
        "Stage5eB3eNestedConsumeSeal",
        "nested consume seal drift",
    )
    require_exact(topology["nested_consume_seal_constructor_count"], 1, "second nested seal issuer opened")
    require_exact(topology["payload_constructor_count"], 1, "payload constructor count drift")
    require_exact(
        topology["payload_fields"],
        [
            "stage5e_stage5c_authorized_callback_material",
            "stage5e_authorized_callback_audit_lineage",
            "callback_invoked_at",
            "callback_authority_id",
        ],
        "linear consume payload schema drift",
    )
    for forbidden in (
        "generic_into_parts",
        "raw_strategy_getter",
        "raw_semantic_bar_getter",
        "alternate_constructor",
        "second_consumer",
    ):
        if forbidden not in topology["payload_forbidden_surfaces"]:
            fail(f"consume payload forbidden surface weakened: {forbidden}")

    payload = inventory["authorized_payload_contract"]
    require_exact(payload["owner"], "callback_authority", "authorized payload owner drift")
    require_exact(
        payload["visibility"],
        "pub_crate_opaque_private_fields",
        "authorized payload fields widened",
    )
    require_exact(
        payload["constructor"],
        "construct_stage5e_authorized_paper_callback_payload",
        "authorized payload constructor drift",
    )
    require_exact(payload["constructor_definition_count"], 1, "authorized payload constructor count drift")
    require_exact(payload["constructor_call_site_count"], 1, "authorized payload constructor call-site drift")
    require_exact(
        payload["constructor_capability"],
        "&Stage5eB3eNestedConsumeSeal",
        "authorized payload nested capability drift",
    )
    require_exact(
        payload["fields"],
        [
            "stage5c_authorized_callback_material",
            "authorized_callback_audit_lineage",
            "callback_invoked_at",
            "callback_authority_id",
        ],
        "authorized payload schema drift",
    )
    require_exact(
        payload["sole_consumer"],
        "Stage5eAuthorizedPaperCallbackPayload::invoke_callback_once_in_authority",
        "authorized payload consumer drift",
    )
    require_exact(payload["consumer_call_site_count"], 1, "authorized payload consumer count drift")
    require_exact(
        payload["post_callback_fields"],
        [
            "post_callback_material",
            "audit_lineage",
            "callback_invoked_at",
            "callback_authority_id",
        ],
        "authorized post-callback payload schema drift",
    )
    require_exact(payload["post_callback_consumer_count"], 1, "authorized post-callback consumer count drift")
    for forbidden in (
        "public_fields",
        "raw_getters",
        "generic_into_parts",
        "alternate_constructor",
        "constructor_without_nested_capability",
        "second_consumer",
    ):
        if forbidden not in payload["forbidden_surfaces"]:
            fail(f"authorized payload surface weakened: {forbidden}")

    require_exact(
        inventory["cross_module_seal_topology"],
        [
            {"seal": "Stage5eB3eNestedPreflightSeal", "owner": "callback_authority", "visibility": "pub_crate_opaque_private_fields", "constructor": "one_private_owner_constructor", "authorized_use": "borrow_b3c_callback_preflight_once"},
            {"seal": "Stage5eB3eNestedConsumeSeal", "owner": "callback_authority", "visibility": "pub_crate_opaque_private_fields", "constructor": "one_private_owner_constructor", "authorized_use": "authorize_b3c_consume_and_stage5c_material_seal_issuance"},
            {"seal": "Stage5cB3eCallbackMaterialSeal", "owner": "stage5c_paper_host", "visibility": "pub_crate_opaque_private_fields", "constructor": "one_private_stage5c_constructor", "authorized_use": "authorize_stage5c_materialization_once"},
            {"seal": "Stage5cB3eCallbackExecutionSeal", "owner": "callback_authority", "visibility": "pub_crate_opaque_private_fields", "constructor": "one_private_owner_constructor", "authorized_use": "authorize_one_material_callback"},
            {"seal": "Stage5eEscrowConstructionSeal", "owner": "callback_authority", "visibility": "pub_crate_opaque_private_fields", "constructor": "one_private_owner_constructor", "authorized_use": "authorize_one_post_callback_escrow_construction"},
        ],
        "cross-module seal topology drift",
    )
    required_seal_forbidden = {
        "Clone",
        "Copy",
        "Default",
        "Serialize",
        "Deserialize",
        "From",
        "Into",
        "public_constructor",
        "second_constructor",
        "second_authorized_call_site",
    }
    if not required_seal_forbidden.issubset(
        set(inventory["cross_module_seal_forbidden_traits_and_surfaces"])
    ):
        fail("cross-module seal forbidden surfaces weakened")

    nested_preflight = inventory["nested_b3c_invocation_preflight_contract"]
    require_exact(
        nested_preflight["seal"],
        "Stage5eB3eNestedPreflightSeal",
        "nested B3C preflight seal drift",
    )
    require_exact(
        nested_preflight["view"],
        "Stage5eB3eNestedPreflight<'a>",
        "nested B3C preflight view drift",
    )
    require_exact(nested_preflight["distinct_from_issue_seal"], True, "B3D issue seal reused")
    require_exact(
        nested_preflight["conversion_from_issue_seal_allowed"],
        False,
        "B3D issue-seal conversion opened",
    )
    require_exact(nested_preflight["seal_constructor_count"], 1, "nested preflight issuer count drift")
    require_exact(nested_preflight["borrow_call_site_count"], 1, "nested preflight call-site drift")
    require_exact(
        nested_preflight["consume_before_validation_allowed"],
        False,
        "B3C consume-before-validation opened",
    )
    require_exact(nested_preflight["raw_objects_allowed"], False, "raw B3C preflight object opened")

    material = inventory["stage5c_callback_materialization_contract"]
    require_exact(material["seal"], "Stage5cB3eCallbackMaterialSeal", "Stage5C material seal drift")
    require_exact(material["function"], "consume_stage5c_for_authorized_callback", "Stage5C bridge drift")
    require_exact(material["seal_constructor_count"], 1, "Stage5C seal constructor count drift")
    require_exact(
        material["seal_visibility"],
        "pub_crate_opaque_private_fields",
        "Stage5C material seal visibility drift",
    )
    require_exact(
        material["seal_issuer"],
        "issue_stage5c_b3e_callback_material_seal",
        "Stage5C material seal issuer drift",
    )
    require_exact(material["seal_issuer_visibility"], "pub_crate", "Stage5C issuer unreachable")
    require_exact(
        material["seal_issuer_signature"],
        "(&Stage5eB3eNestedConsumeSeal)->Stage5cB3eCallbackMaterialSeal",
        "Stage5C issuer capability signature drift",
    )
    require_exact(
        material["seal_issuer_requires_borrowed_nested_consume_capability"],
        True,
        "Stage5C material seal issuer capability removed",
    )
    require_exact(material["issuer_call_site_count"], 1, "Stage5C seal issuer count drift")
    require_exact(material["function_call_site_count"], 1, "Stage5C bridge call-site count drift")
    require_exact(
        material["output"],
        "Result<Stage5eStage5cAuthorizedCallbackMaterial,Stage5eStage5cMaterializationTerminalBlock>",
        "Stage5C materialization result drift",
    )
    require_exact(
        material["integrity_failure_policy"],
        {
            "failure": "admission_instrument_or_tick_mismatch_against_owned_accepted_bar",
            "result": "Stage5eStage5cMaterializationTerminalBlock",
            "reason": "MaterializationIntegrityMismatch",
            "failure_is_post_consume_terminal": True,
            "returns_strategy": False,
            "returns_recovery_receipt": False,
            "returns_authority": False,
            "panic_allowed": False,
            "retry_allowed": False,
            "alternate_success_material_allowed": False,
        },
        "Stage5C materialization failure policy drift",
    )
    require_exact(
        material["terminal_type_contract"],
        {
            "type": "Stage5eStage5cMaterializationTerminalBlock",
            "owner": "strategy_runtime_core::stage5c_paper_host",
            "visibility": "pub_crate_opaque_private_zero_sized_fields",
            "constructor": "construct_stage5e_stage5c_materialization_terminal_block",
            "constructor_count": 1,
            "constructor_call_site_count": 1,
            "sole_reason": "MaterializationIntegrityMismatch",
            "raw_reason_accessor_allowed": False,
            "returns_consumed_material": False,
            "forbidden_traits": [
                "Debug",
                "Clone",
                "Copy",
                "Default",
                "From",
                "Into",
                "Serialize",
                "Deserialize",
            ],
        },
        "Stage5C materialization terminal type drift",
    )
    require_exact(
        material["material_visibility"],
        "pub_crate_opaque_private_fields",
        "Stage5C material fields widened",
    )
    require_exact(material["material_constructor_count"], 1, "Stage5C material constructor count drift")
    require_exact(material["material_consumer_count"], 1, "Stage5C material consumer count drift")
    require_exact(
        material["material_fields"],
        [
            "strategy",
            "recovery_receipt",
            "callback_input",
            "attribution_snapshot",
            "retained_bar_metadata",
        ],
        "Stage5C callback material schema drift",
    )
    for forbidden in (
        "raw_admission_getter",
        "raw_semantic_bar_getter",
        "raw_cleanup_ledger_getter",
        "alternate_builder",
        "duplicated_stage5c_context_algorithm",
        "duplicated_stage5c_attribution_algorithm",
    ):
        if forbidden not in material["forbidden_surfaces"]:
            fail(f"Stage5C callback material surface weakened: {forbidden}")

    execution = inventory["stage5c_material_callback_execution_contract"]
    require_exact(
        execution["method"],
        "Stage5eStage5cAuthorizedCallbackMaterial::invoke_authorized_callback_once",
        "material callback consumer drift",
    )
    require_exact(execution["seal_constructor_count"], 1, "callback execution seal constructor drift")
    require_exact(execution["method_call_site_count"], 1, "material callback consumer count drift")
    require_exact(execution["callback_count"], 1, "material callback cardinality drift")
    require_exact(
        execution["callback_location"],
        "inside_stage5c_paper_host_material_consumer",
        "callback privacy owner drift",
    )
    require_exact(
        execution["output"],
        "Stage5eStage5cPostCallbackMaterial",
        "post-callback material type drift",
    )
    require_exact(
        execution["callback_error_returns_post_callback_material"],
        True,
        "callback error ownership lost",
    )
    require_exact(execution["panic_returns_reusable_input"], False, "callback panic reuse opened")
    require_exact(
        execution["legacy_stage5c_apply_or_loop_allowed"],
        False,
        "legacy Stage5C callback route opened",
    )

    post_material = inventory["stage5c_post_callback_material_contract"]
    require_exact(
        post_material["fields"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "attribution_snapshot",
            "retained_bar_metadata",
            "callback_outcome",
        ],
        "post-callback material schema drift",
    )
    require_exact(post_material["constructor_count"], 1, "post-callback constructor count drift")
    require_exact(post_material["consumer_count"], 1, "post-callback consumer count drift")
    for forbidden in (
        "Debug",
        "Clone",
        "Copy",
        "Default",
        "From",
        "Into",
        "Serialize",
        "Deserialize",
        "raw_getters",
        "generic_into_parts",
        "alternate_constructor",
        "callback_retry",
        "second_callback_consumer",
        "legacy_route_conversion",
    ):
        if forbidden not in post_material["forbidden_surfaces"]:
            fail(f"post-callback material surface weakened: {forbidden}")

    retained = inventory["retained_bar_metadata_contract"]
    require_exact(
        retained["fields"],
        [
            "accepted_bar_close_ts",
            "accepted_bar_origin",
            "execution_eligible",
            "accepted_semantic_bar_identity",
        ],
        "retained accepted-bar metadata drift",
    )
    require_exact(retained["accepted_bar_origin"], "Live", "retained bar origin drift")
    require_exact(retained["execution_eligible"], True, "retained execution eligibility drift")
    require_exact(
        retained["reconstruction_after_callback_allowed"],
        False,
        "post-callback bar metadata reconstruction opened",
    )

    audit = inventory["audit_lineage_contract"]
    require_exact(audit["owner"], "callback_authority", "audit lineage owner drift")
    require_exact(
        audit["visibility"],
        "pub_crate_opaque_private_fields",
        "audit lineage fields widened",
    )
    require_exact(
        audit["fields"],
        [
            "schedule_projection_and_selected_window_identity",
            "sequence_classification_and_optional_boundary_fingerprint",
            "sequence_identity_observed_at_and_expires_at",
            "b3b_event_key_and_effective_chronology",
            "b3c_continuation_binding_bound_at_and_effective_chronology",
            "callback_authority_id_issued_at_and_exact_expiry",
            "accepted_semantic_bar_identity_and_full_instrument_id",
        ],
        "audit lineage field vector drift",
    )
    require_exact(
        audit["constructor"],
        "construct_stage5e_authorized_callback_audit_lineage",
        "audit lineage constructor drift",
    )
    require_exact(audit["constructor_definition_count"], 1, "audit lineage constructor count drift")
    require_exact(audit["constructor_call_site_count"], 1, "audit lineage constructor call-site drift")
    require_exact(
        audit["constructor_capability"],
        "&Stage5eB3eNestedConsumeSeal",
        "audit lineage constructor capability drift",
    )
    require_exact(
        audit["source_authority_material"],
        "exact_B3C_destructured_scalar_vector",
        "audit lineage authority source drift",
    )
    require_exact(audit["nested_material_destructure_owner"], "b3c_evidence", "nested material owner drift")
    require_exact(audit["nested_material_destructure_count"], 1, "nested material destructure count drift")
    require_exact(
        audit["constructor_scalar_arguments"],
        [
            "callback_authority_id",
            "issued_at",
            "effective_observed_at",
            "authority_expires_at",
            "full_instrument_id",
            "accepted_semantic_bar_identity",
            "b3b_event_key_fingerprint",
            "b3c_continuation_binding_id",
            "sequence_identity_fingerprint",
        ],
        "audit lineage scalar vector drift",
    )
    require_exact(
        audit["nested_to_audit_bridge"],
        "b3c_evidence::construct_audit_lineage_from_consumed_nested_material",
        "nested-to-audit bridge drift",
    )
    require_exact(audit["nested_to_audit_bridge_count"], 1, "nested-to-audit bridge count drift")
    require_exact(
        audit["nested_to_audit_bridge_output"],
        "Stage5eAuthorizedCallbackAuditLineage",
        "nested-to-audit bridge output drift",
    )
    require_exact(audit["nested_to_audit_bridge_raw_getters_allowed"], False, "audit raw getter opened")
    require_exact(
        audit["nested_to_audit_bridge_second_consumer_allowed"],
        False,
        "second nested-to-audit consumer opened",
    )
    require_exact(
        audit["field_transfer_matrix"],
        [
            {"source": "callback_now", "destination": "stage5c_materialization_and_payload_callback_invoked_at"},
            {"source": "callback_authority_id", "destination": "audit_lineage_callback_authority_id_and_payload_equality_proof"},
            {"source": "issued_at", "destination": "audit_lineage_issued_at"},
            {"source": "effective_observed_at", "destination": "audit_lineage_effective_observed_at"},
            {"source": "authority_expires_at", "destination": "audit_lineage_exact_authority_expiry"},
            {"source": "full_instrument_id", "destination": "audit_lineage_full_instrument_id"},
            {"source": "accepted_semantic_bar_identity", "destination": "audit_lineage_accepted_semantic_bar_identity"},
            {"source": "b3b_event_key_fingerprint", "destination": "audit_lineage_b3b_event_key_equality_binding"},
            {"source": "b3c_continuation_binding_id", "destination": "audit_lineage_b3c_continuation_equality_binding"},
            {"source": "sequence_identity_fingerprint", "destination": "audit_lineage_sequence_identity_equality_binding"},
        ],
        "nested field transfer matrix drift",
    )
    require_exact(
        audit["sole_destination"],
        "Stage5eAuthorizedPaperCallbackPayload",
        "audit lineage destination drift",
    )
    require_exact(
        audit["outer_authority_sources"],
        {
            "callback_authority_id": "invocation_consume_context_callback_authority_id",
            "issued_at": "invocation_consume_context_issued_at",
            "effective_observed_at": "invocation_consume_context_effective_observed_at",
            "authority_expires_at": "invocation_consume_context_authority_expires_at",
        },
        "outer authority metadata lineage transport drift",
    )
    require_exact(audit["second_constructor_allowed"], False, "second audit lineage constructor opened")
    require_exact(audit["alternate_destination_allowed"], False, "alternate audit lineage destination opened")

    context = inventory["canonical_callback_input_contract"]
    require_exact(
        context["type"],
        "HybridRuntimeCallbackInput<HybridRuntimeBarEvent>",
        "canonical callback input type drift",
    )
    require_exact(
        context["builder"],
        "sole_stage5c_consume_stage5c_for_authorized_callback_bridge",
        "callback-input builder ownership drift",
    )
    require_exact(
        context["context_fields"],
        [
            {"field": "strategy_id", "source": "accepted_stage5c_admission_strategy_id"},
            {
                "field": "request_namespace_account",
                "source": "accepted_stage5c_admission_account_id",
            },
            {
                "field": "instrument",
                "source": "accepted_stage5c_admission_target_instrument",
            },
            {"field": "tick_size", "source": "accepted_stage5c_admission_tick_size"},
            {"field": "trade_mode", "source": "constant_HybridRuntimeTradeMode_Paper"},
            {
                "field": "paper_execution_mode",
                "source": "constant_HybridRuntimePaperExecutionMode_LiveOnly",
            },
            {"field": "allow_live_orders", "source": "constant_false"},
            {
                "field": "gateway_phase",
                "source": "constant_HybridRuntimeGatewayPhase_LiveReady",
            },
            {
                "field": "position_qty",
                "source": "Some_pre_callback_strategy_stage5c_current_position_qty",
            },
            {
                "field": "event_ts_utc",
                "source": "exact_accepted_semantic_bar_close_time_utc",
            },
            {
                "field": "strategy_now_ts_utc",
                "source": "callback_production_clock_timestamp",
            },
            {
                "field": "last_bar_ts_utc",
                "source": "Some_exact_accepted_semantic_bar_close_time_utc",
            },
        ],
        "canonical callback context vector drift",
    )
    require_exact(
        context["payload_source"],
        "exact_owned_stage5c_accepted_semantic_bar_moved_once",
        "accepted callback bar source drift",
    )
    require_exact(context["payload_origin"], "Live", "callback bar origin drift")
    require_exact(context["payload_timeframe_sec"], 600, "callback timeframe drift")
    require_exact(context["payload_final"], True, "callback finality drift")
    for field in (
        "caller_context_allowed",
        "payload_clone_or_reconstruction_allowed",
        "position_read_after_callback_allowed",
    ):
        require_exact(context[field], False, f"callback input discretion opened: {field}")

    attribution = inventory["pre_callback_attribution_snapshot_contract"]
    require_exact(
        attribution["source_state"],
        "exact_pre_callback_strategy_state",
        "pre-callback attribution source drift",
    )
    require_exact(
        attribution["algorithm"],
        "accepted_stage5cj_cleanup_attribution_ledger",
        "attribution algorithm drift",
    )
    require_exact(
        attribution["bindings"],
        [
            "accepted_strategy_id",
            "accepted_account_id",
            "accepted_target_instrument",
            "accepted_semantic_bar_identity",
            "accepted_bar_close_timestamp",
        ],
        "attribution binding vector drift",
    )
    for field in (
        "post_callback_state_substitution_allowed",
        "serialization_allowed",
        "clone_allowed",
        "raw_getters_allowed",
    ):
        require_exact(attribution[field], False, f"attribution snapshot surface opened: {field}")

    terminal = inventory["terminal_block_contract"]
    require_exact(
        terminal["reasons"],
        [
            "ClockBeforeAuthorityIssue",
            "AuthorityExpired",
            "AcceptedBarObservedInFuture",
            "InvalidAuthorityChronology",
            "InstrumentIdentityMissing",
            "OwnedIdentityMismatch",
            "CallbackAuthorityIdMismatch",
            "MaterializationIntegrityMismatch",
        ],
        "terminal reason taxonomy drift",
    )
    require_exact(terminal["reason_visibility"], "redacted_caller_visible_enum", "terminal reason visibility drift")
    require_exact(
        terminal["materialization_mapping"],
        {
            "model": "unified_top_level_terminal_reason",
            "source": "Stage5eStage5cMaterializationTerminalBlock",
            "destination": "Stage5eCallbackInvocationTerminalBlock(MaterializationIntegrityMismatch)",
            "mapper": "map_stage5c_materialization_terminal_to_callback_terminal",
            "mapper_owner": "callback_authority",
            "mapper_visibility": "pub_crate_nested_capability_gated",
            "mapper_capability": "&Stage5eB3eNestedConsumeSeal",
            "mapper_definition_count": 1,
            "mapper_call_site_count": 1,
            "mapper_call_site_owner": "b3c_evidence",
            "generic_from_allowed": False,
            "alternate_mapping_allowed": False,
            "retryable_mapping_allowed": False,
        },
        "materialization terminal mapping drift",
    )
    require_exact(
        terminal["propagation_chain"],
        [
            "stage5c_materialization_Result_Err",
            "b3c_maps_once_to_top_level_terminal",
            "nested_consume_returns_Err_top_level_terminal",
            "authority_consume_propagates_same_Err",
            "top_level_invocation_returns_same_Err",
        ],
        "materialization terminal propagation drift",
    )
    for field in (
        "materialization_terminal_callback_count",
        "materialization_terminal_intent_count",
    ):
        require_exact(terminal[field], 0, f"materialization terminal side effect opened: {field}")
    for field in (
        "materialization_terminal_returns_strategy",
        "materialization_terminal_returns_recovery_receipt",
        "materialization_terminal_returns_authority",
    ):
        require_exact(terminal[field], False, f"materialization terminal ownership returned: {field}")
    for field in (
        "returns_authority_receipt",
        "retry_allowed",
        "refresh_allowed",
        "reconstruction_allowed",
        "unbinding_allowed",
    ):
        require_exact(terminal[field], False, f"terminal authority reuse opened: {field}")

    callback = inventory["callback_execution_contract"]
    require_exact(
        callback["callback"],
        "BrokerNeutralHybridStrategy::on_broker_bar",
        "callback identity drift",
    )
    require_exact(callback["exactly_once_after_preflight"], True, "callback cardinality drift")
    require_exact(callback["uses_legacy_stage5c_apply"], False, "legacy apply route opened")
    require_exact(callback["uses_legacy_stage5c_loop"], False, "legacy loop route opened")
    require_exact(callback["catch_unwind_allowed"], False, "callback unwind retry surface opened")
    require_exact(callback["panic_retry_allowed"], False, "callback panic retry opened")
    require_exact(
        callback["actual_callback_status"],
        "hold_until_separate_implementation_review",
        "actual callback implementation opened",
    )

    outcome = inventory["callback_outcome_contract"]
    require_exact(outcome["owner"], "callback_authority", "callback outcome owner drift")
    require_exact(
        outcome["representation"],
        "pub_crate_opaque_struct_with_private_inner_enum",
        "callback outcome representation drift",
    )
    require_exact(outcome["wrapper_visibility"], "pub_crate_private_fields", "outcome wrapper fields widened")
    require_exact(outcome["inner_type"], "PrivateStage5ePaperCallbackOutcome", "private outcome type drift")
    require_exact(outcome["inner_visibility"], "private_owner_only", "outcome variants exposed")
    require_exact(
        outcome["variants"],
        [
            "Ok(Vec<BrokerNeutralHybridIntent>)",
            "ValidationError(HybridRuntimeCallbackValidationError)",
        ],
        "callback outcome variants drift",
    )
    require_exact(
        outcome["source"],
        "move_exact_BrokerNeutralHybridCallbackResult",
        "callback outcome source drift",
    )
    require_exact(
        outcome["move_constructor"],
        "move_stage5e_paper_callback_outcome",
        "callback outcome move constructor drift",
    )
    require_exact(outcome["move_constructor_count"], 1, "callback outcome constructor count drift")
    require_exact(outcome["move_constructor_call_site_count"], 1, "callback outcome call-site count drift")
    require_exact(
        outcome["move_constructor_capability"],
        "&Stage5cB3eCallbackExecutionSeal",
        "callback outcome execution capability drift",
    )
    require_exact(
        outcome["future_inspection_method"],
        "Stage5ePaperCallbackOutcome::consume_for_settlement",
        "future outcome inspection seam drift",
    )
    require_exact(
        outcome["future_inspection_seal"],
        "Stage5ePaperCallbackOutcomeInspectionSeal",
        "future outcome inspection seal drift",
    )
    require_exact(
        outcome["future_inspection_seal_constructible_in_b3e"],
        False,
        "outcome inspection opened in B3E",
    )
    require_exact(outcome["external_variant_construction_allowed"], False, "external outcome construction opened")
    require_exact(outcome["external_variant_inspection_allowed"], False, "external outcome inspection opened")
    require_exact(outcome["intent_vector_owner_count"], 1, "intent ownership duplicated")
    require_exact(outcome["intent_clone_allowed"], False, "intent clone opened")
    require_exact(
        outcome["second_result_representation_allowed"],
        False,
        "second callback result representation opened",
    )
    require_exact(
        outcome["pre_settlement_queue_or_persistence_allowed"],
        False,
        "pre-settlement queue/persistence opened",
    )
    require_exact(
        outcome["forbidden_traits"],
        ["Debug", "Clone", "Copy", "Default", "From", "Into", "Serialize", "Deserialize"],
        "callback outcome trait freeze drift",
    )

    require_exact(
        inventory["payload_to_escrow_transfer_matrix"],
        [
            {"input": "material.strategy", "callback_use": "mutable_callback_receiver", "escrow_destination": "mutated_strategy"},
            {"input": "material.recovery_receipt", "callback_use": "untouched", "escrow_destination": "recovery_receipt"},
            {"input": "material.callback_input.payload", "callback_use": "moved_once_into_callback", "escrow_destination": "facts_retained_in_retained_bar_metadata"},
            {"input": "material.attribution_snapshot", "callback_use": "untouched", "escrow_destination": "attribution_snapshot"},
            {"input": "material.retained_bar_metadata", "callback_use": "untouched", "escrow_destination": "exact_accepted_bar_settlement_fields"},
            {"input": "audit_lineage", "callback_use": "untouched", "escrow_destination": "audit_lineage"},
            {"input": "callback_now", "callback_use": "callback_production_clock", "escrow_destination": "callback_invoked_at"},
            {"input": "owned_callback_authority_id", "callback_use": "callback_time_equality_proof", "escrow_destination": "callback_authority_id"},
            {"input": "exact_callback_result", "callback_use": "converted_by_move_once", "escrow_destination": "exactly_one_stage5e_paper_callback_outcome"},
        ],
        "payload-to-escrow transfer matrix drift",
    )

    construction = inventory["escrow_construction_contract"]
    require_exact(
        construction["escrow_owner"],
        "strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority",
        "escrow owner drift",
    )
    require_exact(construction["seal"], "Stage5eEscrowConstructionSeal", "escrow seal drift")
    require_exact(construction["seal_constructor_count"], 1, "escrow seal constructor count drift")
    require_exact(
        construction["seal_construction_time"],
        "after_authorized_post_callback_payload_returned",
        "escrow seal constructed before callback",
    )
    require_exact(
        construction["post_callback_consume_method"],
        "Stage5eAuthorizedPostCallbackPayload::construct_result_escrow",
        "post-callback escrow bridge drift",
    )
    require_exact(
        construction["post_callback_consume_method_visibility"],
        "private_owner_only",
        "post-callback payload consumer visibility drift",
    )
    require_exact(
        construction["post_callback_consume_call_site_count"],
        1,
        "post-callback escrow bridge call-site count drift",
    )
    require_exact(
        construction["stage5c_sibling_bridge"],
        "Stage5eStage5cPostCallbackMaterial::construct_result_escrow",
        "Stage5C post-callback sibling bridge drift",
    )
    require_exact(
        construction["stage5c_sibling_bridge_visibility"],
        "pub_crate_seal_gated",
        "Stage5C sibling bridge visibility drift",
    )
    require_exact(
        construction["stage5c_sibling_bridge_call_site_count"],
        1,
        "Stage5C sibling bridge call-site count drift",
    )
    require_exact(
        construction["stage5c_sibling_bridge_inputs"],
        [
            "self",
            "Stage5eAuthorizedCallbackAuditLineage",
            "callback_invoked_at",
            "callback_authority_id",
            "Stage5eEscrowConstructionSeal",
        ],
        "Stage5C sibling bridge signature drift",
    )
    require_exact(
        construction["constructor"],
        "construct_stage5e_paper_callback_result_escrow",
        "escrow constructor drift",
    )
    require_exact(construction["constructor_definition_count"], 1, "escrow constructor count drift")
    require_exact(construction["constructor_call_site_count"], 1, "escrow constructor call-site drift")
    for field in (
        "pre_callback_construction_allowed",
        "construction_without_authorized_post_callback_payload_allowed",
        "construction_without_seal_allowed",
    ):
        require_exact(construction[field], False, f"escrow construction guard opened: {field}")

    escrow = inventory["result_escrow_contract"]
    require_exact(
        escrow["implementation_status"],
        "design_only_not_implemented",
        "escrow implementation opened",
    )
    require_exact(
        escrow["callback_error_retains_post_callback_strategy"],
        True,
        "callback-error ownership lost",
    )
    require_exact(
        escrow["callback_error_retry_allowed"],
        False,
        "callback-error retry opened",
    )
    require_exact(
        escrow["owns"],
        [
            "mutated_strategy",
            "recovery_receipt",
            "audit_lineage",
            "attribution_snapshot",
            "accepted_bar_close_ts",
            "accepted_bar_origin_Live",
            "execution_eligible_true",
            "accepted_semantic_bar_identity",
            "callback_invoked_at",
            "callback_authority_id",
            "exactly_one_stage5e_paper_callback_outcome",
        ],
        "result escrow exact ownership drift",
    )
    required_forbidden = {
        "Clone",
        "Serialize",
        "Deserialize",
        "into_parts",
        "raw_strategy_getter",
        "intent_getter",
        "intent_iterator",
        "intent_sink_conversion",
        "command_conversion",
        "send_capable_consumer",
        "execution_ready_marker",
    }
    if not required_forbidden.issubset(set(escrow["forbidden_traits_and_surfaces"])):
        fail("private escrow forbidden-surface set weakened")
    if any(value is not False for value in inventory["closed_surfaces"].values()):
        fail("a design-only closed surface was opened")


def check_accepted_source_is_unchanged() -> None:
    for rel, expected in EXPECTED_B3D_SOURCE_SHA256.items():
        if sha256(ROOT / rel) != expected:
            fail(f"accepted B3D source drift: {rel}")
    runtime_source = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    for forbidden in (
        "fn invoke_stage5e_authorized_paper_callback(",
        "struct Stage5eCallbackInvocationSeal",
        "struct Stage5eCallbackInvocationPreflight",
        "struct Stage5ePaperCallbackResultEscrow",
        "struct Stage5eCallbackInvocationTerminalBlock",
    ):
        if forbidden in runtime_source:
            fail(f"design-only stage contains implementation symbol: {forbidden}")


def check_plan_markers() -> None:
    text = PLAN.read_text()
    for marker in (
        "This is the governance-only B3E-r7 closure",
        "175b172b61e580d4db81aad8182020fabd38e482",
        "93d365ae51f2f6ad94954782a27bc49857fe21ff",
        "invoke_stage5e_authorized_paper_callback",
        "Stage5eCallbackInvocationSeal",
        "Stage5eCallbackInvocationPreflight<'a>",
        "Stage5eB3eNestedPreflightSeal",
        "Stage5cB3eCallbackMaterialSeal",
        "consume_stage5c_for_authorized_callback",
        "Stage5eStage5cAuthorizedCallbackMaterial",
        "Stage5eB3eInvocationConsumeContext",
        "consume_for_nested_b3c",
        "Stage5eB3eNestedInvocationMaterial",
        "construct_nested_invocation_material",
        "construct_stage5e_authorized_paper_callback_payload",
        "Stage5eAuthorizedPostCallbackPayload",
        "invoke_callback_once_in_authority",
        "move_stage5e_paper_callback_outcome",
        "PrivateStage5ePaperCallbackOutcome",
        "Stage5ePaperCallbackOutcomeInspectionSeal",
        "construct_stage5e_authorized_callback_audit_lineage",
        "construct_audit_lineage_from_consumed_nested_material",
        "Stage5eStage5cMaterializationTerminalBlock",
        "MaterializationIntegrityMismatch",
        "map_stage5c_materialization_terminal_to_callback_terminal",
        "issue_stage5c_b3e_callback_material_seal",
        "Stage5cB3eCallbackExecutionSeal",
        "Stage5eStage5cPostCallbackMaterial",
        "invoke_authorized_callback_once",
        "Stage5eEscrowConstructionSeal",
        "construct_stage5e_paper_callback_result_escrow",
        "Exact payload-to-callback-to-escrow transfer",
        "Stage5ePaperCallbackResultEscrow",
        "BrokerNeutralHybridStrategy::on_broker_bar exactly once",
        "HybridRuntimeCallbackInput<HybridRuntimeBarEvent>",
        "Stage5eAuthorizedPaperCallbackPayload",
        "Stage5ePreCallbackAttributionSnapshot",
        "stage5cj_cleanup_attribution_ledger",
        "pub(crate) struct Stage5ePaperCallbackOutcome",
        "authority.effective_observed_at == owned B3C effective_observed_at",
        "A callback validation error remains inside the escrow",
        "Stage4AcceptedPaperHostEvidence → B3C → callback authority",
        "Any implementation requires a separate accepted assignment and review.",
    ):
        if marker not in text:
            fail(f"required design marker missing: {marker}")


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid B3E design contract: {exc}")
    check_inventory(inventory)
    check_accepted_source_is_unchanged()
    check_plan_markers()
    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", BASELINE_REF, "--"],
            cwd=ROOT,
            text=True,
        ).splitlines()
        if sorted(changed) != sorted(EXPECTED_ALLOWED_CHANGED_PATHS):
            fail("B3E design review diff drift")
    print("stage5e-b3e-callback-invocation-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
