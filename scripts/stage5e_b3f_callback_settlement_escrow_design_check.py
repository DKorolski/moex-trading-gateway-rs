#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3f settlement-escrow design."""

from __future__ import annotations

import hashlib
import json
import re
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
BASELINE_REF = "a5ccea08bc64a66e768340f7121e9b94a09ff884"
EXPECTED_PLAN_SHA256 = (
    "a2433e5fd18eeae5a58e51fb1c54697e4ac4a282251ea2509a11149acca29a8b"
)
EXPECTED_INVENTORY_SHA256 = (
    "f16333f076cfdd15cc2be29227c3752886fd2e1221c00350e779d1540847ad72"
)
EXPECTED_PROTECTED_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "d7458cc5acb0004c9a82eb42675ca7a3672f7c584cd686a1ddaa0b72d8035e41"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "75e3e30deff70fd58f740361395bb82c32981bd6107831dfb21ff037591c6b7d"
    ),
}
EXPECTED_IMPLEMENTATION_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/lib.rs": (
        "4a248db1a97799604bcfcb094abd1b22abebc98aec67882c829e1fa5a884e7ae"
    ),
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "0fce95557b2e7673d7e7e74a5b4d65dd3ec28360fab3674c20e3e6de6be02ff3"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "34ed25d3ee188d3f0c52d4b655c6105349e9761b7bd3a5af934e52cab14fb2d6"
    ),
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3f-callback-settlement-escrow-design.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5d_additive_freeze_check.py",
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


def require_source_fragment(source: str, fragment: str, message: str) -> None:
    if fragment not in source:
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


