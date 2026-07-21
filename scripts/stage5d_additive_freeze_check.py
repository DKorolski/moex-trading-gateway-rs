#!/usr/bin/env python3
"""Validate the Stage 5D additive freeze baseline.

The checker is intentionally local and conservative. Stage 5D has a dual
baseline:

* Stage 5C closure baseline: immutable historical public API/source evidence;
* Stage 5D additive baseline: reviewed bridge regions and Stage5d* API.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any

sys.dont_write_bytecode = True


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_REL = Path("docs/stage-5/stage-5d-additive-freeze-manifest.json")
STAGE5C_MANIFEST_REL = Path("docs/stage-5/stage-5c-api-freeze-manifest.json")
STAGE5C_CHECKER_REL = Path("scripts/stage5c_api_freeze_check.py")
STAGE5C_CLOSURE_CHECKER_REL = Path("tests/fixtures/stage5/stage5c_api_freeze_check.closure.py")
LIB_REL = Path("crates/strategy-runtime-core/src/lib.rs")
STAGE5C_HOST_REL = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
WRAPPER_REL = Path("crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs")
STAGE5D_REL = Path("crates/strategy-runtime-core/src/stage5d_persistence.rs")
STAGE5D_BOOTSTRAP_BRIDGE_IDENTIFIER = (
    "stage5d_bootstrap_preserving_loaded_with_validated_working_sets_at"
)
STAGE5D_BOOTSTRAP_BRIDGE_ALLOWED_CALL_FUNCTION = "stage5d_notify_broker_truth_bootstrap_at"
STAGE5D_RISKGATE_BRIDGE_IDENTIFIER = "stage5d_inject_authoritative_riskgate_state"
STAGE5D_RISKGATE_BRIDGE_ALLOWED_CALL_FUNCTION = "stage5d_inject_authoritative_riskgate_with_evidence"
STAGE5D_RUNTIME_RESTORED_BRIDGE_IDENTIFIER = "stage5d_notify_runtime_state_restored_bridge_at"
STAGE5D_RUNTIME_RESTORED_BRIDGE_ALLOWED_CALL_FUNCTION = "stage5d_notify_runtime_state_restored_at"
FORBIDDEN_SCANNER_REL = Path("scripts/forbidden_surface_scan.sh")
CI_REL = Path(".github/workflows/ci.yml")
RUNTIME_RESTORED_OWNERSHIP_REL = Path(
    "docs/stage-5/stage5d-b2bd1-r6-blocker-ownership.json"
)
FINAL_RESTART_INVENTORY_REL = Path(
    "docs/stage-5/stage5d-final-restart-r2-scenario-inventory.json"
)
FINAL_RESTART_R3_INVENTORY_REL = Path(
    "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
)

EXPECTED_MANIFEST_CHECKER = "scripts/stage5d_additive_freeze_check.py"
EXPECTED_NEGATIVE_HARNESS = "scripts/stage5d_additive_freeze_negative_harness.py"
EXPECTED_FORBIDDEN_NEGATIVE_HARNESS_CONTRACT = {
    "launcher_path": "scripts/forbidden_surface_negative_harness.sh",
    "launcher_sha256": "1b4e6b494a7831640201924783d1f1bf7ea3deba0fd9051102b24ae7908dfc36",
    "coordinator_path": "scripts/forbidden_surface_negative_harness.py",
    "coordinator_sha256": "04053bd8c44d41dd229ec806ed5b4083260c33efefecd67a8f18555a653fd245",
    "worker_path": "scripts/forbidden_surface_negative_case_worker.sh",
    "worker_sha256": "c3d33055a4991f14b72da866285cc51f1c99644d2a05a87601e3c12d45a1b852",
    "scanner_contract": "stage5d-b2bc1-r4-v1",
    "declared_cases": 87,
    "negative_cases": 86,
    "positive_controls": 1,
    "default_workers": 4,
    "max_workers": 4,
    "minimum_case_timeout_seconds": 180,
    "ci_timeout_minutes": 75,
}
EXPECTED_STAGE5C_COMPATIBILITY_CHECKER = {
    "path": "scripts/stage5c_api_freeze_check.py",
    "sha256": "2ed629e4e7a157f03b25e55f7b294713855d84a5a9cef3b284d58baa60bc257d",
}
EXPECTED_HISTORICAL_STAGE5C_CHECKER = {
    "path": "tests/fixtures/stage5/stage5c_api_freeze_check.closure.py",
    "sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209",
}

EXPECTED_STAGE5C_CLOSURE = {
    "short_commit": "69cc73b",
    "full_commit": "69cc73b7f33d8cb418c784ac993856d8a487693d",
    "handoff_archive": "moex-trading-project-69cc73b.zip",
    "handoff_sha256": "0b614ebe83b0a8af85cde0ca7a1ae481457813edad72626cd4bb5972c9c83f91",
    "manifest_sha256": "f8c555d11de1271f5041b4d3abf880ac7a406d6fb23f5e4d38ca25468a974323",
    "report_sha256": "1d15c992ce1658fea6d7ec8a25094b094400ba00b764ac23d32c525207d19b48",
    "original_checker_sha256": "e494e92ffb5f8d90b6a581c7b99e4e80f1906aeedfa1e7446d428eb31c757209",
}

EXPECTED_CONTROLLED_SOURCE_SEMANTIC_EXTENSIONS = [
    {
        "path": "crates/strategy-runtime-core/src/hybrid_intraday/mod.rs",
        "stage5c_baseline_sha256": "c70e3847f1a99e00c5d078d19b7b5f103d9b4d26853886b0b47d4805818ac84c",
        "current_sha256": "67224b1523d3eeeae924f10c77cb74582671dc24a5badef554843fe57d079fd1",
        "reason_id": "stage5d-b2bc1-r8-source-owned-codec-crate-private-export",
        "public_api_unchanged": True,
        "approved_region_markers": [],
        "source_correspondence_change_class": "ControlledStage5dSourceOwnedCodec",
        "source_correspondence_path": "crates/strategy-runtime-core/source-correspondence.toml",
        "source_correspondence_sha256": "18a5f7eef690f5886ad9077d0558a41899bbcb261519f59b8208ecd54c94c153",
        "source_codec_owner": "hybrid_intraday/risk_gate.rs",
        "stage5d_consumer_path": "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_consumer_sha256": "ba0be17a6bdbe432a5e626b45fd8e584ce07863401fe907f1662fc30de4adc5b",
    },
    {
        "path": "crates/strategy-runtime-core/src/hybrid_intraday/risk_gate.rs",
        "stage5c_baseline_sha256": "c85779ec5023e602cb6088e116fb58ed0bc80c31828499a0bd4557e2034dee34",
        "current_sha256": "e5b80db163b0d97cfd50b8ad064c076850dbd2c15a95833895f5beb7a66d71a6",
        "reason_id": "stage5d-b2bc1-r8-riskgate-authority-decimal-codec-extension",
        "public_api_unchanged": True,
        "approved_region_markers": [],
        "source_correspondence_change_class": "ControlledStage5dSourceOwnedCodec",
        "source_correspondence_path": "crates/strategy-runtime-core/source-correspondence.toml",
        "source_correspondence_sha256": "18a5f7eef690f5886ad9077d0558a41899bbcb261519f59b8208ecd54c94c153",
        "source_codec_owner": "hybrid_intraday/risk_gate.rs",
        "stage5d_consumer_path": "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "stage5d_consumer_sha256": "ba0be17a6bdbe432a5e626b45fd8e584ce07863401fe907f1662fc30de4adc5b",
    },
]

APPROVED_BRIDGE_FILES = {
    str(LIB_REL): ["lib-stage5d-module", "lib-stage5d-exports"],
    str(STAGE5C_HOST_REL): ["type-state-transitions"],
    str(WRAPPER_REL): ["runtime-private-snapshot"],
}

EXPECTED_CLOSED_SURFACES = {
    "redis": False,
    "finam": False,
    "transport": False,
    "dispatch": False,
    "runtime_live": False,
    "broker_execution": False,
    "runtime_private_mutation": "controlled_validated_stage5d_apply_then_broker_truth_bootstrap_then_riskgate_injection_then_restored_callback_only",
}

EXPECTED_STAGE5C_PRIVATE_LAYOUT_EXTENSIONS = [
    {
        "path": "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "reason_id": "stage5d-b2b-a-persisted-load-provenance-v1",
        "public_api_unchanged": True,
        "stripped_without_additive_regions_sha256": (
            "bcc2c4d6ff08d06c49f9716495ce177fc968a8dcd71f6b2c38bcb8d5b4cb0914"
        ),
    }
]

EXPECTED_NEGATIVE_CASES = [
    "stage5c_api_drift",
    "trading_region_drift",
    "additive_region_escape",
    "public_namespace_leakage",
    "raw_strategy_extractor",
    "missing_historical_baseline",
    "closed_surface_downgrade",
    "negative_cases_removed",
    "manifest_checker_changed",
    "negative_harness_changed",
    "stage5d_symbol_removed",
    "stage5d_symbol_added",
    "current_compat_checker_drift",
    "historical_checker_missing",
    "historical_checker_content_drift",
    "historical_current_checker_substitution",
    "legacy_restore_direct_call",
    "legacy_restore_alias_call",
    "legacy_restore_multiline_call",
    "legacy_restore_function_reference",
    "legacy_restore_qualified_whitespace",
    "legacy_alias_reexport_in_lib_additive_region",
    "legacy_wrapper_in_stage5c_additive_region",
    "legacy_alias_in_stage5d_persistence",
    "unexpected_legacy_reference_in_allowed_file",
    "legacy_reference_moved_to_wrong_region",
    "stage5d_api_surface_drift",
    "private_layout_extension_removed",
    "private_layout_extension_hash_changed",
    "private_layout_extension_additional_path",
    "private_layout_extension_wrapper_path",
    "private_layout_extension_lib_path",
    "private_layout_self_authorized_semantic_drift",
    "private_layout_extension_reason_id_changed",
    "bootstrap_bridge_runtime_compat_direct_call",
    "bootstrap_bridge_runtime_compat_alias_call",
    "bootstrap_bridge_runtime_compat_forwarding_wrapper",
    "bootstrap_bridge_runtime_compat_function_reference",
    "bootstrap_bridge_second_stage5d_call",
    "riskgate_bridge_runtime_compat_direct_call",
    "riskgate_bridge_runtime_compat_alias_call",
    "riskgate_bridge_runtime_compat_forwarding_wrapper",
    "riskgate_bridge_runtime_compat_function_reference",
    "riskgate_bridge_second_stage5d_call",
    "runtime_restored_bridge_runtime_compat_direct_call",
    "runtime_restored_bridge_runtime_compat_alias_call",
    "runtime_restored_bridge_runtime_compat_function_reference",
    "runtime_restored_bridge_second_stage5d_call",
    "runtime_restored_bridge_made_public",
    "runtime_restored_intent_runtime_guard_removed",
    "runtime_restored_intent_guard_after_debug_assert",
    "runtime_restored_post_callback_exact_guard_removed",
    "runtime_restored_callback_count_hook_removed",
    "runtime_restored_post_callback_position_guard_removed",
    "runtime_restored_post_callback_side_guard_removed",
    "runtime_restored_post_callback_protective_guard_removed",
    "runtime_restored_preflight_invocation_removed",
    "runtime_restored_recovery_complete_guard_removed",
    "runtime_restored_pending_finalization_guard_removed",
    "runtime_restored_recovery_plan_binding_guard_removed",
    "runtime_restored_recovery_index_guard_removed",
    "runtime_restored_closed_boundary_guard_removed",
    "runtime_restored_blocked_retained_capability_removed",
    "runtime_restored_terminal_retry_enabled",
    "runtime_restored_lifecycle_notification_guard_removed",
    "runtime_restored_flat_side_exact_guard_removed",
    "runtime_restored_r4_source_prebind_proof_removed",
    "runtime_restored_r4_current_shadow_matrix_removed",
    "runtime_restored_r4_single_row_restored_removed",
    "runtime_restored_r4_multi_row_restored_removed",
    "runtime_restored_r4_actual_long_removed",
    "runtime_restored_r4_actual_short_removed",
    "runtime_restored_r4_known_order_removed",
    "runtime_restored_r4_pending_request_removed",
    "runtime_restored_r4_blocked_fingerprint_removed",
    "runtime_restored_r4_compilefail_private_field_removed",
    "runtime_restored_r4_compilefail_private_bridge_removed",
    "runtime_restored_r4_compilefail_consumed_input_removed",
    "runtime_restored_r5_strict_helper_removed",
    "runtime_restored_r5_known_order_strict_removed",
    "runtime_restored_r5_not_paper_only_blocker_removed",
    "runtime_restored_r5_ownership_table_removed",
    "runtime_restored_r6_strict_long_removed",
    "runtime_restored_r6_strict_short_removed",
    "runtime_restored_r6_strict_known_order_removed",
    "runtime_restored_r6_strict_pending_request_removed",
    "runtime_restored_r6_common_blocked_helper_bypassed",
    "runtime_restored_r6_quantity_ownership_removed",
    "runtime_restored_r6_ownership_stage_changed",
    "runtime_restored_r6_non_ack_decision_removed",
    "runtime_restored_r6_expiry_ownership_removed",
    "runtime_restored_r6_timestamp_ownership_removed",
    "runtime_restored_r6_identity_generation_ownership_removed",
    "runtime_restored_final_canonical_export_removed",
    "runtime_restored_final_restart_matrix_removed",
    "runtime_restored_final_post_export_mutation_removed",
    "runtime_restored_final_recovery_index_binding_removed",
    "runtime_restored_final_package_export_removed",
    "runtime_restored_final_package_decode_removed",
    "runtime_restored_final_package_corruption_removed",
    "runtime_restored_final_clean_process_removed",
    "runtime_restored_final_inventory_missing",
    "runtime_restored_final_inventory_duplicate",
    "runtime_restored_final_r2_positive_matrix_removed",
    "runtime_restored_final_r2_source_callback_removed",
    "runtime_restored_final_r2_crash_store_removed",
    "runtime_restored_final_r2_negative_matrix_removed",
    "runtime_restored_final_r2_golden_vectors_removed",
    "runtime_restored_final_r2_inventory_missing",
    "runtime_restored_final_r2_inventory_reduced",
    "runtime_restored_final_r2_inventory_helper_owner",
    "runtime_restored_final_r2_stage5c_warmup_removed",
    "runtime_restored_final_r2_package_full_validation_removed",
    "final_r3a_reproduction_test_removed",
    "final_r3a_post_apply_private_equality_removed",
    "final_r3a_post_apply_semantic_equality_removed",
    "final_r3a_restored_callback_moved_before_private_apply",
    "final_r3a_mr_long_short_mapping_swapped",
    "final_r3a_bo_reason_mapping_changed",
    "final_r3a_mr_stop_take_dropped",
    "final_r3a_incomplete_mr_accepted",
    "final_r3a_owner_side_reason_mismatch_accepted",
    "final_r3a_unauthorized_set_state_source_change",
    "final_r3_resumption_inventory_removed",
    "final_r3_resumption_r3a_reuse_removed",
    "final_r3_resumption_clean_flat_prematurely_promoted",
    "final_r3_resumption_current_shadow_prematurely_promoted",
    "final_r3_resumption_unapproved_retained_status",
    "final_r3_resumption_nonexistent_owning_test",
    "final_r3_resumption_false_resumption_owner",
    "final_r3_resumption_todo_set_reduced",
    "final_r3_resumption_accepted_r3a_downgraded",
    "final_r3_resumption_stage5e_marker_removed",
    "final_r3_resumption_todo_non_null_owner",
    "final_r3_resumption_accepted_null_owner",
    "final_r3_positive_core_clean_fixture_substituted",
    "final_r3_positive_core_long_direct_mutation_substituted",
    "final_r3_positive_core_short_direct_mutation_substituted",
    "final_r3_positive_core_source_callback_removed",
    "final_r3_positive_core_source_runtime_not_dropped",
    "final_r3_positive_core_strict_decode_removed",
    "final_r3_positive_core_fresh_runtime_removed",
    "final_r3_positive_core_post_apply_equality_removed",
    "final_r3_positive_core_broker_truth_equality_removed",
    "final_r3_positive_core_stage5c_warmup_removed",
    "final_r3_positive_core_current_shadow_todo_promoted",
    "final_r3_positive_core_current_shadow_discovery_removed",
    "final_r3_positive_core_nonexecuting_owner",
    "final_r3_positive_core_stage5e_or_surface_opened",
    "final_r3_current_shadow_long_without_full_path",
    "final_r3_current_shadow_short_without_full_path",
    "final_r3_current_shadow_realized_without_trade_count",
    "final_r3_current_shadow_session_lost",
    "final_r3_current_shadow_pnl_bit_drift",
    "final_r3_current_shadow_signed_zero_accepted",
    "final_r3_current_shadow_evidence_envelope_mismatch",
    "final_r3_current_shadow_materialized_apply_skipped",
    "final_r3_current_shadow_materialized_apply_after_injection",
    "final_r3_current_shadow_callback_before_apply",
    "final_r3_current_shadow_source_runtime_reused",
    "final_r3_current_shadow_direct_mutation_substituted",
    "final_r3_current_shadow_generation_identity_mismatch",
    "final_r3_current_shadow_stage5e_or_surface_opened",
    "final_r3_current_shadow_r1r1_production_boundary_removed",
    "final_r3_current_shadow_r1r1_boundary_cfg_test_only",
    "final_r3_current_shadow_r1r1_raw_envelope_authority",
    "final_r3_current_shadow_r1r1_raw_strategy_extractor",
    "final_r3_current_shadow_r1r1_apply_after_bootstrap",
    "final_r3_current_shadow_r1r1_apply_after_injection",
    "final_r3_current_shadow_r1r1_callback_before_apply",
    "final_r3_current_shadow_r1r1_blocked_loses_capability",
    "final_r3_current_shadow_r1r1_partial_mutation_on_block",
    "final_r3_current_shadow_r1r1_identity_binding_removed",
    "final_r3_current_shadow_r1r1_generation_binding_removed",
    "final_r3_current_shadow_r1r1_ledger_tail_binding_removed",
    "final_r3_current_shadow_r1r1_pnl_binding_removed",
    "final_r3_current_shadow_r1r1_builder_accepts_stale_source",
    "final_r3_current_shadow_r1r1_unrestorable_committed_package",
    "final_r3_current_shadow_r1r1_lifecycle_fields_overwritten",
    "final_r3_current_shadow_r1r1_field_level_proof_removed",
    "final_r3_current_shadow_r1r1_stage5e_or_surface_opened",
    "final_r3_operational_source_callback_removed",
    "final_r3_operational_direct_substitution",
    "final_r3_operational_source_runtime_reused",
    "final_r3_operational_strict_decode_removed",
    "final_r3_operational_private_apply_moved",
    "final_r3_operational_lifecycle_equality_removed",
    "final_r3_operational_partial_timer_removed",
    "final_r3_operational_deferred_entry_stop_take_removed",
    "final_r3_operational_safe_mode_entry_block_removed",
    "final_r3_operational_stage5c_continuation_removed",
    "final_r3_operational_premature_next_group_promotion",
    "final_r3_operational_stage5e_or_surface_opened",
    "final_r3_recovery_index_production_boundary_removed",
    "final_r3_recovery_index_stop_truth_removed",
    "final_r3_recovery_index_negative_matrix_removed",
    "final_r3_recovery_index_pending_field_proof_removed",
    "final_r3_recovery_index_tp_sl_swap_proof_removed",
    "recovery_r1r3_unbroken_path_reconstruction_introduced",
    "recovery_r1r3_authoritative_admission_moved_after_private_apply",
    "recovery_r1r3_production_working_set_call_removed",
    "recovery_r1r3_working_set_coordinator_not_crate_visible",
    "recovery_r1r3_validated_stop_truth_roundtrip_removed",
    "recovery_r1r3_raw_stop_truth_consumed",
    "recovery_r1r3_normalization_call_removed",
    "recovery_r1r3_normalization_block_capability_lost",
    "recovery_r1r3_normalization_partial_mutation_accepted",
    "recovery_r1r3_duplicate_sl_callback_removed",
    "recovery_r1r3_terminal_sl_callback_removed",
    "recovery_r1r3_exact_sl_set_assertion_removed",
    "recovery_r1r3_pending_stage_assertion_removed",
    "recovery_r1r3_pending_terminal_orphan_accepted",
    "recovery_r1r3_stage5c_continuation_removed",
    "recovery_r1r3_final_group_or_stage5e_prematurely_opened",
    "riskrec_single_source_producer_removed",
    "riskrec_runtime_pending_direct_inserted",
    "riskrec_durable_outbox_direct_inserted",
    "riskrec_identity_mismatch_accepted",
    "riskrec_materialized_mismatch_accepted",
    "riskrec_single_action_omitted",
    "riskrec_single_action_duplicated",
    "riskrec_multi_order_reversed",
    "riskrec_multi_set_comparison",
    "riskrec_second_action_before_checkpoint",
    "riskrec_retry_duplicates_ledger_append",
    "riskrec_retry_duplicates_materialized",
    "riskrec_retry_duplicates_runtime_ack",
    "riskrec_final_commit_checkpoint_omitted",
    "riskrec_complete_plan_produces_action",
    "riskrec_complete_plan_changes_state",
    "riskrec_source_runtime_reused",
    "riskrec_strict_decode_removed",
    "riskrec_committed_guard_removed",
    "riskrec_partial_write_accepted",
    "riskrec_full_uncommitted_accepted",
    "riskrec_restored_callback_duplicated",
    "riskrec_stage5c_continuation_removed",
    "riskrec_stage5e_opened",
    "riskrec_r1r1_source_rollover_removed",
    "riskrec_r1r1_runtime_pending_direct",
    "riskrec_r1r1_durable_outbox_direct",
    "riskrec_r1r1_transition_removed",
    "riskrec_r1r1_test_row_executor_substituted",
    "riskrec_r1r1_second_action_selected",
    "riskrec_r1r1_ledger_append_omitted",
    "riskrec_r1r1_ledger_append_duplicated",
    "riskrec_r1r1_materialized_update_omitted",
    "riskrec_r1r1_materialized_update_duplicated",
    "riskrec_r1r1_runtime_ack_omitted",
    "riskrec_r1r1_runtime_ack_duplicated",
    "riskrec_r1r1_final_receipt_omitted",
    "riskrec_r1r1_final_receipt_forged",
    "riskrec_r1r1_checkpoint_package_not_persisted",
    "riskrec_r1r1_store_handles_reused",
    "riskrec_r1r1_partial_file_accepted",
    "riskrec_r1r1_full_uncommitted_accepted",
    "riskrec_r1r1_complete_direct_frontier",
    "riskrec_r1r1_complete_plan_action",
    "riskrec_r1r1_stage5c_warmup_removed",
    "riskrec_r1r1_restored_callback_duplicated",
    "riskrec_r1r1_golden_hash_changed",
    "riskrec_r1r1_stage5e_opened"
]

EXPECTED_STAGE = "5D-final-restart-r3-riskgate-recovery-r1-r2"
EXPECTED_FINAL_RESTART_INVENTORY_STAGE = "5D-final-restart-r2"

EXPECTED_FINAL_RESTART_SCENARIO_IDS = [
    "positive_clean_flat",
    "positive_broker_consistent_open_long",
    "positive_broker_consistent_open_short",
    "positive_pending_entry",
    "positive_partial_entry",
    "positive_pending_exit",
    "positive_deferred_entry",
    "positive_deferred_exit",
    "positive_safe_mode_close_only",
    "positive_non_empty_known_order_index",
    "positive_non_empty_pending_request_index",
    "positive_working_protective_order_hints",
    "positive_already_complete_recovery_plan",
    "positive_current_shadow_long",
    "positive_current_shadow_short",
    "positive_current_shadow_realized_pnl",
    "crash_constructed_no_bytes",
    "crash_truncated_partial_write",
    "crash_full_bytes_no_commit_proof",
    "crash_committed_before_ledger_append",
    "crash_ledger_appended_before_materialized",
    "crash_materialized_before_runtime_ack",
    "crash_runtime_ack_before_final_checkpoint",
    "crash_restart_after_each_recovery_action",
    "crash_replay_after_each_already_applied_action",
    "crash_multi_row_crash_between_rows",
    "negative_outer_unknown_duplicate_malformed",
    "negative_truncated_package",
    "negative_unsupported_package_schema",
    "negative_invalid_uncommitted_checkpoint_state",
    "negative_envelope_checksum_corruption",
    "negative_evidence_checksum_corruption",
    "negative_package_checksum_corruption",
    "negative_envelope_evidence_cross_binding_mismatch",
    "negative_snapshot_revision_generation_mismatch",
    "negative_strategy_account_instrument_config_profile_mismatch",
    "negative_ledger_tail_generation_identity_mismatch",
    "negative_semantic_private_contradiction",
    "negative_recovery_index_mismatch",
    "negative_unexplained_ledger_materialized_runtime_lag",
    "negative_missing_duplicate_outbox_rows",
    "negative_stale_incomplete_contradictory_broker_truth",
    "negative_missing_working_order",
    "negative_unknown_orphan_order_or_trade",
    "negative_protective_hint_while_truth_surface_closed",
    "negative_non_paper_runtime_host_intent_sink_opening",
    "negative_callback_before_recovery_completion",
    "golden_flat",
    "golden_open_long",
    "golden_pending_entry",
    "golden_multi_row_recovery",
]

EXPECTED_FINAL_RESTART_R3_POSITIVE_IDS = [
    "positive_clean_flat",
    "positive_broker_consistent_open_long",
    "positive_broker_consistent_open_short",
    "positive_current_shadow_long",
    "positive_current_shadow_short",
    "positive_current_shadow_realized_pnl",
    "positive_mr_long_bracket_pending_entry",
    "positive_mr_short_bracket_pending_entry",
    "positive_bo_long_market_pending_entry",
    "positive_bo_short_market_pending_entry",
    "positive_partial_entry",
    "positive_pending_exit",
    "positive_deferred_entry",
    "positive_deferred_exit",
    "positive_safe_mode_close_only",
    "positive_non_empty_known_order_index",
    "positive_non_empty_pending_request_index",
    "positive_working_protective_order_hints",
    "positive_single_pending_riskgate_finalization",
    "positive_ordered_multi_row_pending_finalizations",
    "positive_already_complete_recovery_plan",
]

EXPECTED_FINAL_RESTART_R3_ACCEPTED_IDS = [
    "positive_mr_long_bracket_pending_entry",
    "positive_mr_short_bracket_pending_entry",
    "positive_bo_long_market_pending_entry",
    "positive_bo_short_market_pending_entry",
]

EXPECTED_FINAL_RESTART_R3_CORE_IDS = [
    "positive_clean_flat",
    "positive_broker_consistent_open_long",
    "positive_broker_consistent_open_short",
]

EXPECTED_FINAL_RESTART_R3_CURRENT_SHADOW_IDS = [
    "positive_current_shadow_long",
    "positive_current_shadow_short",
    "positive_current_shadow_realized_pnl",
]

EXPECTED_FINAL_RESTART_R3_OPERATIONAL_STATE_IDS = [
    "positive_partial_entry",
    "positive_pending_exit",
    "positive_deferred_entry",
    "positive_deferred_exit",
    "positive_safe_mode_close_only",
]

EXPECTED_FINAL_RESTART_R3_RECOVERY_INDEX_IDS = [
    "positive_non_empty_known_order_index",
    "positive_non_empty_pending_request_index",
    "positive_working_protective_order_hints",
]

EXPECTED_FINAL_RESTART_R3_RISKGATE_RECOVERY_IDS = [
    "positive_single_pending_riskgate_finalization",
    "positive_ordered_multi_row_pending_finalizations",
    "positive_already_complete_recovery_plan",
]

EXPECTED_RUNTIME_RESTORED_OWNERSHIP_IDS = [
    "recovery_incomplete",
    "pending_riskgate_finalization",
    "non_acknowledged_recovery_decision",
    "recovery_plan_binding_mismatch",
    "known_order_index_mismatch",
    "pending_request_index_mismatch",
    "admission_expired",
    "lifecycle_timestamp_reversal_before_persisted",
    "lifecycle_timestamp_reversal_before_bootstrap_notification",
    "runtime_host_attached",
    "intent_sink_attached",
    "non_paper_admission",
    "broker_position_mismatch",
    "broker_side_mismatch_flat_long",
    "broker_side_mismatch_flat_short",
    "broker_side_mismatch_open_long",
    "broker_side_mismatch_open_short",
    "broker_owned_tp_id",
    "broker_owned_stop_id",
    "broker_owned_exchange_stop_id",
    "broker_quantity_not_representable",
    "strategy_mismatch",
    "account_mismatch",
    "instrument_mismatch",
    "config_fingerprint_mismatch",
    "profile_mismatch",
    "riskgate_evidence_mismatch",
    "riskgate_identity_mismatch",
    "riskgate_generation_mismatch",
]

EXPECTED_RUNTIME_RESTORED_COMMON_HELPER_TESTS = {
    "stage5d_b2bd_incomplete_recovery_blocks_before_callback",
    "stage5d_b2bd1r2_pre_callback_matrix_blocks_without_callback",
    "stage5d_b2bd_expired_admission_blocks_before_callback_and_preserves_input",
    "stage5d_b2bd1r3_restored_before_persisted_envelope_blocks_before_callback",
    "stage5d_b2bd1_restored_before_bootstrap_notification_blocks_before_callback",
    "stage5d_b2bd1_flat_broker_side_is_exact_and_blocks_stale_side_before_callback",
    "stage5d_b2bd1r4_open_broker_position_side_mismatch_blocks_before_callback",
}

EXPECTED_STAGE5D_PUBLIC_SYMBOLS = [
    "STAGE5D_ADDITIVE_FREEZE_SCHEMA_VERSION",
    "STAGE5D_PERSISTENCE_ENVELOPE_SCHEMA_VERSION",
    "STAGE5D_RISKGATE_SCHEMA_VERSION",
    "STAGE5D_RUNTIME_PRIVATE_EXTENSION_SCHEMA_VERSION",
    "STAGE5D_STRATEGY_STATE_PAYLOAD_SCHEMA_VERSION",
    "Stage5dAdditiveFreezeEvidence",
    "Stage5dBootstrapBlockReason",
    "Stage5dBootstrapBlocked",
    "Stage5dBootstrappedPaperStrategy",
    "Stage5dBracketReconciliationTimer",
    "Stage5dCleanupRetryState",
    "Stage5dEntryStyle",
    "Stage5dEnvelopeBoundRuntimeStateLoaded",
    "Stage5dEnvelopeValidationError",
    "Stage5dExpectedWorkingSets",
    "Stage5dHybridIntradayStrategyStateV1",
    "Stage5dInstrumentBinding",
    "Stage5dLifecycleReason",
    "Stage5dLifecycleWatermarks",
    "Stage5dOwner",
    "Stage5dPartialEntryTimer",
    "Stage5dPendingEntryExtension",
    "Stage5dPendingExitExtension",
    "Stage5dPersistenceEnvelope",
    "Stage5dPersistenceStage",
    "Stage5dPrivateStateAppliedPaperStrategy",
    "Stage5dRecoveryIndexes",
    "Stage5dRestoreBlockReason",
    "Stage5dRestoreBlocked",
    "Stage5dRiskGateFinalizationOutboxRecord",
    "Stage5dRiskGateFinalizationState",
    "Stage5dRiskGateIdentity",
    "Stage5dRiskGateInjectedPaperStrategy",
    "Stage5dRiskGateInjectionBlockReason",
    "Stage5dRiskGateInjectionBlocked",
    "Stage5dRiskGateLedgerEvidence",
    "Stage5dRiskGateLedgerRecord",
    "Stage5dRiskGateMaterializedState",
    "Stage5dRiskGatePersistence",
    "Stage5dRiskGateRowSource",
    "Stage5dRiskGateRowStatus",
    "Stage5dRuntimePendingRiskGateFinalization",
    "Stage5dRuntimePrivateApplyBlocked",
    "Stage5dRuntimePrivateExtension",
    "Stage5dRuntimeStateRestoreBlocked",
    "Stage5dRuntimeStateRestoreBlockedReason",
    "Stage5dRuntimeStateRestoreOutcome",
    "Stage5dRuntimeStateRestoreRecoveryDisposition",
    "Stage5dRuntimeStateRestoreTerminalFailure",
    "Stage5dRuntimeStateRestoreTerminalReason",
    "Stage5dSemanticStrategyStateV1",
    "Stage5dSide",
    "Stage5dSnapshotBinding",
    "Stage5dStrategyKind",
    "Stage5dStrategyStatePayload",
    "Stage5dStructuredTimestampFormat",
    "Stage5dTimestampPolicy",
    "Stage5dTimestampUnits",
    "Stage5dValidatedPersistenceEnvelope",
    "Stage5dValidatedRiskGateLedgerEvidence",
    "Stage5dValidatedRuntimePrivateExtension",
    "stage5d_apply_runtime_private_extension",
    "stage5d_bind_runtime_state_loaded",
    "stage5d_inject_authoritative_riskgate",
    "stage5d_notify_broker_truth_bootstrap",
    "stage5d_notify_runtime_state_restored",
    "stage5d_retry_authoritative_riskgate_injection",
    "stage5d_retry_bind_runtime_state_loaded",
    "stage5d_retry_broker_truth_bootstrap",
    "stage5d_validate_riskgate_ledger_evidence",
]

FORBIDDEN_STAGE5D_PUBLIC_PATTERNS = [
    re.compile(r"pub\s+fn\s+.*(?:raw|inner|extract|into_parts|strategy)", re.I),
    re.compile(r"pub\s+struct\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+enum\s+(?!Stage5d)[A-Za-z0-9_]+"),
    re.compile(r"pub\s+const\s+(?!STAGE5D)[A-Za-z0-9_]+"),
]

LEGACY_RESTORE_IDENTIFIERS = [
    "restore_stage5c_runtime_state",
    "notify_stage5c_bootstrap",
    "notify_stage5c_runtime_state_restored",
]

ALLOWED_LEGACY_RESTORE_CALL_PATHS = {
    str(LIB_REL),
    str(STAGE5C_HOST_REL),
    str(STAGE5D_REL),
}

EXPECTED_LEGACY_REFERENCE_COUNTS = {
    str(LIB_REL): {
        "restore_stage5c_runtime_state": 1,
        "notify_stage5c_bootstrap": 1,
        "notify_stage5c_runtime_state_restored": 1,
    },
    str(STAGE5C_HOST_REL): {
        "restore_stage5c_runtime_state": 2,
        "notify_stage5c_bootstrap": 4,
        "notify_stage5c_runtime_state_restored": 1,
    },
    str(STAGE5D_REL): {
        "restore_stage5c_runtime_state": 0,
        "notify_stage5c_bootstrap": 0,
        "notify_stage5c_runtime_state_restored": 0,
    },
}


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def extract_fn_body(source: str, signature: str) -> str:
    start = source.find(signature)
    if start < 0:
        return ""
    brace = source.find("{", start + len(signature))
    if brace < 0:
        return ""
    depth = 0
    for idx in range(brace, len(source)):
        char = source[idx]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start : idx + 1]
    return ""


def load_stage5c_checker(root: Path):
    checker_path = root / STAGE5C_CHECKER_REL
    spec = importlib.util.spec_from_file_location("stage5c_api_freeze_check_for_stage5d", checker_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {checker_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def additive_markers(region: str) -> tuple[bytes, bytes]:
    return (
        f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}".encode(),
        f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}".encode(),
    )


def strip_additive_regions(path: Path, regions: list[str]) -> tuple[bytes, list[str]]:
    payload = path.read_bytes()
    failures: list[str] = []
    stripped = payload
    previous_start = -1
    for region in regions:
        begin, end = additive_markers(region)
        begin_count = stripped.count(begin)
        end_count = stripped.count(end)
        if begin_count != 1 or end_count != 1:
            failures.append(
                f"{path}: additive region {region} markers must appear exactly once "
                f"(begin={begin_count}, end={end_count})"
            )
            continue
        begin_index = stripped.find(begin)
        end_index = stripped.find(end)
        if begin_index <= previous_start:
            failures.append(f"{path}: additive region {region} marker order drifted")
        if end_index <= begin_index:
            failures.append(f"{path}: additive region {region} closing marker precedes opening marker")
            continue
        line_end = stripped.find(b"\n", end_index)
        if line_end == -1:
            line_end = len(stripped)
        else:
            line_end += 1
        stripped = stripped[:begin_index] + stripped[line_end:]
        previous_start = begin_index
    return stripped, failures


def collect_additive_regions(path: Path, regions: list[str]) -> tuple[dict[str, str], list[str]]:
    payload = path.read_text()
    failures: list[str] = []
    collected: dict[str, str] = {}
    previous_start = -1
    for region in regions:
        begin = f"// STAGE5D-ADDITIVE-BRIDGE-BEGIN: {region}"
        end = f"// STAGE5D-ADDITIVE-BRIDGE-END: {region}"
        begin_count = payload.count(begin)
        end_count = payload.count(end)
        if begin_count != 1 or end_count != 1:
            failures.append(
                f"{path}: additive region {region} markers must appear exactly once "
                f"(begin={begin_count}, end={end_count})"
            )
            continue
        begin_index = payload.find(begin)
        end_index = payload.find(end)
        if begin_index <= previous_start:
            failures.append(f"{path}: additive region {region} marker order drifted")
        if end_index <= begin_index:
            failures.append(f"{path}: additive region {region} closing marker precedes opening marker")
            continue
        collected[region] = payload[begin_index:end_index + len(end)]
        previous_start = begin_index
    return collected, failures


def legacy_identifier_hits(source: str) -> list[str]:
    return [
        identifier
        for identifier in LEGACY_RESTORE_IDENTIFIERS
        if re.search(rf"\b{re.escape(identifier)}\b", source)
    ]


def legacy_identifier_counts(source: str) -> dict[str, int]:
    return {
        identifier: len(re.findall(rf"\b{re.escape(identifier)}\b", source))
        for identifier in LEGACY_RESTORE_IDENTIFIERS
    }


def parse_stage5d_public_symbols(source: str) -> list[str]:
    symbols: set[str] = set()
    for pattern in [
        r"^pub\s+struct\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+enum\s+(Stage5d[A-Za-z0-9_]+)",
        r"^pub\s+const\s+(STAGE5D[A-Za-z0-9_]+)",
        r"^pub\s+fn\s+(stage5d_[A-Za-z0-9_]+)",
    ]:
        for match in re.finditer(pattern, source, re.M):
            symbols.add(match.group(1))
    return sorted(symbols)


def normalize_signature(text: str) -> str:
    text = re.sub(r"//.*", "", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text.removesuffix("{").removesuffix(";").strip()


def collect_signature(lines: list[str], start_index: int) -> tuple[str, int]:
    parts = []
    index = start_index
    while index < len(lines):
        line = lines[index].strip()
        parts.append(line)
        if "{" in line or line.endswith(";"):
            break
        index += 1
    signature = " ".join(parts)
    if "{" in signature:
        signature = signature.split("{", 1)[0]
    return normalize_signature(signature), index


def top_level_brace_delta(line: str) -> int:
    stripped = line.strip()
    if stripped.startswith("//"):
        return 0
    return stripped.count("{") - stripped.count("}")


def collect_block(lines: list[str], start_index: int) -> tuple[list[str], int]:
    block = [lines[start_index]]
    depth = top_level_brace_delta(lines[start_index])
    index = start_index + 1
    while index < len(lines) and depth > 0:
        block.append(lines[index])
        depth += top_level_brace_delta(lines[index])
        index += 1
    return block, index - 1


def parse_struct_fields(block: list[str]) -> list[dict[str, str]]:
    fields = []
    for line in block[1:-1]:
        stripped = line.strip().rstrip(",")
        if not stripped.startswith("pub "):
            continue
        declaration = stripped.removeprefix("pub ").strip()
        if ":" not in declaration:
            continue
        name, type_name = declaration.split(":", 1)
        fields.append({"name": name.strip(), "type": normalize_signature(type_name)})
    return fields


def parse_enum_variants(block: list[str]) -> list[str]:
    variants = []
    depth = 0
    for raw in block[1:-1]:
        stripped = raw.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("//"):
            continue
        if depth == 0:
            match = re.match(r"([A-Z][A-Za-z0-9_]*)", stripped)
            if match:
                variants.append(match.group(1))
        depth += stripped.count("{") + stripped.count("(") - stripped.count("}") - stripped.count(")")
        if depth < 0:
            depth = 0
    return variants


def public_api_hash(surface: dict[str, Any]) -> str:
    payload = json.dumps(surface, sort_keys=True, separators=(",", ":")).encode()
    return sha256_bytes(payload)


def parse_stage5d_surface(stage5d_source: str, lib_source: str) -> dict[str, Any]:
    lines = stage5d_source.splitlines()
    constants = []
    free_functions = []
    types: dict[str, dict[str, Any]] = {}
    methods = []

    index = 0
    while index < len(lines):
        stripped = lines[index].strip()

        const_match = re.match(r"^pub const (STAGE5D[A-Za-z0-9_]+)\s*:\s*([^=]+)=", stripped)
        if const_match:
            constants.append(
                {
                    "name": const_match.group(1),
                    "type": normalize_signature(const_match.group(2)),
                    "signature": normalize_signature(stripped),
                }
            )

        struct_match = re.match(r"^pub struct (Stage5d[A-Za-z0-9_]+)", stripped)
        if struct_match:
            name = struct_match.group(1)
            block, end_index = collect_block(lines, index) if "{" in stripped else ([lines[index]], index)
            fields = parse_struct_fields(block) if "{" in stripped else []
            types[name] = {
                "name": name,
                "kind": "struct",
                "opaque": len(fields) == 0,
                "public_fields": fields,
                "public_variants": [],
            }
            index = end_index

        enum_match = re.match(r"^pub enum (Stage5d[A-Za-z0-9_]+)", stripped)
        if enum_match:
            name = enum_match.group(1)
            block, end_index = collect_block(lines, index)
            types[name] = {
                "name": name,
                "kind": "enum",
                "opaque": False,
                "public_fields": [],
                "public_variants": parse_enum_variants(block),
            }
            index = end_index

        fn_match = re.match(r"^pub fn (stage5d_[A-Za-z0-9_]+)", stripped)
        if fn_match:
            signature, end_index = collect_signature(lines, index)
            free_functions.append({"name": fn_match.group(1), "signature": signature})
            index = end_index

        impl_match = re.match(r"^impl (Stage5d[A-Za-z0-9_]+)", stripped)
        if impl_match and "{" in stripped:
            type_name = impl_match.group(1)
            block, end_index = collect_block(lines, index)
            for offset, block_line in enumerate(block):
                method_stripped = block_line.strip()
                method_match = re.match(r"^pub fn ([A-Za-z0-9_]+)", method_stripped)
                if not method_match:
                    continue
                signature, _ = collect_signature(block, offset)
                methods.append(
                    {
                        "type": type_name,
                        "name": method_match.group(1),
                        "signature": signature,
                    }
                )
            index = end_index

        index += 1

    surface = {
        "public_reexports": parse_stage5d_reexports(lib_source),
        "public_constants": sorted(constants, key=lambda item: item["name"]),
        "public_free_functions": sorted(free_functions, key=lambda item: item["name"]),
        "public_types": sorted(types.values(), key=lambda item: item["name"]),
        "public_methods": sorted(methods, key=lambda item: (item["type"], item["name"], item["signature"])),
    }
    surface["opaque_capabilities"] = sorted(
        item["name"] for item in surface["public_types"] if item["kind"] == "struct" and item["opaque"]
    )
    surface["externally_constructible_enums"] = sorted(
        item["name"] for item in surface["public_types"] if item["kind"] == "enum"
    )
    surface["normalized_signature_hash"] = public_api_hash(surface)
    surface["public_surface_counts"] = {
        "public_reexports": len(surface["public_reexports"]),
        "public_constants": len(surface["public_constants"]),
        "public_free_functions": len(surface["public_free_functions"]),
        "public_types": len(surface["public_types"]),
        "public_methods": len(surface["public_methods"]),
        "opaque_capabilities": len(surface["opaque_capabilities"]),
        "externally_constructible_enums": len(surface["externally_constructible_enums"]),
    }
    return surface


def parse_stage5d_reexports(lib_source: str) -> list[str]:
    match = re.search(r"pub use stage5d_persistence::\{(?P<body>.*?)\};", lib_source, re.S)
    if not match:
        return []
    body = match.group("body")
    return sorted(token.strip() for token in body.replace("\n", " ").split(",") if token.strip())


def validate_stage5c_public_shape(root: Path, manifest: dict[str, Any], failures: list[str]) -> None:
    stage5c_checker = load_stage5c_checker(root)
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    surface = stage5c_checker.derive_manifest_surface()
    for key in [
        "public_reexports",
        "public_constants",
        "public_free_functions",
        "public_types",
        "public_methods",
        "opaque_capabilities",
        "externally_constructible_enums",
        "normalized_signature_hash",
    ]:
        if surface.get(key) != stage5c_manifest.get(key):
            failures.append(f"Stage 5C public API shape drifted for {key}")
    declared_count = (
        len(surface["public_constants"])
        + len(surface["public_free_functions"])
        + len(surface["public_types"])
    )
    expected_count = manifest.get("stage5c_public_api", {}).get("public_symbol_count")
    if declared_count != expected_count:
        failures.append(
            f"Stage 5C public symbol count mismatch: actual={declared_count} expected={expected_count}"
        )
    expected_hash = manifest.get("stage5c_public_api", {}).get("normalized_signature_hash")
    if surface.get("normalized_signature_hash") != expected_hash:
        failures.append("Stage 5C normalized signature hash mismatch")


def validate_legacy_restore_call_sites(root: Path, failures: list[str]) -> None:
    for path in sorted((root / "crates").glob("**/*.rs")):
        rel = str(path.relative_to(root))
        if rel in ALLOWED_LEGACY_RESTORE_CALL_PATHS:
            continue
        source = path.read_text(errors="replace")
        for identifier in legacy_identifier_hits(source):
            failures.append(f"legacy Stage 5C restore bypass symbol forbidden: {rel}: {identifier}")

    for rel, expected_counts in EXPECTED_LEGACY_REFERENCE_COUNTS.items():
        path = root / rel
        if not path.is_file():
            failures.append(f"legacy Stage 5C restore allowlisted file missing: {rel}")
            continue
        actual_counts = legacy_identifier_counts(path.read_text(errors="replace"))
        if actual_counts != expected_counts:
            failures.append(
                f"legacy Stage 5C restore reference count mismatch for {rel}: "
                f"actual={actual_counts} expected={expected_counts}"
            )


def validate_no_legacy_identifiers_in_additive_regions(
    root: Path,
    approved_bridge_regions: dict[str, list[str]],
    failures: list[str],
) -> None:
    for rel, regions in approved_bridge_regions.items():
        path = root / rel
        if not path.is_file():
            continue
        collected, marker_failures = collect_additive_regions(path, regions)
        failures.extend(marker_failures)
        for region, source in collected.items():
            for identifier in legacy_identifier_hits(source):
                if (
                    rel == str(STAGE5C_HOST_REL)
                    and region == "type-state-transitions"
                    and identifier == "restore_stage5c_runtime_state"
                    and "mod stage5d_pair_binding_restore_tests" in source
                    and len(re.findall(r"\brestore_stage5c_runtime_state\s*\(", source)) == 1
                ):
                    continue
                failures.append(
                    "legacy Stage 5C restore symbol forbidden in additive region: "
                    f"{rel}:{region}:{identifier}"
                )

    stage5d_path = root / STAGE5D_REL
    if stage5d_path.is_file():
        for identifier in legacy_identifier_hits(stage5d_path.read_text(errors="replace")):
            failures.append(
                f"legacy Stage 5C restore symbol forbidden in Stage 5D persistence surface: {identifier}"
            )


def source_function_slice(source: str, function_name: str) -> str:
    match = re.search(rf"\n(?:pub\s+)?fn\s+{re.escape(function_name)}\b", source)
    if not match:
        return ""
    start = match.start()
    next_match = re.search(r"\n(?:pub\s+)?fn\s+[A-Za-z0-9_]+\b", source[start + 1 :])
    if next_match:
        return source[start : start + 1 + next_match.start()]
    return source[start:]


def rust_test_body(source: str, test_name: str) -> str:
    match = re.search(rf"\n\s*fn\s+{re.escape(test_name)}\s*\(", source)
    if not match:
        return ""
    next_test = source.find("\n    #[test]", match.end())
    if next_test == -1:
        next_test = source.find("\n    fn ", match.end())
    if next_test == -1:
        next_test = len(source)
    return source[match.start():next_test]


def source_without_rustdoc_comments(source: str) -> str:
    return "\n".join(
        line for line in source.splitlines() if not line.lstrip().startswith("///")
    )


def validate_stage5d_single_bridge_call_sites(
    root: Path,
    failures: list[str],
    *,
    label: str,
    identifier: str,
    allowed_call_function: str,
) -> None:
    pattern = re.compile(rf"\b{re.escape(identifier)}\b")
    source_root = root / "crates/strategy-runtime-core/src"
    refs: dict[str, int] = {}
    for path in sorted(source_root.rglob("*.rs")):
        rel = str(path.relative_to(root))
        refs[rel] = len(
            pattern.findall(source_without_rustdoc_comments(path.read_text(errors="replace")))
        )

    stage5c_rel = str(STAGE5C_HOST_REL)
    stage5d_rel = str(STAGE5D_REL)
    stage5c_source = (root / STAGE5C_HOST_REL).read_text(errors="replace")
    stage5d_source = source_without_rustdoc_comments(
        (root / STAGE5D_REL).read_text(errors="replace")
    )
    expected_definition = rf"pub\(crate\)\s+fn\s+{re.escape(identifier)}\s*\("
    if refs.get(stage5c_rel) != 1 or not re.search(expected_definition, stage5c_source):
        failures.append(
            f"Stage 5D {label} bridge definition contract mismatch: {stage5c_rel}"
        )
    if refs.get(stage5d_rel) != 1:
        failures.append(
            f"Stage 5D {label} bridge production call count mismatch: {stage5d_rel} "
            f"actual={refs.get(stage5d_rel)} expected=1"
        )
    allowed_function = source_function_slice(stage5d_source, allowed_call_function)
    if len(pattern.findall(allowed_function)) != 1:
        failures.append(
            f"Stage 5D {label} bridge call must remain inside "
            f"{allowed_call_function}"
        )
    for rel, count in refs.items():
        if rel not in {stage5c_rel, stage5d_rel} and count:
            failures.append(
                f"Stage 5D {label} bridge reference outside allowlist: {rel} count={count}"
            )


def validate_stage5d_bridge_call_sites(root: Path, failures: list[str]) -> None:
    validate_stage5d_single_bridge_call_sites(
        root,
        failures,
        label="bootstrap",
        identifier=STAGE5D_BOOTSTRAP_BRIDGE_IDENTIFIER,
        allowed_call_function=STAGE5D_BOOTSTRAP_BRIDGE_ALLOWED_CALL_FUNCTION,
    )
    validate_stage5d_single_bridge_call_sites(
        root,
        failures,
        label="riskgate",
        identifier=STAGE5D_RISKGATE_BRIDGE_IDENTIFIER,
        allowed_call_function=STAGE5D_RISKGATE_BRIDGE_ALLOWED_CALL_FUNCTION,
    )
    validate_stage5d_single_bridge_call_sites(
        root,
        failures,
        label="runtime-restored",
        identifier=STAGE5D_RUNTIME_RESTORED_BRIDGE_IDENTIFIER,
        allowed_call_function=STAGE5D_RUNTIME_RESTORED_BRIDGE_ALLOWED_CALL_FUNCTION,
    )


def validate_stage5d_runtime_restored_ownership_inventory(
    root: Path, stage5d_source: str, failures: list[str]
) -> None:
    ownership_path = root / RUNTIME_RESTORED_OWNERSHIP_REL
    if not ownership_path.exists():
        failures.append("Stage 5D runtime-restored r6 ownership inventory missing")
        return
    try:
        inventory = json.loads(ownership_path.read_text())
    except json.JSONDecodeError:
        failures.append("Stage 5D runtime-restored r6 ownership inventory must be valid JSON")
        return

    if inventory.get("schema_version") != 1:
        failures.append("Stage 5D runtime-restored r6 ownership inventory schema mismatch")
    if inventory.get("stage") != "5D-b2b-d1-r6":
        failures.append("Stage 5D runtime-restored r6 ownership stage mismatch")
    if inventory.get("closed_surfaces") != {
        "redis": False,
        "finam": False,
        "transport": False,
        "dispatch": False,
        "runtime_live": False,
        "broker_execution": False,
    }:
        failures.append("Stage 5D runtime-restored r6 ownership closed-surface contract mismatch")

    rows = inventory.get("ownership_rows")
    if not isinstance(rows, list):
        failures.append("Stage 5D runtime-restored r6 ownership rows missing")
        return

    ids = [row.get("case_id") for row in rows if isinstance(row, dict)]
    if ids != EXPECTED_RUNTIME_RESTORED_OWNERSHIP_IDS:
        failures.append("Stage 5D runtime-restored r6 ownership case inventory mismatch")
    if len(set(ids)) != len(ids):
        failures.append("Stage 5D runtime-restored r6 ownership case ids must be unique")

    allowed_proof_kinds = {"common_helper", "earlier_gate", "defensive_branch"}
    for row in rows:
        if not isinstance(row, dict):
            failures.append("Stage 5D runtime-restored r6 ownership row must be an object")
            continue
        case_id = row.get("case_id")
        representable = row.get("representable_at_b2bd")
        owning_stage = row.get("owning_stage")
        owning_function = row.get("owning_function")
        focused_test = row.get("focused_test")
        expected_reason = row.get("expected_reason")
        proof_kind = row.get("proof_kind")
        source_domain_argument = row.get("source_domain_argument")

        for field_name, value in {
            "case_id": case_id,
            "owning_stage": owning_stage,
            "owning_function": owning_function,
            "focused_test": focused_test,
            "expected_reason": expected_reason,
            "proof_kind": proof_kind,
            "source_domain_argument": source_domain_argument,
        }.items():
            if not isinstance(value, str) or not value.strip():
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} missing {field_name}"
                )
        if proof_kind not in allowed_proof_kinds:
            failures.append(
                f"Stage 5D runtime-restored r6 ownership row {case_id!r} has unsupported proof_kind"
            )
        if isinstance(owning_function, str):
            for function_name in re.split(r"\s+and\s+|\s*/\s*", owning_function):
                function_name = function_name.strip()
                if function_name and function_name not in stage5d_source:
                    failures.append(
                        f"Stage 5D runtime-restored r6 ownership function missing: {function_name}"
                    )
        if isinstance(focused_test, str) and focused_test not in stage5d_source:
            failures.append(
                f"Stage 5D runtime-restored r6 ownership focused test missing: {focused_test}"
            )

        if representable is True:
            if proof_kind != "common_helper":
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} must use common_helper"
                )
            if owning_stage != "Stage 5D-b2b-d":
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} must be owned by b2b-d"
                )
            if focused_test not in EXPECTED_RUNTIME_RESTORED_COMMON_HELPER_TESTS:
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} has unexpected common-helper test"
                )
            elif "stage5d_test_assert_restore_blocks_before_callback(" not in rust_test_body(
                stage5d_source, focused_test
            ):
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} focused test must use common helper"
                )
        elif representable is False:
            if proof_kind == "common_helper":
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} cannot use common_helper when not representable at b2b-d"
                )
            if (
                case_id != "broker_quantity_not_representable"
                and owning_stage == "Stage 5D-b2b-d"
            ):
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} must not be owned by b2b-d"
                )
            if case_id == "broker_quantity_not_representable":
                if proof_kind != "defensive_branch":
                    failures.append(
                        "Stage 5D runtime-restored r6 quantity ownership must remain defensive_branch"
                    )
            elif proof_kind != "earlier_gate":
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} must be earlier_gate"
                )
        else:
                failures.append(
                    f"Stage 5D runtime-restored r6 ownership row {case_id!r} representable flag invalid"
                )


def validate_stage5d_final_restart_inventory(
    root: Path, stage5d_source: str, failures: list[str]
) -> None:
    inventory_path = root / FINAL_RESTART_INVENTORY_REL
    if not inventory_path.exists():
        failures.append("Stage 5D final restart r1 scenario inventory missing")
        return
    try:
        inventory = json.loads(inventory_path.read_text())
    except json.JSONDecodeError:
        failures.append("Stage 5D final restart r1 scenario inventory must be valid JSON")
        return

    if inventory.get("schema_version") != 1:
        failures.append("Stage 5D final restart r1 scenario inventory schema mismatch")
    if inventory.get("stage") != EXPECTED_FINAL_RESTART_INVENTORY_STAGE:
        failures.append("Stage 5D final restart r1 scenario inventory stage mismatch")
    if inventory.get("closed_surfaces") != EXPECTED_CLOSED_SURFACES:
        failures.append("Stage 5D final restart r1 scenario inventory closed-surface mismatch")

    rows = inventory.get("scenario_rows")
    if not isinstance(rows, list) or not rows:
        failures.append("Stage 5D final restart r1 scenario rows missing")
        return
    case_ids = [row.get("case_id") for row in rows if isinstance(row, dict)]
    if case_ids != EXPECTED_FINAL_RESTART_SCENARIO_IDS:
        failures.append("Stage 5D final restart r1 scenario inventory mismatch")
    if len(set(case_ids)) != len(case_ids):
        failures.append("Stage 5D final restart r1 scenario ids must be unique")

    required_fields = (
        "case_id",
        "category",
        "source_produced",
        "clean_process_restart",
        "package_sections",
        "expected_outcome",
        "owning_test",
        "expected_reason_or_fingerprint",
        "restart_after_action",
        "replay_idempotent",
        "stage5c_continuation",
        "closed_surfaces_proven",
    )
    for row in rows:
        if not isinstance(row, dict):
            failures.append("Stage 5D final restart r1 scenario row must be an object")
            continue
        case_id = row.get("case_id")
        for field_name in required_fields:
            if field_name not in row:
                failures.append(
                    f"Stage 5D final restart r1 scenario row {case_id!r} missing {field_name}"
                )
        if row.get("closed_surfaces_proven") != EXPECTED_CLOSED_SURFACES:
            failures.append(
                f"Stage 5D final restart r1 scenario row {case_id!r} closed-surface proof mismatch"
            )
        owning_test = row.get("owning_test")
        if isinstance(owning_test, str) and owning_test not in stage5d_source:
            failures.append(
                f"Stage 5D final restart r1 scenario owning test missing: {owning_test}"
            )
        elif not isinstance(owning_test, str):
            failures.append(
                f"Stage 5D final restart r1 scenario row {case_id!r} owning_test invalid"
            )
        elif not re.search(r"#\[test\]\s+fn\s+" + re.escape(owning_test) + r"\s*\(", stage5d_source):
            failures.append(
                f"Stage 5D final restart r1 scenario owning item is not a test: {owning_test}"
            )
        package_sections = row.get("package_sections")
        if not isinstance(package_sections, list):
            failures.append(
                f"Stage 5D final restart r1 scenario row {case_id!r} package_sections invalid"
            )
            continue
        if row.get("source_produced") is True and row.get("clean_process_restart") is True:
            required_sections = {
                "stage5d_persistence_envelope",
                "stage5d_riskgate_ledger_evidence",
            }
            if not required_sections.issubset(set(package_sections)):
                failures.append(
                    f"Stage 5D final restart r1 scenario row {case_id!r} lacks durable package sections"
                )


def validate_stage5d_final_restart_r3_inventory(root: Path, failures: list[str]) -> None:
    inventory_path = root / FINAL_RESTART_R3_INVENTORY_REL
    if not inventory_path.exists():
        failures.append("Stage 5D final r3 resumption inventory proof missing")
        return
    try:
        inventory = json.loads(inventory_path.read_text())
    except json.JSONDecodeError:
        failures.append("Stage 5D final r3 resumption inventory must be valid JSON")
        return
    if inventory.get("schema_version") != 1:
        failures.append("Stage 5D final r3 resumption inventory schema mismatch")
    if inventory.get("stage") != "5D-final-restart-r3":
        failures.append("Stage 5D final r3 resumption inventory stage mismatch")
    if inventory.get("status") != "riskgate_recovery_r1_r2_evidence_closed":
        failures.append("Stage 5D final r3 riskgate recovery inventory must close 21/0 evidence")
    if inventory.get("closed_surfaces") != EXPECTED_CLOSED_SURFACES:
        failures.append("Stage 5D final r3 resumption inventory closed-surface mismatch")
    rows = inventory.get("scenario_rows")
    if not isinstance(rows, list):
        failures.append("Stage 5D final r3 resumption rows missing")
        return
    positive_ids = [
        row.get("case_id")
        for row in rows
        if isinstance(row, dict) and row.get("category") == "positive"
    ]
    if positive_ids != EXPECTED_FINAL_RESTART_R3_POSITIVE_IDS:
        failures.append("Stage 5D final r3 mandatory positive inventory mismatch")
    if len(set(positive_ids)) != len(positive_ids):
        failures.append("Stage 5D final r3 mandatory positive ids must be unique")
    r3a_ids = set(EXPECTED_FINAL_RESTART_R3_ACCEPTED_IDS)
    core_ids = set(EXPECTED_FINAL_RESTART_R3_CORE_IDS)
    current_shadow_ids = set(EXPECTED_FINAL_RESTART_R3_CURRENT_SHADOW_IDS)
    operational_state_ids = set(EXPECTED_FINAL_RESTART_R3_OPERATIONAL_STATE_IDS)
    recovery_index_ids = set(EXPECTED_FINAL_RESTART_R3_RECOVERY_INDEX_IDS)
    riskgate_recovery_ids = set(EXPECTED_FINAL_RESTART_R3_RISKGATE_RECOVERY_IDS)
    accepted_expected_ids = (
        r3a_ids
        | core_ids
        | current_shadow_ids
        | operational_state_ids
        | recovery_index_ids
        | riskgate_recovery_ids
    )
    accepted_ids = []
    todo_ids = []
    source_path = root / STAGE5D_REL
    stage5d_source = source_path.read_text()
    for row in rows:
        if not isinstance(row, dict):
            failures.append("Stage 5D final r3 resumption row must be object")
            continue
        if row.get("category") != "positive":
            failures.append("Stage 5D final r3 row category must be positive")
        case_id = row.get("case_id")
        status = row.get("execution_status")
        owning_test = row.get("owning_test")
        if status == "accepted_r3a_r1_source_produced":
            accepted_ids.append(case_id)
            if case_id not in r3a_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3a_source_pending_entry_full_restart_matrix":
                failures.append("Stage 5D final r3 r3a-r1 reuse proof missing")
        elif status == "accepted_r3_positive_core_r1b_source_produced":
            accepted_ids.append(case_id)
            if case_id not in core_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3_positive_core_source_produced_full_restart_matrix":
                failures.append("Stage 5D final r3 positive-core owner/status proof missing")
            expected_entrypoint = {
                "positive_clean_flat": "stage5d_test_source_clean_flat_strategy",
                "positive_broker_consistent_open_long": "stage5d_test_source_broker_open_strategy",
                "positive_broker_consistent_open_short": "stage5d_test_source_broker_open_strategy",
            }.get(case_id)
            if row.get("producer_kind") != "runtime_callback":
                failures.append("Stage 5D final r3 positive-core producer lineage proof missing")
            if row.get("producer_entrypoint") != expected_entrypoint:
                failures.append("Stage 5D final r3 positive-core producer lineage proof missing")
            if expected_entrypoint and f"fn {expected_entrypoint}" not in stage5d_source:
                failures.append("Stage 5D final r3 positive-core producer lineage proof missing")
            for key in [
                "canonical_package_path",
                "source_object_destroyed",
                "strict_decode_used",
                "fresh_runtime_used",
                "stage5c_continuation_executed",
            ]:
                if row.get(key) is not True:
                    failures.append("Stage 5D final r3 positive-core producer lineage proof missing")
        elif status == "accepted_r3_current_shadow_r1_source_produced":
            accepted_ids.append(case_id)
            if case_id not in current_shadow_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix":
                failures.append("Stage 5D final r3 current-shadow owner/status proof missing")
            if row.get("producer_kind") != "runtime_callback":
                failures.append("Stage 5D final r3 current-shadow producer lineage proof missing")
            if row.get("producer_entrypoint") != "stage5d_test_source_current_shadow_strategy":
                failures.append("Stage 5D final r3 current-shadow producer lineage proof missing")
            if row.get("materialized_apply_boundary") != "stage5d_test_apply_approved_current_shadow_materialized_boundary":
                failures.append("Stage 5D final r3 current-shadow materialized apply proof missing")
            for key in [
                "canonical_package_path",
                "source_object_destroyed",
                "strict_decode_used",
                "fresh_runtime_used",
                "exact_post_apply_equality_checked",
                "stage5c_continuation_executed",
            ]:
                if row.get(key) is not True:
                    failures.append("Stage 5D final r3 current-shadow producer lineage proof missing")
        elif status == "accepted_r3_operational_state_r1_source_produced":
            accepted_ids.append(case_id)
            if case_id not in operational_state_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix":
                failures.append("Stage 5D final r3 operational-state owner/status proof missing")
            if row.get("producer_kind") != "runtime_callback":
                failures.append("Stage 5D final r3 operational-state producer lineage proof missing")
            callbacks = row.get("producer_callbacks")
            if not isinstance(callbacks, list) or not callbacks:
                failures.append("Stage 5D final r3 operational-state producer lineage proof missing")
            for key in [
                "canonical_package_path",
                "source_object_destroyed",
                "strict_decode_used",
                "fresh_runtime_used",
                "private_apply_before_bootstrap",
                "broker_truth_exact_qty_checked",
                "lifecycle_request_cycle_timestamp_equality_checked",
                "stage5c_continuation_executed",
            ]:
                if row.get(key) is not True:
                    failures.append("Stage 5D final r3 operational-state producer lineage proof missing")
            per_case_flag = {
                "positive_partial_entry": "partial_timer_quantity_evidence_checked",
                "positive_pending_exit": "pending_exit_duplicate_suppression_checked",
                "positive_deferred_entry": "deferred_entry_stop_take_reason_checked",
                "positive_deferred_exit": "deferred_exit_close_only_semantics_checked",
                "positive_safe_mode_close_only": "safe_mode_entry_block_checked",
            }.get(case_id)
            if per_case_flag and row.get(per_case_flag) is not True:
                failures.append("Stage 5D final r3 operational-state lifecycle-specific proof missing")
        elif status == "accepted_r3_recovery_index_r1_source_produced":
            accepted_ids.append(case_id)
            if case_id not in recovery_index_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix":
                failures.append("Stage 5D final r3 recovery-index owner/status proof missing")
            if row.get("producer_kind") != "runtime_callback":
                failures.append("Stage 5D final r3 recovery-index producer lineage proof missing")
            callbacks = row.get("producer_callbacks")
            if not isinstance(callbacks, list) or not callbacks:
                failures.append("Stage 5D final r3 recovery-index producer lineage proof missing")
            for key in [
                "canonical_package_path",
                "source_object_destroyed",
                "strict_decode_used",
                "fresh_runtime_used",
                "private_apply_before_bootstrap",
                "duplicate_suppression_after_restore",
                "stage5c_continuation_executed",
            ]:
                if row.get(key) is not True:
                    failures.append("Stage 5D final r3 recovery-index producer lineage proof missing")
            if case_id in {
                "positive_non_empty_known_order_index",
                "positive_working_protective_order_hints",
            } and row.get("broker_truth_exact_working_order_checked") is not True:
                failures.append("Stage 5D final r3 recovery-index broker-truth proof missing")
            if case_id == "positive_working_protective_order_hints":
                expected_callbacks = [
                    "on_order:working_tp",
                    "on_stop_order:working_sl",
                    "on_order:duplicate_and_terminal_tp",
                    "on_stop_order:duplicate_and_terminal_sl",
                ]
                if callbacks != expected_callbacks:
                    failures.append("Stage 5D final r3 recovery-index protective callback inventory stale")
                for key in [
                    "supplemental_stop_truth_validated",
                    "normalization_block_capability_preserved",
                    "exact_tp_truth_checked",
                    "exact_sl_truth_checked",
                    "wrong_kind_swap_duplicate_fail_closed_matrix",
                    "tp_duplicate_suppressed",
                    "sl_duplicate_suppressed",
                    "tp_terminal_no_entry_or_flip",
                    "sl_terminal_no_entry_or_flip",
                ]:
                    if row.get(key) is not True:
                        failures.append("Stage 5D final r3 recovery-index protective evidence metadata missing")
                if "stop_truth_surface_remains_unsupported" in row:
                    failures.append("Stage 5D final r3 recovery-index protective inventory retained obsolete stop-truth metadata")
            if (
                case_id == "positive_non_empty_pending_request_index"
                and row.get("terminal_resolution_no_orphan_index_checked") is not True
            ):
                failures.append("Stage 5D final r3 recovery-index terminal resolution proof missing")
        elif status == "accepted_r3_riskgate_recovery_r1_r2_source_produced":
            accepted_ids.append(case_id)
            if case_id not in riskgate_recovery_ids:
                failures.append("Stage 5D final r3 accepted executable set mismatch")
            if owning_test != "stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix":
                failures.append("Stage 5D final r3 riskgate-recovery owner/status proof missing")
            if row.get("producer_kind") != "runtime_callback":
                failures.append("Stage 5D final r3 riskgate-recovery source producer proof missing")
            if row.get("producer_entrypoint") != "stage5d_test_source_runtime_with_real_pending_finalizations":
                failures.append("Stage 5D final r3 riskgate-recovery source producer proof missing")
            callbacks = row.get("producer_callbacks")
            if not isinstance(callbacks, list) or not callbacks:
                failures.append("Stage 5D final r3 riskgate-recovery callback inventory missing")
            for key in [
                "canonical_package_path",
                "source_object_destroyed",
                "strict_decode_used",
                "fresh_runtime_used",
                "durable_store_matrix_executed",
                "idempotent_replay_verified",
                "callback_exactly_once_checked",
                "stage5c_continuation_executed",
                "stage5e_closed",
            ]:
                if row.get(key) is not True:
                    failures.append("Stage 5D final r3 riskgate-recovery evidence metadata missing")
            per_case_flag = {
                "positive_single_pending_riskgate_finalization": "one_row_equality_checked",
                "positive_ordered_multi_row_pending_finalizations": "ordered_multi_row_equality_checked",
                "positive_already_complete_recovery_plan": "complete_plan_noop_checked",
            }.get(case_id)
            if per_case_flag and row.get(per_case_flag) is not True:
                failures.append("Stage 5D final r3 riskgate-recovery case-specific proof missing")
        elif status == "todo_source_produced":
            todo_ids.append(case_id)
            if owning_test is not None:
                failures.append("Stage 5D final r3 TODO row must not claim owning test")
        else:
            failures.append("Stage 5D final r3 unapproved execution status")
        if status == "accepted_r3a_r1_source_produced" and owning_test is None:
            failures.append("Stage 5D final r3 accepted row must have owning test")
    if set(accepted_ids) != accepted_expected_ids or len(accepted_ids) != len(accepted_expected_ids):
        failures.append("Stage 5D final r3 accepted executable set mismatch")
    expected_todo_ids = set(EXPECTED_FINAL_RESTART_R3_POSITIVE_IDS) - accepted_expected_ids
    if set(todo_ids) != expected_todo_ids or len(todo_ids) != len(expected_todo_ids):
        failures.append("Stage 5D final r3 TODO source-produced set mismatch")
    positive_fn = "fn stage5d_final_r3_positive_core_source_produced_full_restart_matrix()"
    positive_start = stage5d_source.find(positive_fn)
    positive_end = stage5d_source.find(
        "#[test]",
        positive_start + len(positive_fn),
    )
    positive_body = (
        stage5d_source[positive_start:positive_end]
        if positive_start >= 0 and positive_end >= 0
        else ""
    )
    if "stage5d_test_r3_positive_core_source_full_restart(case)" not in positive_body:
        failures.append("Stage 5D final r3 positive-core actual source callback proof missing")
    for forbidden in [
        "stage5d_test_canonical_package_full_restart_with_stage5c_continuation(",
        "stage5d_test_set_position_side(",
        "flat_persisted_fixture()",
    ]:
        if forbidden in positive_body:
            failures.append("Stage 5D final r3 positive-core fixture substitution guard missing")
    required_r1b_tokens = [
        "positive_core_clean_flat_actual_source_lifecycle",
        "positive_core_broker_open_long_short_actual_source_lifecycle",
        "no_flat_persisted_fixture_as_positive_core_producer",
        "no_stage5d_test_set_position_side_as_positive_core_producer",
        "source_runtime_destroyed_before_restart_boundary",
        "strict_package_decode_used_for_positive_core",
        "actual_post_apply_state_equality_checked",
        "actual_post_apply_broker_truth_quantity_side_checked",
        "actual_source_core_cases_executed_3",
        "current_shadow_discovery_cases_executed_3",
        "current_shadow_discovery_without_preseed",
        "current_shadow_first_mismatch_materialized_riskgate_state",
    ]
    for token in required_r1b_tokens:
        if token not in stage5d_source:
            failures.append("Stage 5D final r3 positive-core r1b marker proof missing")
    if "actual post-apply/restored state must match strict source envelope" not in stage5d_source:
        failures.append("Stage 5D final r3 positive-core actual post-apply equality proof missing")
    if "stage5d_final_r3_current_shadow_discovery_localizes_materialized_gap" not in stage5d_source:
        failures.append("Stage 5D final r3 current-shadow executable discovery proof missing")
    test_module_start = stage5d_source.find("\n#[cfg(test)]\nmod tests")
    materialized_boundary_start = stage5d_source.find(
        "stage5d_apply_validated_materialized_riskgate_for_restart("
    )
    if materialized_boundary_start < 0 or (
        test_module_start >= 0 and materialized_boundary_start > test_module_start
    ):
        failures.append("Stage 5D final r3 current-shadow production materialized apply boundary missing")
    production_boundary_tokens = [
        "Stage5dMaterializedRiskGateAppliedPaperStrategy",
        "Stage5dMaterializedRiskGateApplyBlocked",
        "Stage5dValidatedPersistenceEnvelope",
        "Stage5dValidatedRiskGateLedgerEvidence",
        "stage5d_build_validated_materialized_riskgate_apply_state(",
        "validated_envelope",
        "input_capability_preserved",
        "strategy.on_risk_gate_state(&apply_state.riskgate_state)",
        "stage5d_validate_canonical_restart_export_self_consistency(",
        "stage5d_compare_semantic_materialized_to_runtime_state(",
        "LedgerIdentityMismatch",
        "LedgerGenerationMismatch",
        "LedgerTailMismatch",
        "&evidence.current_shadow_pnl_points,",
    ]
    if any(token not in stage5d_source for token in production_boundary_tokens):
        failures.append("Stage 5D final r3 current-shadow production materialized apply proof missing")
    forbidden_boundary_tokens = [
        "pub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(\n    strategy: &mut crate::hybrid_intraday_runtime::HybridIntradayRuntimeStrategy,\n    envelope: &Stage5dPersistenceEnvelope",
        "stage5d_raw_strategy_extractor",
        "direct_current_shadow_materialized_mutation",
        "#[cfg(test)]\npub(crate) fn stage5d_apply_validated_materialized_riskgate_for_restart(",
        "source_current_shadow_lifecycle_overwrite",
    ]
    if any(token in stage5d_source for token in forbidden_boundary_tokens):
        failures.append("Stage 5D final r3 current-shadow production materialized apply proof missing")
    current_shadow_fn = "fn stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix()"
    current_shadow_start = stage5d_source.find(current_shadow_fn)
    current_shadow_end = stage5d_source.find(
        "#[test]",
        current_shadow_start + len(current_shadow_fn),
    )
    current_shadow_body = (
        stage5d_source[current_shadow_start:current_shadow_end]
        if current_shadow_start >= 0 and current_shadow_end >= 0
        else ""
    )
    if (
        "stage5d_test_r3_current_shadow_full_restart_with_stage5c_continuation(" not in current_shadow_body
        or "current_shadow_cases_executed += 1" not in current_shadow_body
    ):
        failures.append("Stage 5D final r3 current-shadow full-path proof missing")
    required_current_shadow_body_tokens = [
        "expected_trade_count,",
        '"stage5d-final-r3-current-shadow-r1-long"',
        '"stage5d-final-r3-current-shadow-r1-short"',
        '"stage5d-final-r3-current-shadow-r1-realized-pnl"',
        '"1.199999999999997"',
        'assert_eq!(current_shadow_cases_executed, 3);',
    ]
    if any(token not in current_shadow_body for token in required_current_shadow_body_tokens):
        failures.append("Stage 5D final r3 current-shadow full-path proof missing")
    if current_shadow_body.count('"0.0"') < 2 or '"-0.0"' in current_shadow_body:
        failures.append("Stage 5D final r3 current-shadow full-path proof missing")
    for forbidden in [
        "stage5d_test_canonical_package_full_restart_with_stage5c_continuation(",
        "stage5d_test_set_position_side(",
        "direct_current_shadow_mutation_substitution",
    ]:
        if forbidden in current_shadow_body:
            failures.append("Stage 5D final r3 current-shadow direct mutation guard missing")
    required_current_shadow_tokens = [
        "current_shadow_cases_executed_3",
        "current_shadow_long_short_realized_pnl_source_callbacks",
        "exact_current_shadow_source_state_before_correction",
        "current_shadow_field_level_mismatch_localized",
        "current_shadow_field_level_mismatch_fields_4",
        "owning_layer_stage5d_materialized_apply_boundary",
        "approved_current_shadow_materialized_apply_boundary_before_injection",
        "current_shadow_stale_package_export_rejected_before_commit",
        "current_shadow_no_committed_strict_package_then_materialized_mismatch",
        "production_materialized_apply_cases_executed_6",
        "strict_package_decode_used_for_current_shadow",
        "current_shadow_source_runtime_destroyed_before_restart_boundary",
        "current_shadow_fresh_runtime_used",
        "current_shadow_exact_post_apply_state_equality_checked",
        "current_shadow_stage5c_continuation_executed",
        "accepted_executable_count_10",
        "todo_source_produced_count_11",
    ]
    for token in required_current_shadow_tokens:
        if token not in stage5d_source:
            failures.append("Stage 5D final r3 current-shadow r1 marker proof missing")
    operational_fn = "fn stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix()"
    operational_start = stage5d_source.find(operational_fn)
    operational_end = stage5d_source.find(
        "#[test]",
        operational_start + len(operational_fn),
    )
    operational_body = (
        stage5d_source[operational_start:operational_end]
        if operational_start >= 0 and operational_end >= 0
        else ""
    )
    required_operational_body_tokens = [
        "Stage5dR3OperationalStateCase::PartialEntry",
        "Stage5dR3OperationalStateCase::PendingExit",
        "Stage5dR3OperationalStateCase::DeferredEntry",
        "Stage5dR3OperationalStateCase::DeferredExit",
        "Stage5dR3OperationalStateCase::SafeModeCloseOnly",
        "stage5d_test_r3_operational_source_full_restart(case)",
        "assert_eq!(executed, 5);",
    ]
    if any(token not in operational_body for token in required_operational_body_tokens):
        failures.append("Stage 5D final r3 operational-state full-path proof missing")
    for forbidden in [
        "flat_persisted_fixture()",
        "stage5d_test_canonical_package_full_restart_with_stage5c_continuation(",
        "direct_operational_state_mutation_substitution",
    ]:
        if forbidden in operational_body:
            failures.append("Stage 5D final r3 operational-state direct mutation guard missing")
    required_operational_tokens = [
        "operational_state_cases_executed_5",
        "operational_state_partial_entry_actual_callbacks",
        "operational_state_pending_exit_actual_callbacks",
        "operational_state_deferred_entry_actual_callbacks",
        "operational_state_deferred_exit_actual_callbacks",
        "operational_state_safe_mode_actual_callbacks",
        "operational_state_no_direct_strategy_state_mutation_as_producer",
        "operational_state_source_runtime_destroyed_before_restart_boundary",
        "strict_package_decode_used_for_operational_state",
        "operational_state_fresh_runtime_used",
        "operational_state_private_apply_before_bootstrap",
        "operational_state_broker_truth_exact_qty_checked",
        "operational_state_lifecycle_request_cycle_timestamp_equality_checked",
        "operational_state_partial_timer_quantity_evidence_checked",
        "operational_state_pending_exit_duplicate_suppression_checked",
        "operational_state_deferred_entry_stop_take_reason_checked",
        "operational_state_deferred_exit_close_only_semantics_checked",
        "operational_state_safe_mode_entry_block_checked",
        "operational_state_stage5c_continuation_executed",
        "operational_state_post_restored_behavior_probe_executed",
        "operational_state_probe_uses_restored_runtime_not_source_runtime",
        "operational_state_partial_post_restore_no_duplicate_callback",
        "operational_state_partial_timeout_residual_quantity_assertion",
        "operational_state_pending_exit_post_restore_duplicate_trigger_callback",
        "operational_state_pending_exit_request_id_equality_assertion",
        "operational_state_deferred_entry_post_restore_gated_callback",
        "operational_state_deferred_entry_one_time_reissue_assertion",
        "operational_state_deferred_exit_post_restore_no_entry_callback",
        "operational_state_deferred_exit_close_only_reissue_assertion",
        "operational_state_safe_mode_post_restore_entry_attempt_callback",
        "operational_state_safe_mode_repair_path_assertion",
        "accepted_executable_count_15",
        "todo_source_produced_count_6",
        "stage5d_test_source_operational_state_strategy",
        "stage5d_test_r3_operational_source_full_restart",
        "stage5d_test_assert_restored_operational_behavior",
        "let (mut probe, receipt) = restored.into_parts();",
        "Stage5cRuntimeStateRestoredPaperStrategy::stage5d_test_restored_from_parts",
        "probe.on_bar(",
        "probe.on_timer(",
        "probe.on_ack(",
        "stage5d_apply_runtime_private_extension(bound)",
        "stage5d_notify_broker_truth_bootstrap_at(applied, strict_envelope.persisted_at_ts_utc)",
    ]
    for token in required_operational_tokens:
        if token not in stage5d_source:
            failures.append("Stage 5D final r3 operational-state r1 marker proof missing")
    restore_idx = stage5d_source.find("stage5d_test_assert_injected_restores_indexes_once(")
    probe_idx = stage5d_source.find(
        "stage5d_test_assert_restored_operational_behavior(case, restored, &strict_envelope)"
    )
    warmup_idx = stage5d_source.find("stage5d_test_warmup_stage5c_history_at(", probe_idx)
    if not (restore_idx >= 0 and probe_idx > restore_idx and warmup_idx > probe_idx):
        failures.append("Stage 5D final r3 operational-state post-restored probe ordering invalid")
    helper_fn = "fn stage5d_test_assert_restored_operational_behavior("
    helper_start = stage5d_source.find(helper_fn)
    helper_end = stage5d_source.find(
        "fn stage5d_test_r3_operational_source_full_restart",
        helper_start + len(helper_fn),
    )
    helper_body = (
        stage5d_source[helper_start:helper_end]
        if helper_start >= 0 and helper_end >= 0
        else ""
    )
    helper_required = [
        "restored.into_parts()",
        "PartialEntry =>",
        "PendingExit =>",
        "DeferredEntry =>",
        "DeferredExit =>",
        "SafeModeCloseOnly =>",
        "ordinary.is_empty()",
        "partial entry timeout after restore must emit exactly one exit repair",
        "repeated.is_empty()",
        "blocked.is_empty()",
        "deferred entry post-restore eligible callback must reissue exactly one entry",
        "stage5d_test_assert_no_entry_intents(&blocked, \"deferred exit gated callback\")",
        "repair_ack.is_empty()",
        "blocked_entry.is_empty()",
    ]
    for token in helper_required:
        if token not in helper_body:
            failures.append("Stage 5D final r3 operational-state post-restored behavior proof missing")

    recovery_fn = "fn stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix()"
    recovery_start = stage5d_source.find(recovery_fn)
    recovery_end = stage5d_source.find(
        "#[test]",
        recovery_start + len(recovery_fn),
    )
    recovery_body = (
        stage5d_source[recovery_start:recovery_end]
        if recovery_start >= 0 and recovery_end >= 0
        else ""
    )
    if not recovery_body:
        failures.append("Stage 5D final r3 recovery-index r1 matrix missing")
    required_recovery_tokens = [
        "recovery_index_cases_executed_3",
        "recovery_index_known_order_source_order_event",
        "recovery_index_pending_request_source_callback",
        "recovery_index_working_protective_source_order_event",
        "recovery_index_r1r1_production_working_set_bootstrap_used",
        "recovery_index_r1r1_production_owned_id_normalization_used",
        "recovery_index_r1r1_working_stop_truth_source_produced",
        "recovery_index_r1r1_negative_matrix_executed",
        "recovery_index_r1r1_pending_request_field_level_assertions",
        "recovery_index_r1r1_tp_sl_swap_fails_closed",
        "recovery_index_r1r2_unbroken_type_state_path",
        "recovery_index_r1r2_production_working_set_transition_executed",
        "recovery_index_r1r2_validated_stop_truth_roundtrip",
        "recovery_index_r1r2_sl_duplicate_suppressed",
        "recovery_index_r1r2_sl_terminal_no_entry_or_flip",
        "recovery_index_r1r3_executed_metric_markers",
        "recovery_index_r1r3_normalization_block_capability_preserved",
        "recovery_index_r1r3_normalization_retry_clears_broker_owned_ids",
        "recovery_index_canonical_strict_decode_used",
        "recovery_index_source_runtime_destroyed_before_restart_boundary",
        "recovery_index_fresh_runtime_used",
        "recovery_index_exact_post_apply_equality_checked",
        "recovery_index_duplicate_suppression_after_restore",
        "recovery_index_stage5c_continuation_executed",
        "accepted_executable_count_18",
        "todo_source_produced_count_3",
        "stage5d_test_source_known_order_strategy",
        "stage5d_test_source_working_protective_strategy",
        "stage5d_test_r3_recovery_index_source_full_restart",
        "stage5d_test_assert_restored_recovery_index_behavior",
        "stage5d_test_bootstrap_expected_working_order_exact_at",
        "source runtime must recognize known order id through on_order",
        "source export must derive expected working set from runtime working order",
        "Stage5dCanonicalRestartPackage::from_json_str_strict(&package_json)",
        "drop(source_strategy);",
        "restore_semantic_state(&mut fresh_strategy, &strict_envelope)",
        "stage5d_apply_runtime_private_extension(bound)",
        "stage5d_notify_working_set_broker_truth_bootstrap_at(",
        "stage5d_normalize_broker_owned_ids_for_closed_restore_bridge",
        "broker truth must contain the expected working order before Stage 5C closed-boundary working-order bootstrap",
        "production broker-owned-ID normalization",
        "stage5d_test_assert_injected_restores_indexes_once(",
        "let (mut probe, receipt) = restored.into_parts();",
        "duplicate replay after restore must not emit intents",
        "terminal resolution must not leave orphan pending request in runtime state",
        "stage5d_final_r3_recovery_index_r1r1_working_set_negatives_fail_closed",
        "stage5d_final_r3_recovery_index_r1r3_normalization_block_retains_capability",
        "protective terminal callback must not emit entry or flip intent",
        "stage5d_test_warmup_stage5c_history_at(",
    ]
    for token in required_recovery_tokens:
        if token not in stage5d_source:
            failures.append("Stage 5D final r3 recovery-index r1 marker/code proof missing")
    bootstrap_helper = extract_fn_body(
        stage5d_source, "fn stage5d_test_bootstrap_expected_working_order_exact_at"
    )
    if not bootstrap_helper:
        failures.append("Stage 5D final r3 recovery-index bootstrap helper missing")
    else:
        forbidden_after_private_apply = [
            "stage5d_into_parts(",
            "stage5d_test_loaded_from_parts(",
            "Stage5dPrivateStateAppliedPaperStrategy {",
        ]
        for token in forbidden_after_private_apply:
            if token in bootstrap_helper:
                failures.append("Stage 5D final r3 recovery-index unbroken type-state path violated")
        for token in [
            "stage5d_validate_supplemental_working_stop_truth(",
            "serde_json::to_string(&stop_truth)",
            "serde_json::from_str(&stop_truth_json)",
            "stage5d_notify_working_set_broker_truth_bootstrap_at(",
        ]:
            if token not in bootstrap_helper:
                failures.append("Stage 5D final r3 recovery-index validated working-set call path missing")
    if "pub(crate) fn stage5d_notify_working_set_broker_truth_bootstrap_at(" not in stage5d_source:
        failures.append("Stage 5D final r3 recovery-index working-set coordinator must remain crate-visible")
    restored_path = extract_fn_body(stage5d_source, "fn stage5d_notify_runtime_state_restored_at")
    if not restored_path:
        failures.append("Stage 5D final r3 recovery-index restored path missing")
    else:
        for token in [
            "stage5d_normalize_broker_owned_ids_for_closed_restore_bridge(",
            "Err(blocked)",
            "bootstrapped: *blocked.bootstrapped",
            "Stage5dRuntimeStateRestoreBlockedReason::BrokerOwnedProtectiveId",
        ]:
            if token not in restored_path:
                failures.append("Stage 5D final r3 recovery-index production normalization retention missing")
    host_source = (root / STAGE5C_HOST_REL).read_text()
    normalize_body = extract_fn_body(
        host_source, "pub(crate) fn stage5d_normalize_broker_owned_ids_for_closed_restore_bridge"
    )
    if not normalize_body:
        failures.append("Stage 5D final r3 recovery-index broker-owned normalization bridge missing")
    else:
        for token in [
            "pub(crate) bootstrapped: Box<Stage5cBootstrappedPaperStrategy>",
            "Stage5dBrokerOwnedIdNormalizationBlocked",
            "bootstrapped: Box::new(Stage5cBootstrappedPaperStrategy",
            "BrokerOwnedOrderIdMismatch",
        ]:
            if token not in host_source:
                failures.append("Stage 5D final r3 recovery-index normalization retained capability missing")
        guard_index = normalize_body.find("if !tp_is_frozen || !sl_stop_is_frozen || !sl_exchange_is_frozen")
        clear_index = normalize_body.find("*tp_order_id = None;")
        if guard_index < 0 or clear_index < 0 or guard_index > clear_index:
            failures.append("Stage 5D final r3 recovery-index normalization partial mutation guard missing")
    behavior_helper = extract_fn_body(
        stage5d_source, "fn stage5d_test_assert_restored_recovery_index_behavior"
    )
    if not behavior_helper:
        failures.append("Stage 5D final r3 recovery-index restored behavior helper missing")
    else:
        if "let duplicate_sl = duplicate.clone();" in behavior_helper or "let terminal_sl = terminal.clone();" in behavior_helper:
            failures.append("Stage 5D final r3 recovery-index actual SL callbacks substituted")
        for token in [
            "duplicate SL callback must not emit protection twice",
            "protective SL terminal callback must not emit entry or flip intent",
            "expected_working_stop_order_ids",
            "working protective duplicate SL callback must preserve exact expected SL set",
            "restored-before-terminal pending entry",
            "let duplicate = probe.on_order(",
            "let duplicate_sl = probe.on_stop_order(",
            "stop_order_id: source_stop_order_id.clone()",
            "status: \"working\".to_string()",
            "assert!(\n                    duplicate_sl.is_empty(),",
            "let terminal = probe.on_order(",
            "let terminal_sl = probe.on_stop_order(",
            "status: \"canceled\".to_string()",
            "stage5d_test_assert_no_entry_intents(\n                    &terminal_sl,",
        ]:
            if token not in behavior_helper:
                failures.append("Stage 5D final r3 recovery-index SL restored behavior proof missing")
    normalization_test = extract_fn_body(
        stage5d_source,
        "fn stage5d_final_r3_recovery_index_r1r3_normalization_block_retains_capability",
    )
    if not normalization_test:
        failures.append("Stage 5D final r3 recovery-index normalization ownership proof missing")
    else:
        for token in [
            "stage5d_normalize_broker_owned_ids_for_closed_restore_bridge(",
            "panic!(\"mismatched SL set must block without consuming the retained capability\")",
            "stage5d_test_strategy_state_fingerprint(blocked.bootstrapped.stage5d_strategy())",
            "*blocked.bootstrapped",
            "panic!(\"retained capability must retry successfully with exact TP/SL sets\")",
            "successful retry must clear broker-owned TP id",
            "successful retry must clear broker-owned SL id",
            "STAGE5D_RECOVERY_R1R3 normalization_block_retained=true",
        ]:
            if token not in normalization_test:
                failures.append("Stage 5D final r3 recovery-index normalization ownership proof missing")
    gate_source = (root / "scripts/stage5d_final_restart_r3_recovery_index_r1_gate.sh").read_text()
    for hardcoded in [
        'print("unbroken_type_state_path=true")',
        'print("production_working_set_transition_executed=true")',
        'print("validated_stop_truth_roundtrip=true")',
        'print("tp_duplicate_suppressed=true")',
        'print("sl_duplicate_suppressed=true")',
        'print("tp_terminal_no_entry_or_flip=true")',
        'print("sl_terminal_no_entry_or_flip=true")',
        'print("pending_terminal_no_orphan=true")',
    ]:
        if hardcoded in gate_source:
            failures.append("Stage 5D final r3 recovery-index focused gate uses hard-coded behavioral boolean")
    for token in [
        "tee \"$focused_log\"",
        "require_marker \"STAGE5D_RECOVERY_R1R3 unbroken_type_state_path=true\"",
        "require_marker \"STAGE5D_RECOVERY_R1R3 production_working_set_transition=true\"",
        "require_marker \"STAGE5D_RECOVERY_R1R3 validated_stop_truth_roundtrip=true\"",
        "require_marker \"STAGE5D_RECOVERY_R1R3 normalization_block_retained=true\"",
        "focused_log_sha256=",
        "negative_cases=",
    ]:
        if token not in gate_source:
            failures.append("Stage 5D final r3 recovery-index focused gate evidence binding missing")
    if "accepted_r3_recovery_index_r1_source_produced" not in stage5d_source:
        failures.append("Stage 5D final r3 recovery-index inventory acceptance proof missing")


def validate_stage5d_b2bd1_runtime_restored_semantic_guards(
    root: Path, failures: list[str]
) -> None:
    host_source = (root / STAGE5C_HOST_REL).read_text()
    intent_guard = (
        "if !intents.is_empty() {\n"
        "        return Err(Stage5dRuntimeStateRestoredBridgeError::CallbackEmittedIntent);"
    )
    debug_assert_guard = "debug_assert!(intents.is_empty());"
    if intent_guard not in host_source:
        failures.append("Stage 5D runtime-restored intent runtime guard missing")
    if debug_assert_guard in host_source and host_source.find(intent_guard) > host_source.find(
        debug_assert_guard
    ):
        failures.append("Stage 5D runtime-restored intent runtime guard must precede debug_assert")
    if (
        "stage5d_validate_post_runtime_restored_broker_truth_exact(&strategy, admission)?"
        not in host_source
    ):
        failures.append("Stage 5D runtime-restored exact post-callback broker-truth guard missing")
    post_guard_start = host_source.find(
        "fn stage5d_validate_post_runtime_restored_broker_truth_exact("
    )
    post_guard_end = host_source.find(
        "\npub(crate) fn stage5d_bootstrap_preserving_loaded_at", post_guard_start
    )
    post_guard_source = (
        host_source[post_guard_start:post_guard_end]
        if post_guard_start >= 0 and post_guard_end > post_guard_start
        else ""
    )
    if (
        "STAGE5D_RUNTIME_RESTORED_CALLBACK_COUNT.with(|count| count.set(count.get() + 1));"
        not in host_source
    ):
        failures.append("Stage 5D runtime-restored callback-count proof hook missing")
    if "if (*last_position_qty - broker_qty).abs() > f64::EPSILON {" not in post_guard_source:
        failures.append("Stage 5D runtime-restored post-callback position guard missing")
    if "if *current_side != expected_side {" not in post_guard_source:
        failures.append("Stage 5D runtime-restored post-callback side guard missing")
    if (
        "if tp_order_id.is_some() || sl_stop_order_id.is_some() || sl_exchange_order_id.is_some() {"
        not in post_guard_source
    ):
        failures.append("Stage 5D runtime-restored post-callback protective-id guard missing")

    stage5d_source = (root / STAGE5D_REL).read_text()
    preflight_source = source_function_slice(
        stage5d_source, "validate_stage5d_runtime_state_restored_preflight"
    )
    if stage5d_source.count(
        "validate_stage5d_runtime_state_restored_preflight(&injected, restored_at)"
    ) < 3:
        failures.append("Stage 5D runtime-restored preflight invocation missing")
    if "if !injected.recovery_plan.recovery_complete" not in stage5d_source:
        failures.append("Stage 5D runtime-restored recovery-complete guard missing")
    if (
        "if !injected\n"
        "        .envelope\n"
        "        .runtime_private_extension\n"
        "        .runtime_pending_finalizations\n"
        "        .is_empty()"
        not in stage5d_source
    ):
        failures.append("Stage 5D runtime-restored pending-finalization guard missing")
    if "if expected_plan != injected.recovery_plan.plan_fingerprint_sha256" not in stage5d_source:
        failures.append("Stage 5D runtime-restored recovery-plan binding guard missing")
    if "if injected.bootstrapped.stage5d_restored().known_order_ids" not in stage5d_source:
        failures.append("Stage 5D runtime-restored recovery-index guard missing")
    if "admission.runtime_host_attached()" not in preflight_source:
        failures.append("Stage 5D runtime-restored closed-boundary guard missing")
    if "injected: Box<Stage5dRiskGateInjectedPaperStrategy>" not in stage5d_source:
        failures.append("Stage 5D runtime-restored blocked retained capability missing")
    if "pub fn retry_capability_available(&self) -> bool {\n        false" not in stage5d_source:
        failures.append("Stage 5D runtime-restored terminal retry denial missing")
    if "bootstrap_notified_at <= restored_at" not in stage5d_source:
        failures.append("Stage 5D runtime-restored lifecycle notification timestamp guard missing")
    if "if *current_side != expected_side {" not in stage5d_source:
        failures.append("Stage 5D runtime-restored flat-side exact guard missing")
    if "stage5d_test_closed_boundary_flags()" not in stage5d_source:
        failures.append("Stage 5D runtime-restored r6 closed-boundary retained flags proof missing")
    required_r3_tokens = {
        "Stage 5D runtime-restored source-produced current-shadow proof missing":
            "stage5d_b2bd1r3_source_produced_current_shadow_long_short_and_realized_pnl_restore",
        "Stage 5D runtime-restored single-row recovery transition proof missing":
            "completed single-row recovery must reach restored transition",
        "Stage 5D runtime-restored multi-row recovery transition proof missing":
            "completed multi-row recovery must reach restored transition",
        "Stage 5D runtime-restored blocked strategy fingerprint proof missing":
            "blocked.stage5d_test_strategy_state_fingerprint()",
        "Stage 5D runtime-restored source pre-bind exact-state proof missing":
            "positive path must use exact source semantic state before Stage 5D binding",
        "Stage 5D runtime-restored compile-fail construction proof missing":
            "let forged = Stage5dRiskGateInjectedPaperStrategy {};",
        "Stage 5D runtime-restored compile-fail consumed-input proof missing":
            "let _second = stage5d_notify_runtime_state_restored(injected);",
        "Stage 5D runtime-restored compile-fail restored-to-injected proof missing":
            "Stage5dRiskGateInjectedPaperStrategy = restored;",
        "Stage 5D runtime-restored compile-fail blocked-terminal proof missing":
            "Stage5dRuntimeStateRestoreOutcome::Terminal(_terminal) = outcome",
        "Stage 5D runtime-restored compile-fail private preflight proof missing":
            "use strategy_runtime_core::stage5d_persistence::validate_stage5d_runtime_state_restored_preflight;",
        "Stage 5D runtime-restored compile-fail private field proof missing":
            "let _raw_bootstrapped = injected.bootstrapped;",
        "Stage 5D runtime-restored compile-fail private bridge proof missing":
            "use strategy_runtime_core::stage5c_paper_host::stage5d_notify_runtime_state_restored_bridge_at;",
        "Stage 5D runtime-restored actual Long broker-position proof missing":
            "(3.0, \"long\", crate::hybrid_intraday::Side::Long)",
        "Stage 5D runtime-restored actual Short broker-position proof missing":
            "(-3.0, \"short\", crate::hybrid_intraday::Side::Short)",
        "Stage 5D runtime-restored genuine broker-position row proof missing":
            "r4 actual broker-position positive must use a genuine broker position row",
        "Stage 5D runtime-restored known-order preservation proof missing":
            "r4 non-empty known-order index must be preserved",
        "Stage 5D runtime-restored pending-request preservation proof missing":
            "r4 non-empty pending-request index must be preserved",
        "Stage 5D runtime-restored receipt known-order retention proof missing":
            "stage5d_test_known_order_ids()",
        "Stage 5D runtime-restored open-position side-mismatch proof missing":
            "r4 open broker position Long/Short side mismatch must block before callback",
        "Stage 5D runtime-restored strict round-trip helper missing":
            "riskgate_enabled_strict_bootstrapped_fixture_with_evidence",
        "Stage 5D runtime-restored strict broker-position proof missing":
            "r5 strict JSON round-trip broker-position",
        "Stage 5D runtime-restored strict known-order proof missing":
            "r5 strict JSON round-trip known-order index evidence",
        "Stage 5D runtime-restored strict pending-request proof missing":
            "r5 strict JSON round-trip pending-request index evidence",
        "Stage 5D runtime-restored paper-only blocker proof missing":
            "not_paper_only_boundary",
        "Stage 5D runtime-restored non-ack recovery decision proof missing":
            "non_acknowledged_recovery_decision",
        "Stage 5D runtime-restored strict Long proof missing":
            "r6 strict JSON round-trip actual Long broker-position evidence",
        "Stage 5D runtime-restored strict Short proof missing":
            "r6 strict JSON round-trip actual Short broker-position evidence",
        "Stage 5D runtime-restored r6 strict known-order proof missing":
            "r6 strict JSON round-trip known-order index evidence",
        "Stage 5D runtime-restored r6 strict pending-request proof missing":
            "r6 strict JSON round-trip pending-request index evidence",
        "Stage 5D runtime-restored r6 common blocked helper proof missing":
            "r6 representable blockers use common callback-zero helper",
        "Stage 5D runtime-restored r6 malformed payload proof missing":
            "stage5d_b2bd1r6_strict_malformed_payload_shapes_fail_closed",
        "Stage 5D runtime-restored r6 earlier strategy/account/instrument proof missing":
            "stage5d_b2bd1r6_earlier_gate_rejects_strategy_account_instrument_mismatches",
        "Stage 5D runtime-restored r6 config/profile proof missing":
            "stage5d_b2bd1r6_earlier_gate_rejects_config_and_profile_mismatches",
    }
    for message, token in required_r3_tokens.items():
        if token not in stage5d_source:
            failures.append(message)
    for message, token in {
        "Stage 5D final canonical export production surface missing":
            "stage5d_export_canonical_envelope_from_runtime",
        "Stage 5D final canonical package production surface missing":
            "stage5d_export_canonical_restart_package_from_runtime",
        "Stage 5D final package strict decode proof missing":
            "Stage5dCanonicalRestartPackage::from_json_str_strict",
        "Stage 5D final canonical restart matrix proof missing":
            "stage5d_final_canonical_export_restart_matrix_flat_long_short",
        "Stage 5D final post-export mutation rejection proof missing":
            "stage5d_final_canonical_export_rejects_post_export_mutation_at_restart_boundary",
        "Stage 5D final recovery-index binding proof missing":
            "stage5d_final_canonical_export_binds_recovery_indexes_from_source_state",
        "Stage 5D final package corruption proof missing":
            "stage5d_final_restart_package_rejects_evidence_and_package_corruption",
        "Stage 5D final clean-process poison proof missing":
            "stage5d_final_clean_process_restart_does_not_reuse_poisoned_source_runtime",
        "Stage 5D final r2 positive full-matrix proof missing":
            "stage5d_final_r2_package_positive_full_matrix_and_stage5c_continuation",
        "Stage 5D final r2 source-callback proof missing":
            "stage5d_final_r2_package_source_callback_current_shadow_matrix",
        "Stage 5D final r2 crash-store replay proof missing":
            "stage5d_final_r2_package_crash_store_replay_matrix",
        "Stage 5D final r2 package negative proof missing":
            "stage5d_final_r2_package_negative_matrix_fails_closed",
        "Stage 5D final r2 golden-vector proof missing":
            "stage5d_final_r2_package_golden_vectors_are_pinned_and_deterministic",
        "Stage 5D final r2 Stage 5C warmup continuation proof missing":
            "r2 Stage 5C history warmup continuation must succeed",
        "Stage 5D final r2 full package validation proof missing":
            "self.validate_full_contract()?;",
        "Stage 5D final r3a source-pending full restart proof missing":
            "stage5d_final_r3a_source_pending_entry_full_restart_matrix",
        "Stage 5D final r3a source-pending negative proof missing":
            "stage5d_final_r3a_source_pending_package_negatives_fail_closed",
        "Stage 5D final r3a exact private apply before bootstrap/callback proof missing":
            "private apply must restore exact pending shape before broker bootstrap and restored callback",
        "Stage 5D final r3a actual semantic post-apply equality proof missing":
            "actual fresh Strategy::state after private apply must preserve exact semantic pending-entry field",
        "Stage 5D final r3a actual private DTO equality proof missing":
            "actual private partial-entry timer after private apply must equal source",
        "Stage 5D final r3a non-partial timer absence proof missing":
            "r3a non-partial source and restored partial-entry timers must both be absent",
        "Stage 5D final r3a source correction not required proof missing":
            "raw semantic set_state placeholder must not be treated as final restored shape",
        "Stage 5D final r3a MR Long source case missing":
            "stage5d-final-r3a-source-mr-long-pending-entry",
        "Stage 5D final r3a MR Short source case missing":
            "stage5d-final-r3a-source-mr-short-pending-entry",
        "Stage 5D final r3a BO Long source case missing":
            "stage5d-final-r3a-source-bo-long-pending-entry",
        "Stage 5D final r3a BO Short source case missing":
            "stage5d-final-r3a-source-bo-short-pending-entry",
        "Stage 5D final r3a MR bracket mapping proof missing":
            "Self::MrLong | Self::MrShort => Stage5dEntryStyle::Bracket",
        "Stage 5D final r3a BO market mapping proof missing":
            "Self::BoLong | Self::BoShort => Stage5dEntryStyle::Market",
        "Stage 5D final r3a MR Long reason mapping proof missing":
            "Self::MrLong => Stage5dLifecycleReason::MorningMeanReversionLong",
        "Stage 5D final r3a MR Short reason mapping proof missing":
            "Self::MrShort => Stage5dLifecycleReason::MorningMeanReversionShort",
        "Stage 5D final r3a BO Long reason mapping proof missing":
            "Self::BoLong => Stage5dLifecycleReason::BreakoutLong",
        "Stage 5D final r3a BO Short reason mapping proof missing":
            "Self::BoShort => Stage5dLifecycleReason::BreakoutShort",
        "Stage 5D final r3a fail-closed MR missing stop/take proof missing":
            "incomplete MR stop/take must fail closed after canonical package decode",
        "Stage 5D final r3a fail-closed owner/side/reason mismatch proof missing":
            "owner/side/reason mismatch must fail closed after canonical package decode",
        "Stage 5D final r3a MR stop/take shape assertion missing":
            "entry.stop_price.is_some() && entry.take_price.is_some()",
        "Stage 5D final r3 resumption inventory proof missing":
            "stage5d_final_r3_resumption_inventory_and_r3a_r1_reuse",
        "Stage 5D final r3 mandatory positive count proof missing":
            "mandatory_positive_count_21",
        "Stage 5D final r3 r3a-r1 reuse proof missing":
            "r3a_r1_source_pending_reused",
        "Stage 5D final r3 schema-only overclaim guard missing":
            "no_schema_only_positive_overclaim",
        "Stage 5D final r3 accepted executable count proof missing":
            "accepted_executable_count_10",
        "Stage 5D final r3 TODO count proof missing":
            "todo_source_produced_count_11",
        "Stage 5D final r3 accepted execution count proof missing":
            "accepted_cases_executed_4",
        "Stage 5D final r3 positive-core source-produced proof missing":
            "stage5d_final_r3_positive_core_source_produced_full_restart_matrix",
        "Stage 5D final r3 positive-core accepted count proof missing":
            "positive_core_accepted_count_3",
        "Stage 5D final r3 positive-core clean flat package proof missing":
            "positive_core_clean_flat_actual_source_lifecycle",
        "Stage 5D final r3 positive-core open position package proof missing":
            "positive_core_broker_open_long_short_actual_source_lifecycle",
        "Stage 5D final r3 positive-core current-shadow package proof missing":
            "current_shadow_first_mismatch_materialized_riskgate_state",
        "Stage 5D final r3 positive-core execution count proof missing":
            "positive_core_accepted_cases_executed_3",
        "Stage 5D final r3 positive-core actual source execution proof missing":
            "actual_source_core_cases_executed_3",
        "Stage 5D final r3 current-shadow discovery execution proof missing":
            "current_shadow_discovery_cases_executed_3",
        "Stage 5D final r3 current-shadow full-path proof missing":
            "stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix",
        "Stage 5D final r3 current-shadow materialized apply proof missing":
            "approved_current_shadow_materialized_apply_boundary_before_injection",
        "Stage 5D final r3 TODO owner guard missing":
            "no_todo_owning_test",
        "Stage 5D final r3 Stage 5E closed marker missing":
            "stage5e_closed",
    }.items():
        if token not in stage5d_source:
            failures.append(message)
    required_order = [
        "        let applied = expect_stage5d_ok(\n            stage5d_apply_runtime_private_extension(bound)",
        "        stage5d_test_assert_r3a_actual_post_apply_semantic_equality(",
        "private apply must restore exact pending shape before broker bootstrap and restored callback",
        "            stage5d_notify_broker_truth_bootstrap_at(applied",
        "&format!(\"{case:?}: r3a source pending riskgate injection must succeed\")",
        "        let restored = stage5d_test_assert_injected_restores_indexes_once(",
    ]
    positions = [stage5d_source.find(token) for token in required_order]
    if any(position < 0 for position in positions):
        failures.append("Stage 5D final r3a apply/bootstrap/callback ordering proof missing")
    elif positions != sorted(positions):
        failures.append("Stage 5D final r3a restored callback moved before private apply")
    r5_summary = root / "docs/stage-5/5d-b2b-d1-r5-review-gate-summary.md"
    if not r5_summary.exists():
        failures.append("Stage 5D runtime-restored r5 ownership summary missing")
    else:
        r5_summary_source = r5_summary.read_text()
        for message, token in {
            "Stage 5D runtime-restored blocker ownership table missing":
                "Stage 5D-b2b-d1-r5 blocker ownership table",
            "Stage 5D runtime-restored quantity ownership proof missing":
                "BrokerQuantityNotRepresentable",
            "Stage 5D runtime-restored earlier-stage ownership proof missing":
                "owned before b2b-d",
        }.items():
            if token not in r5_summary_source:
                failures.append(message)
    validate_stage5d_runtime_restored_ownership_inventory(root, stage5d_source, failures)
    validate_stage5d_final_restart_inventory(root, stage5d_source, failures)
    validate_stage5d_final_restart_r3_inventory(root, failures)


def validate_stage5d_final_r3_riskgate_recovery_r1(
    root: Path, failures: list[str]
) -> None:
    stage5d_source = (root / STAGE5D_REL).read_text()
    test_body = extract_fn_body(
        stage5d_source,
        "fn stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix",
    )
    if not test_body:
        failures.append("Stage 5D final r3 riskgate-recovery r1 matrix missing")
        return
    required_tokens = [
        "riskrec_source_finalization_producer_entrypoint",
        "riskrec_runtime_pending_created_by_source_lifecycle",
        "riskrec_durable_outbox_created_by_canonical_export_input",
        "riskrec_single_row_equality_runtime_outbox_ledger_plan",
        "riskrec_multi_row_stable_order_assertions",
        "riskrec_complete_plan_noop_from_final_checkpoint",
        "riskrec_checkpoint_restart_matrix_executed",
        "riskrec_store_state_matrix_executed",
        "riskrec_source_runtime_destroyed_before_decode",
        "riskrec_strict_decode_fresh_runtime_used",
        "riskrec_callback_exactly_once_no_intents",
        "riskrec_idempotent_replay_verified",
        "riskrec_stage5c_continuation_executed",
        "accepted_executable_count_21",
        "todo_source_produced_count_0",
        "stage5d_test_source_runtime_with_real_pending_finalizations(1)",
        "stage5d_test_source_runtime_with_real_pending_finalizations(2)",
        "drop(source_strategy);",
        "Stage5dCanonicalRestartPackage::from_json_str_strict(&package_json)",
        "stage5d_test_bootstrap_strict_envelope_with_strategy(",
        "stage5d_inject_authoritative_riskgate(bootstrapped, validated_evidence)",
        "stage5d_apply_next_riskgate_recovery_action(",
        "Stage5dRiskGateRecoveryReady",
        "Stage5dRiskGateRecoveryCommitReceipt",
        "Stage5dRiskRecBoundedFileStore",
        "Stage5dRiskRecStoreState::PartialWrite",
        "Stage5dRiskRecStoreState::FullWrittenUncommitted",
        "stage5d_riskrec_store_commit_and_reload(",
        "stage5d_riskgate_recovery_ready_from_canonical_package(",
        "Stage5dRiskGateRecoveryCheckpoint::InitialPackageCommitted",
        "Stage5dRiskGateRecoveryCheckpoint::FinalCheckpointCommitted",
        "precommit crash must replay the same not-yet-committed action",
        "postcommit crash must not duplicate the already committed action",
        "stage5d_test_warmup_stage5c_history_at(",
        "\"checkpoint_state\":\"full_written_uncommitted\"",
        "stage5d_riskrec_single_pending_golden.json",
        "stage5d_riskrec_ordered_multi_row_golden.json",
        "stage5d_riskrec_complete_noop_golden.json",
        "STAGE5D_RISKREC production_transition_outside_test=true",
        "STAGE5D_RISKREC source_rows_exact=true",
        "STAGE5D_RISKREC checkpoint_receipts_exact=true",
        "STAGE5D_RISKREC final_receipt_persisted_exactly=true",
        "STAGE5D_RISKREC writer_reader_reopened_each_checkpoint=true",
        "STAGE5D_RISKREC precommit_crash_idempotent=true",
        "STAGE5D_RISKREC postcommit_crash_idempotent=true",
        "STAGE5D_RISKREC production_recovery_actions=true",
        "STAGE5D_RISKREC single_pending_finalization=true",
        "STAGE5D_RISKREC multi_row_ordered=true",
        "STAGE5D_RISKREC complete_plan_noop=true",
        "STAGE5D_RISKREC checkpoint_restart_matrix=true",
        "STAGE5D_RISKREC durable_store_matrix=true",
        "STAGE5D_RISKREC final_checkpoint_committed=true",
        "STAGE5D_RISKREC callback_exactly_once=true",
        "STAGE5D_RISKREC idempotent_replay=true",
        "STAGE5D_RISKREC exact_package_receipt_goldens=true",
        "STAGE5D_RISKREC golden_values_exact=true",
        "STAGE5D_RISKREC stage5c_continuation=true",
        "STAGE5D_RISKREC stage5e_closed=true",
    ]
    for token in required_tokens:
        if token not in stage5d_source:
            failures.append("Stage 5D final r3 riskgate-recovery r1 marker/code proof missing")
    if "HashSet" in test_body:
        failures.append("Stage 5D final r3 riskgate-recovery must not use unordered row proof")
    for rel in [
        "tests/fixtures/stage5/stage5d_riskrec_single_pending_golden.json",
        "tests/fixtures/stage5/stage5d_riskrec_ordered_multi_row_golden.json",
        "tests/fixtures/stage5/stage5d_riskrec_complete_noop_golden.json",
    ]:
        path = root / rel
        if not path.is_file():
            failures.append("Stage 5D final r3 riskgate-recovery golden evidence missing")
            continue
        try:
            golden = json.loads(path.read_text())
        except json.JSONDecodeError:
            failures.append("Stage 5D final r3 riskgate-recovery golden evidence invalid")
            continue
        if golden.get("stage") != "5D-final-restart-r3-riskgate-recovery-r1-r2":
            failures.append("Stage 5D final r3 riskgate-recovery golden stage mismatch")
        if golden.get("golden_kind") == "checked_in_summary":
            failures.append("Stage 5D final r3 riskgate-recovery golden can refresh silently")
        if golden.get("stage5e_closed") is not True:
            failures.append("Stage 5D final r3 riskgate-recovery golden Stage 5E closure missing")
        for key in [
            "package_sha256",
            "envelope_sha256",
            "final_commit_receipt_fingerprint",
        ]:
            value = golden.get(key)
            if not isinstance(value, str) or len(value) != 64:
                failures.append("Stage 5D final r3 riskgate-recovery golden fingerprint missing")
        if not str(golden.get("evidence_fingerprint_sha256", "")).startswith(
            "stage5d_riskgate_evidence_sha256:"
        ):
            failures.append("Stage 5D final r3 riskgate-recovery evidence fingerprint missing")
        if not str(golden.get("recovery_plan_fingerprint_sha256", "")).startswith(
            "stage5d_riskgate_recovery_plan_sha256:"
        ):
            failures.append("Stage 5D final r3 riskgate-recovery plan fingerprint missing")
        if not golden.get("expected_checkpoint_progression"):
            failures.append("Stage 5D final r3 riskgate-recovery checkpoint golden missing")
    gate_path = root / "scripts/stage5d_final_restart_r3_riskgate_recovery_r1_gate.sh"
    if not gate_path.is_file():
        failures.append("Stage 5D final r3 riskgate-recovery focused gate missing")
    else:
        gate_source = gate_path.read_text()
        for marker in [
            "require_marker \"STAGE5D_RISKREC production_transition_outside_test=true\"",
            "require_marker \"STAGE5D_RISKREC source_rows_exact=true\"",
            "require_marker \"STAGE5D_RISKREC checkpoint_receipts_exact=true\"",
            "require_marker \"STAGE5D_RISKREC final_receipt_persisted_exactly=true\"",
            "require_marker \"STAGE5D_RISKREC writer_reader_reopened_each_checkpoint=true\"",
            "require_marker \"STAGE5D_RISKREC precommit_crash_idempotent=true\"",
            "require_marker \"STAGE5D_RISKREC postcommit_crash_idempotent=true\"",
            "require_marker \"STAGE5D_RISKREC production_recovery_actions=true\"",
            "require_marker \"STAGE5D_RISKREC single_pending_finalization=true\"",
            "require_marker \"STAGE5D_RISKREC multi_row_ordered=true\"",
            "require_marker \"STAGE5D_RISKREC complete_plan_noop=true\"",
            "require_marker \"STAGE5D_RISKREC exact_package_receipt_goldens=true\"",
            "require_marker \"STAGE5D_RISKREC final_checkpoint_committed=true\"",
            "require_marker \"STAGE5D_RISKREC golden_values_exact=true\"",
            "accepted_executable_count=21",
            "todo_source_produced_count=0",
            "golden_sha256",
            "negative_cases=",
        ]:
            if marker not in gate_source:
                failures.append("Stage 5D final r3 riskgate-recovery focused gate evidence binding missing")


def validate(root: Path, manifest_path: Path) -> list[str]:
    failures: list[str] = []
    manifest = json.loads(manifest_path.read_text())

    if manifest.get("schema_version") != 1:
        failures.append("schema_version must be 1")
    if manifest.get("stage") != EXPECTED_STAGE:
        failures.append(f"stage must be {EXPECTED_STAGE}")
    if manifest.get("status") != "additive_freeze_candidate":
        failures.append("status must be additive_freeze_candidate")
    if manifest.get("stage5c_closure_baseline") != EXPECTED_STAGE5C_CLOSURE:
        failures.append("Stage 5C closure baseline reference mismatch")
    if manifest.get("manifest_checker") != EXPECTED_MANIFEST_CHECKER:
        failures.append("manifest_checker mismatch")
    if manifest.get("negative_harness") != EXPECTED_NEGATIVE_HARNESS:
        failures.append("negative_harness mismatch")
    forbidden_contract = manifest.get("forbidden_negative_harness_contract")
    if forbidden_contract != EXPECTED_FORBIDDEN_NEGATIVE_HARNESS_CONTRACT:
        failures.append("forbidden negative harness contract mismatch")
    else:
        for path_key, hash_key in (
            ("launcher_path", "launcher_sha256"),
            ("coordinator_path", "coordinator_sha256"),
            ("worker_path", "worker_sha256"),
        ):
            artifact = root / forbidden_contract[path_key]
            if not artifact.is_file() or sha256_file(artifact) != forbidden_contract[hash_key]:
                failures.append(
                    "forbidden negative harness artifact hash mismatch: "
                    f"{forbidden_contract[path_key]}"
                )
        coordinator_source = (root / forbidden_contract["coordinator_path"]).read_text(
            errors="replace"
        )
        worker_source = (root / forbidden_contract["worker_path"]).read_text(errors="replace")
        required_coordinator_tokens = (
            "if clean_result.returncode != 0:",
            "case.expected_marker",
            "declared != implemented",
            "ThreadPoolExecutor",
            "os.killpg",
            "derive_case_timeout_seconds",
            "--self-test-timeout-contract",
            "CI_HEADROOM_SECONDS",
        )
        required_worker_tokens = (
            'grep -F -- "$expected_marker"',
            "infrastructure failure is not valid case evidence",
            "selected case did not execute exactly once",
        )
        if any(token not in coordinator_source for token in required_coordinator_tokens):
            failures.append("forbidden negative harness coordinator contract incomplete")
        if any(token not in worker_source for token in required_worker_tokens):
            failures.append("forbidden negative harness worker contract incomplete")
        if coordinator_source.count("Case(") != forbidden_contract["declared_cases"]:
            failures.append("forbidden negative harness declared case inventory mismatch")
        if worker_source.count("|failure'") != forbidden_contract["negative_cases"]:
            failures.append("forbidden negative harness worker negative inventory mismatch")
        if worker_source.count("|success'") != forbidden_contract["positive_controls"]:
            failures.append("forbidden negative harness worker positive inventory mismatch")
        scanner_source = (root / FORBIDDEN_SCANNER_REL).read_text(errors="replace")
        scanner_marker = (
            'FORBIDDEN_SURFACE_SCANNER_CONTRACT="'
            f'{forbidden_contract["scanner_contract"]}"'
        )
        if scanner_source.count(scanner_marker) != 1:
            failures.append("forbidden scanner contract marker mismatch")
        ci_source = (root / CI_REL).read_text(errors="replace")
        ci_timeout_match = re.search(
            r"- name: Forbidden surface negative harness\s+"
            r"run: bash scripts/forbidden_surface_negative_harness\.sh\s+"
            r"timeout-minutes: (?P<minutes>\d+)",
            ci_source,
        )
        if (
            ci_timeout_match is None
            or int(ci_timeout_match.group("minutes")) < forbidden_contract["ci_timeout_minutes"]
        ):
            failures.append(
                "forbidden negative harness CI timeout is below "
                f"{forbidden_contract['ci_timeout_minutes']} minutes"
            )
    if manifest.get("closed_surfaces") != EXPECTED_CLOSED_SURFACES:
        failures.append("closed_surfaces mismatch")
    if manifest.get("negative_cases") != EXPECTED_NEGATIVE_CASES:
        failures.append("negative_cases mismatch")
    if manifest.get("stage5d_public_symbols") != EXPECTED_STAGE5D_PUBLIC_SYMBOLS:
        failures.append("Stage5d public symbol contract mismatch")
    if (
        manifest.get("stage5c_private_layout_extensions")
        != EXPECTED_STAGE5C_PRIVATE_LAYOUT_EXTENSIONS
    ):
        failures.append("Stage 5C private layout extension contract mismatch")
    approved_private_layout_extensions = {
        extension.get("path"): extension
        for extension in manifest.get("stage5c_private_layout_extensions", [])
        if isinstance(extension, dict)
    }
    if manifest.get("stage5c_compatibility_checker") != EXPECTED_STAGE5C_COMPATIBILITY_CHECKER:
        failures.append("Stage 5C compatibility checker manifest entry mismatch")
    if manifest.get("historical_stage5c_checker") != EXPECTED_HISTORICAL_STAGE5C_CHECKER:
        failures.append("historical Stage 5C checker manifest entry mismatch")
    controlled_extensions = manifest.get("controlled_source_semantic_extensions")
    if controlled_extensions != EXPECTED_CONTROLLED_SOURCE_SEMANTIC_EXTENSIONS:
        failures.append("controlled source semantic extension contract mismatch")
    else:
        closure_hashes_for_extensions = json.loads((root / STAGE5C_MANIFEST_REL).read_text()).get(
            "source_hashes", {}
        )
        source_correspondence_path = (
            root
            / EXPECTED_CONTROLLED_SOURCE_SEMANTIC_EXTENSIONS[0]["source_correspondence_path"]
        )
        if not source_correspondence_path.is_file():
            failures.append("source correspondence ledger missing for controlled extension")
        for extension in controlled_extensions:
            extension_path = root / extension["path"]
            if not extension_path.is_file():
                failures.append(f"controlled source extension file missing: {extension['path']}")
                continue
            actual_hash = sha256_file(extension_path)
            if actual_hash != extension["current_sha256"]:
                failures.append(
                    f"controlled source extension current hash mismatch for {extension['path']}: "
                    f"actual={actual_hash}"
                )
            if closure_hashes_for_extensions.get(extension["path"]) != extension[
                "stage5c_baseline_sha256"
            ]:
                failures.append(
                    f"controlled source extension baseline mismatch for {extension['path']}"
                )
            if extension["source_correspondence_sha256"] != sha256_file(
                source_correspondence_path
            ):
                failures.append("controlled source extension correspondence ledger hash mismatch")
            consumer_path = root / extension["stage5d_consumer_path"]
            if not consumer_path.is_file() or sha256_file(consumer_path) != extension[
                "stage5d_consumer_sha256"
            ]:
                failures.append(
                    f"controlled source extension consumer hash mismatch for {extension['path']}"
                )

    stage5c_manifest_hash = sha256_file(root / STAGE5C_MANIFEST_REL)
    if stage5c_manifest_hash != EXPECTED_STAGE5C_CLOSURE["manifest_sha256"]:
        failures.append(
            f"Stage 5C closure manifest hash mismatch: actual={stage5c_manifest_hash}"
        )
    report_hash = sha256_file(root / "docs/stage-5/stage-5c-acceptance-api-freeze-report.md")
    if report_hash != EXPECTED_STAGE5C_CLOSURE["report_sha256"]:
        failures.append(f"Stage 5C closure report hash mismatch: actual={report_hash}")
    compatibility_checker_hash = sha256_file(root / STAGE5C_CHECKER_REL)
    if compatibility_checker_hash != EXPECTED_STAGE5C_COMPATIBILITY_CHECKER["sha256"]:
        failures.append(
            f"Stage 5C compatibility checker hash mismatch: actual={compatibility_checker_hash}"
        )
    historical_checker_path = root / STAGE5C_CLOSURE_CHECKER_REL
    if not historical_checker_path.is_file():
        failures.append("historical Stage 5C closure checker artifact missing")
    else:
        historical_checker_hash = sha256_file(historical_checker_path)
        if historical_checker_hash != EXPECTED_HISTORICAL_STAGE5C_CHECKER["sha256"]:
            failures.append(
                f"historical Stage 5C closure checker hash mismatch: actual={historical_checker_hash}"
            )

    validate_stage5c_public_shape(root, manifest, failures)

    approved = manifest.get("approved_bridge_files", {})
    if set(approved) != set(APPROVED_BRIDGE_FILES):
        failures.append(
            f"approved bridge file set mismatch: actual={sorted(approved)} "
            f"expected={sorted(APPROVED_BRIDGE_FILES)}"
        )
    stage5c_manifest = json.loads((root / STAGE5C_MANIFEST_REL).read_text())
    closure_hashes = stage5c_manifest.get("source_hashes", {})
    for rel, regions in APPROVED_BRIDGE_FILES.items():
        path = root / rel
        record = approved.get(rel, {})
        if not path.is_file():
            failures.append(f"approved bridge file missing: {rel}")
            continue
        current_hash = sha256_file(path)
        if record.get("current_sha256") != current_hash:
            failures.append(f"{rel}: current hash mismatch actual={current_hash}")
        if record.get("closure_sha256") != closure_hashes.get(rel):
            failures.append(f"{rel}: closure hash reference mismatch")
        stripped, marker_failures = strip_additive_regions(path, regions)
        failures.extend(marker_failures)
        stripped_hash = sha256_bytes(stripped)
        if record.get("stripped_without_additive_regions_sha256") != stripped_hash:
            failures.append(f"{rel}: stripped hash mismatch actual={stripped_hash}")
        if stripped_hash != closure_hashes.get(rel):
            extension = approved_private_layout_extensions.get(rel)
            if (
                extension is None
                or extension.get("stripped_without_additive_regions_sha256") != stripped_hash
                or extension.get("public_api_unchanged") is not True
                or extension.get("reason_id")
                != "stage5d-b2b-a-persisted-load-provenance-v1"
            ):
                failures.append(f"{rel}: frozen region does not match Stage 5C closure source")

    validate_no_legacy_identifiers_in_additive_regions(root, APPROVED_BRIDGE_FILES, failures)
    validate_stage5d_bridge_call_sites(root, failures)
    validate_stage5d_b2bd1_runtime_restored_semantic_guards(root, failures)
    validate_stage5d_final_r3_riskgate_recovery_r1(root, failures)

    stage5d_record = manifest.get("stage5d_persistence_file", {})
    stage5d_path = root / STAGE5D_REL
    if not stage5d_path.is_file():
        failures.append("stage5d_persistence.rs missing")
    else:
        stage5d_hash = sha256_file(stage5d_path)
        if stage5d_record.get("path") != str(STAGE5D_REL):
            failures.append("Stage 5D persistence file path mismatch")
        if stage5d_record.get("current_sha256") != stage5d_hash:
            failures.append(f"stage5d_persistence.rs hash mismatch actual={stage5d_hash}")
        stage5d_source = stage5d_path.read_text()
        for pattern in FORBIDDEN_STAGE5D_PUBLIC_PATTERNS:
            for match in pattern.finditer(stage5d_source):
                failures.append(f"forbidden Stage 5D public surface: {match.group(0)}")
        public_symbols = parse_stage5d_public_symbols(stage5d_source)
        if public_symbols != EXPECTED_STAGE5D_PUBLIC_SYMBOLS:
            failures.append(
                f"Stage5d public symbol mismatch actual={public_symbols} "
                f"expected={EXPECTED_STAGE5D_PUBLIC_SYMBOLS}"
            )
        reexports = parse_stage5d_reexports((root / LIB_REL).read_text())
        if reexports != public_symbols:
            failures.append(f"Stage5d re-export mismatch actual={reexports} expected={public_symbols}")
        surface = parse_stage5d_surface(stage5d_source, (root / LIB_REL).read_text())
        if surface != manifest.get("stage5d_public_api"):
            failures.append("Stage5d public API surface mismatch")

    validate_legacy_restore_call_sites(root, failures)
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=str(DEFAULT_ROOT), help="workspace root")
    parser.add_argument("--manifest", default=None, help="manifest path")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    manifest_path = Path(args.manifest).resolve() if args.manifest else root / MANIFEST_REL
    failures = validate(root, manifest_path)
    if failures:
        for failure in failures:
            print(f"stage5d-additive-freeze-check: {failure}", file=sys.stderr)
        return 1
    print("stage5d-additive-freeze-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