def accepted_git_blob_sha256(relative: str) -> str:
    completed = subprocess.run(
        ["git", "show", f"{BASELINE_REF}:{relative}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    )
    return hashlib.sha256(completed.stdout).hexdigest()


def marked_region(source: str, label: str, marker_id: str) -> str:
    begin = f"// {label}-BEGIN: {marker_id}"
    end = f"// {label}-END: {marker_id}"
    require_exact(source.count(begin), 1, f"{label} begin marker drift")
    require_exact(source.count(end), 1, f"{label} end marker drift")
    start = source.index(begin) + len(begin)
    finish = source.index(end, start)
    region = source[start:finish]
    if not region.strip():
        fail(f"{label} implementation region is empty")
    return region


def validate_implementation_source() -> None:
    lib = (ROOT / "crates/strategy-runtime-core/src/lib.rs").read_text()
    stage5c = (
        ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    ).read_text()
    stage5e = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    stage5c_region = marked_region(
        stage5c,
        "STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION",
        "stage5c-private-bridge-v1",
    )
    stage5e_region = marked_region(
        stage5e,
        "STAGE5E-B3F-SETTLEMENT-IMPLEMENTATION",
        "private-process-local-v1",
    )
    combined = stage5c_region + stage5e_region
    for forbidden in (
        "redis::",
        "reqwest",
        "finam",
        ".send(",
        "tokio::spawn",
        "std::fs",
        "runtime_live",
        "broker_execution",
    ):
        if forbidden.lower() in combined.lower():
            fail(f"forbidden implementation surface opened: {forbidden}")
    for symbol in (
        "validate_and_settle_stage5e_paper_callback_escrow",
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "construct_stage5e_b3f_stage5c_expected_preflight_binding",
        "settle_stage5e_callback_escrow_material",
        "settle_stage5c_semantic_result_owning_core",
        "map_stage5c_preflight_mismatch_exact",
        "map_stage5c_settlement_error_exact",
        "construct_stage5e_b3f_settlement_identity",
    ):
        if symbol not in combined:
            fail(f"required B3F implementation symbol missing: {symbol}")
    mismatch_match = re.search(
        r"pub\(crate\) enum Stage5eStage5cPreflightMismatch\s*\{(?P<body>.*?)\n\}",
        stage5c_region,
        re.S,
    )
    if mismatch_match is None:
        fail("Stage 5C mismatch enum missing")
    mismatch_prefix = stage5c_region[: mismatch_match.start()]
    if mismatch_prefix.rstrip().endswith("]"):
        fail("Stage 5C mismatch enum gained an attribute or derive")
    for forbidden_trait in (
        "Clone",
        "Copy",
        "Serialize",
        "Deserialize",
        "Display",
        "From",
        "Into",
    ):
        if re.search(
            rf"impl(?:<[^>]+>)?\s+{forbidden_trait}\s+for\s+"
            r"Stage5eStage5cPreflightMismatch",
            combined,
        ):
            fail(f"Stage 5C mismatch gained forbidden trait: {forbidden_trait}")
    if "drop(exact_intent_vector);" not in stage5c_region:
        fail("early-attribution intent vector is no longer irreversibly disposed")
    if "fn accepted_bar_close_ts(" in stage5c:
        fail("production raw retained-close getter opened")
    if ".accepted_bar_close_ts()" in stage5e:
        fail("settlement chronology regressed to raw scalar extraction")
    for retained_close_fragment in (
        "pub(crate) fn validate_stage5e_b3f_retained_close_chronology(",
        "DateTime::from_timestamp(retained_bar_metadata.accepted_bar_close_ts, 0)",
        "accepted_bar_close > authority_issued_at",
        "accepted_bar_close > callback_invoked_at",
        "Stage5eStage5cRetainedCloseChronologyProof(())",
        "Stage5eStage5cRetainedCloseChronologyMismatch(())",
    ):
        require_source_fragment(
            stage5c_region,
            retained_close_fragment,
            f"Stage 5C retained-close authority missing: {retained_close_fragment}",
        )
    require_exact(
        (stage5c + stage5e).count("validate_stage5e_b3f_retained_close_chronology("),
        2,
        "retained-close chronology authority must have one definition and one call site",
    )
    for chronology_fragment in (
        "audit._b3c_effective_observed_at == audit._effective_observed_at",
        "audit._b3c_effective_expires_at == audit._authority_expires_at",
    ):
        require_source_fragment(
            stage5e_region,
            chronology_fragment,
            f"settlement chronology relation missing: {chronology_fragment}",
        )
    for authority_fragment in (
        "let recomputed = super::callback_authority_id(",
        "recomputed.0 == escrow.callback_authority_id",
        "recomputed.0 == audit._callback_authority_id",
    ):
        require_source_fragment(
            stage5e_region,
            authority_fragment,
            f"canonical callback-authority recomputation missing: {authority_fragment}",
        )
    if combined.count("super::callback_authority_id(") != 1:
        fail("callback-authority encoder must have exactly one B3F call site")
    compile_fail_fixtures = (
        "b3f_compile_fail_consume_seal_clone_or_copy",
        "b3f_compile_fail_consume_seal_reconstruction",
        "b3f_compile_fail_capability_escape",
        "b3f_compile_fail_second_escrow_consume",
        "b3f_compile_fail_borrow_survives_consume",
    )
    for compile_fail_fixture in compile_fail_fixtures:
        if lib.count(compile_fail_fixture) != 1:
            fail(f"required B3F compile-fail fixture missing: {compile_fail_fixture}")
    for diagnostic_fence in (
        "```compile_fail,E0599\n//! // b3f_compile_fail_consume_seal_clone_or_copy",
        "```compile_fail,E0423\n//! // b3f_compile_fail_consume_seal_reconstruction",
        "```compile_fail,E0599\n//! // b3f_compile_fail_capability_escape",
        "```compile_fail,E0382\n//! // b3f_compile_fail_second_escrow_consume",
        "```compile_fail,E0505\n//! // b3f_compile_fail_borrow_survives_consume",
    ):
        require_source_fragment(
            lib,
            diagnostic_fence,
            f"compile-fail diagnostic class drift: {diagnostic_fence}",
        )
    if "compile_error!" in lib:
        fail("B3F compile-fail evidence may not use unconditional compile_error")
    for facade_fragment in (
        "#[cfg(doctest)]\n#[doc(hidden)]\npub mod stage5e_b3f_compile_fail_facade",
        "type ProductionEscrow = callback_authority::Stage5ePaperCallbackResultEscrow;",
        "Stage5ePaperSettlementPreflightSeal;",
        "Stage5ePaperSettlementConsumeSeal;",
        "b3f_doctest_borrow_preflight(&self.0, &seal.0);",
        "b3f_doctest_consume_escrow(self.0, &seal.0);",
        "let _first = escrow.consume(&seal);",
        "let _second = escrow.consume(&seal);",
        "let borrowed = escrow.preflight(&preflight_seal);",
        "let _payload = escrow.consume(&consume_seal);",
    ):
        require_source_fragment(
            lib,
            facade_fragment,
            f"production-backed compile-fail facade drift: {facade_fragment}",
        )
    for helper in (
        "b3f_doctest_borrow_preflight",
        "b3f_doctest_consume_escrow",
    ):
        if not re.search(rf"#\[cfg\(doctest\)\]\s+pub\(crate\) fn {helper}", stage5e_region):
            fail(f"compile-fail production delegate is not doctest-only: {helper}")
    for test_name in (
        "b3f_owning_core_matches_legacy_public_zero_intent_settlement",
        "b3f_canonical_zero_intent_escrow_settles_once_with_one_entry_history",
        "b3f_source_produced_intent_preserves_ordered_request_ids_and_exact_count",
        "b3f_callback_validation_error_consumes_escrow_into_terminal_receipt",
        "b3f_intent_capacity_boundary_is_exact_at_255_and_256",
        "b3f_preflight_mismatch_mapper_is_exact_for_all_nine_variants",
        "b3f_stage5c_error_mapper_is_exact_for_all_twelve_variants",
        "b3f_event_key_validator_rejects_every_frozen_source_drift",
        "b3f_settlement_identity_preserves_request_order_and_chronology",
        "b3f_stage5c_preflight_validator_produces_all_nine_exact_mismatches",
        "b3f_callback_before_retained_close_is_terminal_chronology_mismatch",
        "b3f_retained_close_after_authority_issue_is_terminal_chronology_mismatch",
        "b3f_b3c_outer_chronology_drift_is_terminal_chronology_mismatch",
        "b3f_same_wrong_stored_authority_ids_fail_canonical_recomputation",
        "b3f_canonical_authority_input_drift_without_new_id_is_identity_mismatch",
        "b3f_early_attribution_error_disposes_exact_intent_vector",
        "b3f_owning_core_matches_legacy_public_nonempty_settlement",
        "b3f_owning_core_matches_legacy_public_representative_error",
    ):
        if test_name not in stage5c + stage5e:
            fail(f"required B3F acceptance test missing: {test_name}")


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
        "private_process_local_implementation_pending_review",
        "implementation status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "baseline drift")
    require_exact(
        inventory.get("expected_provenance_case_count"),
        518,
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

    if (ROOT / ".git").exists():
        for relative, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
            require_exact(
                accepted_git_blob_sha256(relative),
                expected,
                f"accepted B3E predecessor source drift: {relative}",
            )
    require_exact(
        inventory.get("protected_b3e_source_sha256"),
        EXPECTED_PROTECTED_SOURCE_SHA256,
        "protected B3E source inventory drift",
    )
    for relative, expected in EXPECTED_IMPLEMENTATION_SOURCE_SHA256.items():
        require_exact(
            sha256(ROOT / relative),
            expected,
            f"protected B3F implementation source changed: {relative}",
        )
    require_exact(
        inventory.get("implementation_source_sha256"),
        EXPECTED_IMPLEMENTATION_SOURCE_SHA256,
        "protected B3F implementation source inventory drift",
    )
    validate_implementation_source()
    require_exact(
        inventory["module_visibility_contract"],
        {
            "callback_settlement_module": "pub_crate_child_module_named_opaque_surface_only",
            "schedule_window_evidence_module": "pub_crate_existing_module",
            "public_outside_crate_surface_allowed": False,
            "unlisted_cross_module_reexports_allowed": False,
        },
        "cross-module path visibility drift",
    )

    transition = inventory["transition_contract"]
    require_exact(
        transition["only_input"],
        "Stage5ePaperCallbackResultEscrow",
        "settlement sole-input drift",
    )
    require_exact(
        transition["implementation_status"],
        "implemented_private_process_local_pending_review",
        "settlement implementation status drift",
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
    require_exact(
        transition["settlement_implementation_allowed_in_this_stage"],
        True,
        "accepted private settlement implementation was closed",
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
    require_exact(
        inventory["retained_close_chronology_authority_contract"],
        {
            "function": "validate_stage5e_b3f_retained_close_chronology",
            "owner": "strategy_runtime_core::stage5c_paper_host",
            "definition_count": 1,
            "call_site_count": 1,
            "ownership": "immutable_borrows_only",
            "inputs": [
                "&Stage5eAcceptedBarSettlementMetadata",
                "authority_issued_at",
                "callback_invoked_at",
                "&Stage5ePaperSettlementPreflightSeal",
            ],
            "success": "Stage5eStage5cRetainedCloseChronologyProof",
            "failure": "Stage5eStage5cRetainedCloseChronologyMismatch",
            "success_and_failure": "pub_crate_opaque_payload_free",
            "raw_timestamp_return_allowed": False,
            "production_raw_getters_allowed": False,
            "proof_reusable_authority": False,
            "mismatch_maps_to": "ChronologyMismatch",
            "borrows_end_before_escrow_consume": True,
        },
        "retained-close chronology authority contract drift",
    )
    require_exact(
        inventory["compile_fail_evidence_contract"],
        {
            "configuration": "cfg_doctest_only",
            "normal_runtime_public_surface_added": False,
            "facade_wraps_actual_production_escrow_and_seals": True,
            "facade_operations_delegate_to_actual_production_borrow_and_consume": True,
            "unconditional_compile_error_allowed": False,
            "expected_diagnostic_codes": {
                "consume_seal_clone_or_copy": "E0599",
                "consume_seal_reconstruction": "E0423",
                "capability_escape": "E0599",
                "second_escrow_consume": "E0382",
                "preflight_borrow_survives_escrow_consume": "E0505",
            },
            "cases": [
                "consume_seal_clone_or_copy",
                "consume_seal_reconstruction",
                "capability_escape",
                "second_escrow_consume",
                "preflight_borrow_survives_escrow_consume",
            ],
        },
        "production-backed compile-fail evidence contract drift",
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
            "&Stage5eB3fStage5cExpectedPreflightBinding<'_>",
            "&Stage5ePaperSettlementPreflightSeal",
        ],
        "Stage 5C preflight input topology drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor"],
        "construct_stage5e_b3f_stage5c_expected_preflight_binding",
        "expected-binding constructor drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "expected-binding reverse-sibling privacy drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "expected-binding constructor owner drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_signature"],
        "eight_lifetime_bound_immutable_audit_field_borrows,&Stage5ePaperSettlementPreflightSeal->Stage5eB3fStage5cExpectedPreflightBinding<'a>",
        "expected-binding constructor signature drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_visibility"],
        "pub_crate_opaque_stage5c_private_fields",
        "expected-binding field visibility drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_lifetime_bound"],
        True,
        "expected-binding lifetime drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_constructor_input_mode"],
        "immutable_borrow_field_for_field_from_stage5e_call_site",
        "expected-binding constructor direction drift",
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
        stage5c_preflight["expected_binding_constructor_inputs"],
        [
            "audit_schedule_identity_fingerprint",
            "audit_sequence_identity_fingerprint",
            "audit_event_key_fingerprint",
            "audit_b3b_event_key_fingerprint",
            "audit_full_instrument_id",
            "audit_owned_instrument",
            "audit_accepted_semantic_bar_identity",
            "audit_owned_bar_identity",
            "Stage5ePaperSettlementPreflightSeal",
        ],
        "expected-binding constructor input vector drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_fields_inspected_only_by_stage5c_owner"],
        True,
        "expected-binding private field inspector drift",
    )
    require_exact(
        stage5c_preflight["expected_binding_borrows_end_before_escrow_consume"],
        True,
        "expected-binding borrow lifetime escaped consume boundary",
    )

    event_key = inventory["b3b_event_key_validation_authority_contract"]
    require_exact(
        event_key["function"],
        "validate_stage5e_b3f_b3b_event_key_binding",
        "B3B validation authority drift",
    )
    require_exact(
        event_key["owner"],
        "strategy_runtime_core::stage5e_no_io_lifecycle::schedule_window_evidence",
        "B3B validation authority owner drift",
    )
    require_exact(event_key["definition_count"], 1, "B3B authority definition drift")
    require_exact(event_key["call_site_count"], 1, "B3B authority call-site drift")
    require_exact(
        event_key["sole_caller"],
        "validate_stage5e_b3f_stage5c_preflight_binding",
        "B3B authority caller drift",
    )
    require_exact(
        event_key["inputs"],
        [
            "&audit_schedule_identity_fingerprint",
            "&audit_full_instrument_id",
            "retained_bar_close_i64",
            "&audit_sequence_identity_fingerprint",
            "&audit_event_key_fingerprint",
            "&audit_b3b_event_key_fingerprint",
            "&Stage5ePaperSettlementPreflightSeal",
        ],
        "B3B authority exact input vector drift",
    )
    require_exact(
        event_key["visibility"],
        "pub_crate_capability_gated",
        "B3B authority visibility drift",
    )
    require_exact(
        event_key["success"],
        "Stage5eB3fEventKeyValidatedProof",
        "B3B authority success type drift",
    )
    require_exact(
        event_key["failure"],
        "Stage5eB3fEventKeyMismatch",
        "B3B authority failure type drift",
    )
    require_exact(
        event_key["canonical_delegate"],
        "schedule_window_evidence::b3b_event_key_fingerprint",
        "canonical B3B encoder delegation drift",
    )
    require_exact(event_key["canonical_delegate_call_count"], 1, "canonical B3B delegate count drift")
    for key in (
        "second_encoder_allowed",
        "second_domain_string_allowed",
        "raw_audit_lineage_getters_allowed",
        "mutable_inputs_allowed",
        "proof_reusable_authority",
    ):
        require_exact(event_key[key], False, f"B3B authority surface opened: {key}")
    require_exact(event_key["proof_fields"], [], "B3B proof gained reusable material")
    require_exact(
        event_key["success_visibility"],
        "pub_crate_opaque_payload_free",
        "B3B proof visibility drift",
    )
    require_exact(
        event_key["failure_visibility"],
        "pub_crate_opaque_payload_free",
        "B3B mismatch visibility drift",
    )
    require_exact(
        event_key["mismatch_maps_to"],
        "Stage5eStage5cPreflightMismatch::AuditEventKey",
        "B3B mismatch mapping drift",
    )

    mismatch = inventory["stage5c_preflight_mismatch_contract"]
    require_exact(
        mismatch["owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "preflight mismatch owner drift",
    )
    require_exact(
        mismatch["representation"],
        "pub_crate_closed_payload_free_enum",
        "preflight mismatch representation drift",
    )
    require_exact(
        mismatch["variants"],
        [
            "StrategyId",
            "AccountId",
            "FullInstrumentId",
            "SemanticBarIdentity",
            "AcceptedBarClose",
            "AuditEventKey",
            "PaperMode",
            "AcceptedBarOrigin",
            "ExecutionEligibility",
        ],
        "preflight mismatch variant drift",
    )
    require_exact(
        mismatch["forbidden_traits"],
        ["Clone", "Copy", "Serialize", "Deserialize", "Display", "From", "Into"],
        "preflight mismatch forbidden-trait contract drift",
    )
    require_exact(
        mismatch["sole_inspector"],
        "map_stage5c_preflight_mismatch_exact",
        "preflight mismatch sole-inspector drift",
    )
    require_exact(mismatch["mapper_definition_count"], 1, "preflight mismatch mapper definition drift")
    require_exact(mismatch["mapper_call_site_count"], 1, "preflight mismatch mapper call-site drift")
    require_exact(
        mismatch["mapper_signature"],
        "Stage5eStage5cPreflightMismatch,&Stage5ePaperSettlementPreflightSeal->Stage5ePaperSettlementTerminalReason",
        "preflight mismatch mapper signature drift",
    )
    require_exact(mismatch["mapper_implementation"], "exhaustive_9_arm_match", "preflight mismatch mapper drift")
    require_exact(mismatch["wildcard_arm_allowed"], False, "preflight mismatch wildcard opened")
    require_exact(mismatch["generic_conversion_allowed"], False, "preflight mismatch conversion opened")
    require_exact(
        mismatch["mapping"],
        {
            "StrategyId": "IdentityMismatch",
            "AccountId": "IdentityMismatch",
            "FullInstrumentId": "IdentityMismatch",
            "SemanticBarIdentity": "IdentityMismatch",
            "AcceptedBarClose": "IdentityMismatch",
            "AuditEventKey": "IdentityMismatch",
            "PaperMode": "PaperModeMismatch",
            "AcceptedBarOrigin": "PaperModeMismatch",
            "ExecutionEligibility": "PaperModeMismatch",
        },
        "preflight mismatch mapping drift",
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
        named["stage5c_expected_binding_builder"]["owner"],
        "strategy_runtime_core::stage5c_paper_host",
        "named expected-binding builder owner drift",
    )
    require_exact(
        named["b3b_event_key_validation_authority"]["name"],
        "validate_stage5e_b3f_b3b_event_key_binding",
        "named B3B event-key authority drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["name"],
        "map_stage5c_preflight_mismatch_exact",
        "named preflight mismatch mapper drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["implementation"],
        "exhaustive_9_arm_match",
        "named preflight mismatch mapper implementation drift",
    )
    require_exact(
        named["stage5c_preflight_mismatch_mapper"]["wildcard_arm_allowed"],
        False,
        "named preflight mismatch wildcard opened",
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
        "escrow_validation_or_settlement",
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
