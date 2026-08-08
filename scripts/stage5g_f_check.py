#!/usr/bin/env python3
"""Stage 5G-f protective completion checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "430bae6cd02f67844623f9d1b2112b1faedcc40a"
SUBMITTED_R4 = "430bae6cd02f67844623f9d1b2112b1faedcc40a"
SUBMITTED_R3 = "7dde2ac181c7a5d3a3312bfb463e384281062a8a"
SUBMITTED_R2 = "34ecc9595bdb83639415ddde1b3975b88ac2faa4"
ACCEPTED_R1 = "a28cedd984d41bd2db4aeb7fd8c125c62ded4b28"
ACCEPTED_EDC_R3 = "c38d2e44e083e39552ea716823e43ebae775b881"
BRANCH = "stage5g-lifecycle"
SOURCE = Path("crates/strategy-runtime-core/src/stage5g_protective_completion.rs")
RESTART = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
GPRT_BIN = Path("crates/strategy-runtime-core/src/bin/stage5g_f_gprt_artifact.rs")
CONTRACT = Path("docs/stage-5/stage5g-f-protective-completion-contract.json")
DESIGN = Path("docs/stage-5/stage5g-f-protective-completion-contract.md")
GATE = Path("scripts/stage5g_f_r5_gate.sh")
NEGATIVE = Path("scripts/stage5g_f_negative_harness.py")
PRESEAL = Path("scripts/stage5g_f_preseal_check.py")
HANDOFF = Path("scripts/make_stage5g_f_handoff_archive.py")

EXPECTED_SCENARIOS = [
    "GPRT01_F12_MR_LONG_TARGET_COMPLETES_FLAT",
    "GPRT02_F13_MR_SHORT_TARGET_COMPLETES_FLAT",
    "GPRT03_F14_MR_LONG_STOP_COMPLETES_FLAT",
    "GPRT04_F15_MR_SHORT_STOP_COMPLETES_FLAT",
    "GPRT05_WRONG_OWNER_OR_CYCLE_BLOCKS",
    "GPRT06_WRONG_INSTRUMENT_OR_ORDER_ID_BLOCKS",
    "GPRT07_TRIGGER_WITHOUT_FLAT_POSITION_BLOCKS",
    "GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT",
]

EXPECTED_SCENARIO_VARIANTS = [
    "Gprt01F12MrLongTargetCompletesFlat",
    "Gprt02F13MrShortTargetCompletesFlat",
    "Gprt03F14MrLongStopCompletesFlat",
    "Gprt04F15MrShortStopCompletesFlat",
    "Gprt05WrongOwnerOrCycleBlocks",
    "Gprt06WrongInstrumentOrOrderIdBlocks",
    "Gprt07TriggerWithoutFlatPositionBlocks",
    "Gprt08NonExecutionTerminalCannotInventExit",
]

REQUIRED_TESTS = [
    "stage5g_f_gprt01_mr_long_target_filled_plus_flat_cleanup_pending",
    "stage5g_f_gprt02_mr_short_target_filled_plus_flat_cleanup_pending",
    "stage5g_f_gprt03_mr_long_stop_execution_plus_flat_cleanup_pending",
    "stage5g_f_gprt04_mr_short_stop_execution_plus_flat_cleanup_pending",
    "stage5g_f_gprt05_wrong_owner_or_cycle_blocks",
    "stage5g_f_gprt06_wrong_instrument_or_ids_block",
    "stage5g_f_gprt07_trigger_without_flat_awaits_position_truth",
    "stage5g_f_gprt08_non_execution_terminal_cannot_invent_exit",
    "stage5g_f_f12_to_f15_bar_extremes_remain_no_bar_exit_authority",
    "stage5g_f_owner_role_instrument_side_qty_and_chronology_are_exact",
    "stage5g_f_complete_absent_target_position_is_flat_but_incomplete_absent_is_not",
    "stage5g_f_position_truth_duplicate_and_contradictory_rows_do_not_sum_flat",
    "stage5g_f_fractional_quantity_is_rejected_by_integral_lot_authority",
    "stage5g_f_duplicate_exact_is_idempotent_and_conflicting_duplicate_blocks",
    "stage5g_f_standalone_json_restart_codec_is_not_available",
    "stage5g_f_callback_generated_cleanup_is_retained_and_raw_cleanup_is_blocked",
    "stage5g_f_gprt_witnesses_are_frozen_and_ordered",
    "stage5g_f_debug_release_parallel_evidence_is_deterministic_in_process",
    "stage5g_f_r3_authenticated_restart_prepares_protective_authority_and_canonical_issuer",
    "stage5g_f_r3_awaiting_position_truth_survives_authenticated_restart",
    "stage5g_f_r3_flat_cleanup_pending_survives_authenticated_restart",
    "stage5g_f_r3_completed_is_not_immediate_when_sibling_cleanup_is_pending",
    "stage5g_f_r5_multi_request_cleanup_settles_only_after_all_requests",
    "stage5g_f_r5_cleanup_token_is_bound_to_exact_pending_authority",
    "stage5g_f_r5_cleanup_execution_race_requires_position_truth",
    "stage5g_f_r4_non_terminal_cleanup_truth_keeps_flat_cleanup_pending",
]

REQUIRED_SOURCE_MARKERS = [
    "pub const STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION: u16 = 1;",
    "pub const STAGE5G_PROTECTIVE_RESTART_PROJECTION_SCHEMA_VERSION: u16 = 1;",
    "pub const STAGE5G_PROTECTIVE_CANONICAL_EVIDENCE_SCHEMA_VERSION: u16 = 1;",
    "pub enum Stage5gProtectiveScenarioId",
    "pub const ALL: [Stage5gProtectiveScenarioId; 8]",
    "pub enum Stage5gProtectiveLeg",
    "pub enum Stage5gProtectiveDisposition",
    "pub enum Stage5gProtectiveBlockReason",
    "Stage5gProtectiveBlockReason::UnsupportedCleanRestartLifecycleKind",
    "Stage5gProtectiveBlockReason::CanonicalBrokerTruthMismatch",
    "Stage5gProtectiveBlockReason::ProtectiveRestartProjectionMismatch",
    "pub enum Stage5gProtectiveRestartProjectionKind",
    "pub struct Stage5gProtectiveRestartProjectionV1",
    "pub struct Stage5gProtectiveReceiptLedgerProjection",
    "pub struct Stage5gAcceptedProtectiveBrokerTruth",
    "pub(crate) fn accept_stage5g_canonical_protective_broker_truth(",
    "pub struct Stage5gProtectiveRestartSource",
    "pub enum Stage5gProtectiveRestoredContinuation",
    "pub struct Stage5gProtectiveCompletionAuthority",
    "pub fn prepare_stage5g_protective_completion(",
    "restart: crate::Stage5gCleanRestartedCapability",
    ".into_stage5g_protective_completion_authority_parts()",
    "pub fn issue_stage5g_canonical_protective_evidence(",
    "pub fn stage5g_protective_restart_source_from_transition(",
    "pub fn restore_stage5g_protective_completion_continuation(",
    "pub(crate) fn admit_stage5g_protective_completion_authority(",
    "pub struct Stage5gValidatedProtectiveEvidence",
    "pub fn apply_stage5g_protective_completion(",
    "validated: Stage5gValidatedProtectiveEvidence",
    "let evidence = validated.evidence;",
    "enum Stage5gProtectiveReplayClassification",
    "Stage5gProtectiveReplayClassification::ExactReplay",
    "Stage5gProtectiveReplayClassification::FingerprintConflict",
    "fn classify_replay(",
    "pub struct Stage5gProtectiveCommittedState",
    "pub struct Stage5gProtectivePostStateSummary",
    "pub struct Stage5gProtectiveFlatCleanupPending",
    "Stage5gProtectiveDisposition::FlatCleanupPending",
    "post_state: Stage5gProtectiveCommittedState",
    "generated_cleanup_batch: crate::Stage5cPaperIntentBatch",
    "generated_cleanup_batch_summary: crate::Stage5cPaperIntentBatchSummary",
    "settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>",
    "fn apply_stage5c_owned_protective_lifecycle_bridge(",
    "resolve_stage5g_protective_broker_lifecycle_bridge(",
    "Stage5gProtectiveBrokerLifecycleExecution::Order",
    "Stage5gProtectiveBrokerLifecycleExecution::StopOrder",
    "bridge_post_state_fingerprint_sha256",
    "stage5g_protective_completion_post_callback_summary",
    "post_callback_state_fingerprint_sha256",
    "Stage5gProtectiveCleanupEscrowProof",
    "paper_lifecycle_escrow",
    "Stage5gProtectiveSiblingTerminalEvidence",
    "sibling_terminal",
    "fn sibling_order_id_matches(",
    "fn terminal_status_is_safe(",
    "fn scenario_for_reason(",
    "Stage5gProtectiveBlockReason::MissingSiblingCleanupProof",
    "Stage5gProtectiveBlockReason::SiblingCleanupOrderIdMismatch",
    "Stage5gProtectiveBlockReason::CanonicalCallbackFailed",
    "Stage5gProtectiveScenarioId::Gprt05WrongOwnerOrCycleBlocks",
    "Stage5gProtectiveScenarioId::Gprt06WrongInstrumentOrOrderIdBlocks",
    "Stage5gProtectiveScenarioId::Gprt07TriggerWithoutFlatPositionBlocks",
    "Stage5gProtectiveScenarioId::Gprt08NonExecutionTerminalCannotInventExit",
    "Stage5gProtectiveRestartProjectionKind::PreExecutionReady",
    "Stage5gProtectiveRestartProjectionKind::AwaitingPositionTruth",
    "Stage5gProtectiveRestartProjectionKind::FlatCleanupPending",
    "Stage5gProtectiveRestartProjectionKind::Completed",
    "protective_projection_fingerprint",
    "validate_stage5g_protective_restart_projection",
    "cleanup_batch_restart_projection",
    "Stage5gProtectiveCleanupSettlementEvidence",
    "Stage5gAcceptedProtectiveCleanupTruth",
    "Stage5gProtectiveCleanupTransition",
    "pub fn accept_stage5g_protective_cleanup_truth(",
    "pub fn apply_stage5g_protective_cleanup_completion(",
    "stage5g_protective_cleanup_batch_restart_projection(",
    "restore_stage5g_protective_cleanup_batch_from_projection(",
    "stage5g_protective_cleanup_batch_projection_fingerprint(",
    "receipt_ledger_projection",
    "runtime_stage5c_state_fingerprint",
    "let Some(projection) = parts.protective_projection.clone() else {",
    "Stage5gProtectiveRestartProjectionKind::FlatCleanupPending => {\n            let post_state = Stage5gProtectiveCommittedState::new(parts.runtime);",
    "let cleanup_projection = projection\n                .cleanup_batch_restart_projection",
    "generated_cleanup_batch,\n                    generated_cleanup_batch_summary,\n                    settled_batch_history,\n                    cleanup_settlement_ledger,\n                    restart_seed: Some(parts.restart_seed),",
    "Stage5gProtectiveRestartProjectionKind::Completed => {\n            let post_state = Stage5gProtectiveCommittedState::new(parts.runtime);",
    "return Err(Stage5gProtectiveBlockReason::UnsupportedCleanRestartLifecycleKind);",
    "validate_evidence(authority, &accepted.evidence)?;",
    "validate_preexisting_sibling_terminal(authority, &accepted.evidence)?;",
    "accepted.canonical_authority_fingerprint_sha256\n        != authority.summary().authority_fingerprint_sha256",
    "projection.post_runtime_stage5c_state_fingerprint_sha256\n            != runtime_stage5c_state_fingerprint_sha256",
    "projection.replay_protection_fingerprint_sha256\n            != protective_projection_fingerprint(projection)",
    "if replay == Stage5gProtectiveReplayClassification::FingerprintConflict {",
    "let replay_should_append = matches!(replay, Stage5gProtectiveReplayClassification::New);",
    "authority.accepted_receipts.push(execution_receipt.clone());",
    "generated_cleanup_batch: bridge.generated_intent_batch,",
    "settled_batch_history: bridge.settled_batch_history,",
    "post_state: callback.post_state,",
    "cleanup_settlement_fingerprint_sha256",
    ".protected_position_qty.to_f64()",
    "Stage5gProtectiveBlockReason::PostCallbackPositionNotIntegral",
    "input.current_owner != HybridRuntimeOwner::MeanReversion",
    "input\n        .active_cycle_id\n        .as_deref()\n        .unwrap_or_default()\n        .is_empty()",
    "input.tp_order_id.is_none() || input.sl_stop_order_id.is_none()",
    "evidence.observed_account_id != authority.input.account_id",
    "event_ts < authority.input.protective_created_ts_utc",
    "event_ts < authority.input.last_lifecycle_checkpoint_ts_utc",
    "order.instrument != authority.input.instrument",
    "Some(&order.order_id) != authority.input.tp_order_id.as_ref()",
    "Some(&order.stop_order_id) != authority.input.sl_stop_order_id.as_ref()",
    "order.exchange_order_id.as_ref() != Some(expected_exchange_order_id)",
    "HybridRuntimeOrderRole::TakeProfit",
    "HybridRuntimeOrderRole::StopLoss",
    "attribution.owner() != Some(HybridRuntimeOwner::MeanReversion)",
    "attribution.role() != Some(expected_role)",
    "attribution.cycle_id() != authority.active_cycle_id()",
    "normalize_side(side) != expected_exit_side(authority.input.protected_position_side)",
    "qty != authority.input.protected_position_qty",
    "filled_qty != authority.input.protected_position_qty",
    "if !truth.positions_complete",
    "truth.received_ts_utc < event_ts_utc",
    "position.account_id != authority.input.account_id",
    "source_ts.timestamp() < event_ts_utc",
    "stage5g_integral_lot_decimal(qty)",
    "stage5g_integral_lot_decimal(filled_qty)",
    "let mut target_position: Option<&BrokerPositionSnapshot> = None;",
    "if target_position.is_some()",
    "if position.qty != Decimal::ZERO",
    "normalize_status(&order.status) == \"filled\"",
    "\"filled\" | \"executed\" | \"triggered\" | \"done\" | \"completed\"",
    "\"canceled\" | \"cancelled\" | \"expired\" | \"rejected\"",
    "Stage5gProtectiveBlockReason::ConflictingDuplicateEvidence",
]

REQUIRED_RESTART_MARKERS = [
    "ProtectiveLifecycle(crate::stage5g_protective_completion::Stage5gProtectiveRestartSource)",
    "ProtectiveLifecycleCommitted",
    "pub(crate) protective_lifecycle_projection:\n        Option<crate::stage5g_protective_completion::Stage5gProtectiveRestartProjectionV1>",
    "protective_lifecycle_projection:",
    "Option<crate::stage5g_protective_completion::Stage5gProtectiveRestartProjectionV1>",
    "Stage5gValidatedReconciliationAuthority::ProtectiveLifecycleCommitted",
    "into_stage5g_protective_completion_authority_parts",
    "Stage5gProtectiveCleanRestartParts",
    "Stage5gProtectiveRestartProjectionKind::FlatCleanupPending",
    "Stage5gProtectiveRestartProjectionKind::Completed",
]

REQUIRED_STAGE5C_BRIDGE_MARKERS = [
    "pub(crate) enum Stage5gProtectiveBrokerLifecycleExecution",
    "pub(crate) struct Stage5gProtectiveBrokerLifecycleBridgeInput",
    "pub(crate) struct Stage5gProtectiveBrokerLifecycleBridgeOutput",
    "pub(crate) fn resolve_stage5g_protective_broker_lifecycle_bridge(",
    "crate::BrokerNeutralHybridStrategy::on_broker_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_stop_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_position",
    "stage5g_protective_merge_generated_intents(",
    "stage5g_protective_completion_callback_context",
    "stage5cj_verify_generated_batch_final_pending_consistency(",
    "stage5ch_batch_summary(generated_batch)",
    "post_state_fingerprint_sha256",
]

FORBIDDEN_R1_PRODUCTION_MARKERS = [
    "pub struct Stage5gProtectiveCompletionAuthorityInput",
    "pub fn admit_stage5g_protective_completion_authority(",
    "pub fn export_stage5g_protective_completion_for_restart(",
    "pub fn restore_stage5g_protective_completion_from_restart(",
    "serde_json::from_slice(bytes)",
    "serde_json::to_vec(transition)",
    "Decimal::from_f64(",
    "accepted_by_paper_lifecycle",
]

FORBIDDEN_R2_STAGE5G_PRODUCTION_MARKERS = [
    "crate::BrokerNeutralHybridStrategy::on_broker_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_stop_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_position",
    "generated_cleanup_intents += intents.len();",
    "pub struct Stage5gProtectiveCompleted {\n    pub cleanup_pending: bool,",
    "pub fn apply_stage5g_protective_completion(\n    authority: Stage5gProtectiveCompletionAuthority,\n    evidence: Stage5gProtectiveCompletionEvidence,",
    "pub fn validate_stage5g_protective_completion_evidence(",
    "pub struct Stage5gProtectiveCompletionEvidence",
    "pub struct Stage5gProtectiveExecutionEvidence",
    "pub struct Stage5gProtectivePositionTruth",
    "pub struct Stage5gProtectiveSiblingCleanupEvidence",
    "pub struct Stage5gProtectiveSiblingTerminalEvidence",
]

FORBIDDEN_PRODUCTION_MARKERS = [
    "reqwest",
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "finam::",
    "FinamRestClient",
    "FinamTransport",
    "dispatch_order",
    "redis::",
    "xread",
    "xgroup",
        "runtime_live_enabled: true",
        "runtime_live_attached: true",
        "redis_command_stream_attached: true",
        "finam_transport_attached: true",
        "Stage 6",
    "Stage6",
    "BarEvent",
    ".high",
    ".low",
    "Utc::now",
    "thread::sleep",
]


def require(ok: bool, message: str) -> None:
    if not ok:
        raise SystemExit(f"stage5g-f-check: FAIL: {message}")


def read(root: Path, path: Path) -> str:
    target = root / path
    require(target.is_file() and not target.is_symlink(), f"missing {path}")
    return target.read_text()


def production_source(source: str) -> str:
    return source.split("\n#[cfg(test)]\nmod tests", 1)[0]


def stage5c_bridge_source(source: str) -> str:
    begin = "// STAGE5G-F-R2-BEGIN: protective-lifecycle-stage5c-bridge"
    end = "// STAGE5G-F-R2-END: protective-lifecycle-stage5c-bridge"
    require(begin in source and end in source, "Stage 5C protective bridge block markers missing")
    return source.split(begin, 1)[1].split(end, 1)[0]


def bounded_section(source: str, start_marker: str, end_marker: str) -> str:
    start = source.find(start_marker)
    require(start != -1, f"section start missing: {start_marker}")
    end = source.find(end_marker, start + len(start_marker))
    require(end != -1, f"section end missing: {end_marker}")
    return source[start:end]


def rust_enum_variants(source: str, enum_name: str) -> list[str]:
    match = re.search(rf"enum\s+{re.escape(enum_name)}\s*\{{(?P<body>.*?)\n\}}", source, re.S)
    require(match is not None, f"missing enum {enum_name}")
    body = re.sub(r"//.*", "", match.group("body"))
    variants = []
    for raw in body.split(","):
        item = raw.strip()
        if not item:
            continue
        variants.append(item.split("(", 1)[0].split("{", 1)[0].strip())
    return variants


def check_contract(root: Path, source: str) -> None:
    contract = json.loads(read(root, CONTRACT))
    require(contract["schema_version"] == 1, "contract schema drift")
    require(contract["stage"] == "5G-f", "contract stage drift")
    require(contract["base_ref"] == BASE, "contract base drift")
    require(contract["branch"] == BRANCH, "contract branch drift")
    require(contract["entry_function"] == "apply_stage5g_protective_completion",
            "contract entry function drift")
    require(contract["validated_evidence_type"] == "Stage5gValidatedProtectiveEvidence",
            "validated evidence type drift")
    require(contract["production_apply_accepts_raw_evidence"] is False,
            "production apply raw evidence reopened")
    require(contract["authority_issuer"] == "prepare_stage5g_protective_completion",
            "authority issuer drift")
    require(contract["authority_source"] == "Stage5gCleanRestartedCapability",
            "authority source drift")
    require(contract["production_public_raw_authority_input"] is False,
            "public raw authority input reopened")
    require(contract["production_standalone_json_restart_codec"] is False,
            "standalone JSON restart codec reopened")
    require(contract["canonical_callback_bridge"] is True,
            "canonical callback bridge not declared")
    require(contract["canonical_callback_bridge_file"] == str(STAGE5C),
            "canonical callback bridge file drift")
    require(contract["stage5g_direct_raw_broker_callback_boundary"] is False,
            "Stage 5G-f direct raw callback boundary reopened")
    require(contract["successful_completion_owns_post_runtime"] is True,
            "successful completion no longer owns post runtime")
    require(contract["flat_cleanup_pending_owns_generated_batch"] is True,
            "FlatCleanupPending no longer owns generated batch")
    require(contract["generated_cleanup_intents_retained"] is True,
            "generated cleanup intent retention disabled")
    require(contract["restart_extension_status"] == "protective_restart_cleanup_completion_r5",
            "restart extension status must be R5 cleanup ledger closure")
    require(contract["authenticated_protective_restart"] is True,
            "authenticated protective restart not declared")
    require(contract["protective_projection_in_clean_restart_package"] is True,
            "protective projection not bound to clean restart")
    require(contract["production_canonical_broker_truth_issuer"] is True,
            "production canonical broker truth issuer missing")
    require(contract["cleanup_batch_restart_projection"] is True,
            "cleanup batch restart projection missing")
    require(contract["cleanup_batch_projection_is_reconstructable"] is True,
            "cleanup batch projection must be reconstructable")
    require(contract["cleanup_settlement_boundary"] == "apply_stage5g_protective_cleanup_completion",
            "cleanup settlement boundary drift")
    require(contract["deterministic_artifact_scenarios"] == 8,
            "all-eight deterministic artifact coverage drift")
    require(contract["canonical_protective_evidence_issuer"] == "issue_stage5g_canonical_protective_evidence",
            "canonical evidence issuer drift")
    require(contract["canonical_broker_truth_acceptor_scope"] == "crate_private_production_issuer",
            "canonical broker truth acceptor scope drift")
    require(contract["production_raw_evidence_validator_exported"] is False,
            "production raw evidence validator exported")
    require(contract["completed_policy"] == "not_immediate_when_sibling_cleanup_pending",
            "Completed policy drift")
    require(contract["callback_bridge_transport_attached"] is False,
            "callback bridge attached transport")
    require(contract["cleanup_caller_boolean_proof"] is False,
            "cleanup caller boolean proof reopened")
    require(contract["cleanup_requires_escrow_or_terminal_proof"] is True,
            "cleanup proof requirement drift")
    require(contract["exact_replay_appends_receipt"] is False,
            "exact replay may append receipt")
    require(contract["position_rows_are_summed_to_flat"] is False,
            "position rows can be summed to flat")
    predecessor = contract["predecessor_verification"]
    require(predecessor["mode"] == "bounded_detached_stage5g_edc_r3",
            "predecessor verification mode drift")
    require(predecessor["commit"] == ACCEPTED_EDC_R3, "predecessor verification commit drift")
    require(predecessor["runs_recursive_historical_lineage"] is False,
            "predecessor verification recursion reopened")
    required_commands = predecessor["required_commands"]
    for command in [
        "python3 scripts/stage5g_edc_r3_check.py",
        "python3 scripts/stage5g_edc_r3_negative_harness.py",
        "python3 scripts/stage5g_edc_r3_preseal_check.py",
        "cargo test -p strategy-runtime-core --lib stage5g_edc_r3_",
        "cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_",
    ]:
        require(command in required_commands, f"predecessor command missing: {command}")
    require(contract["scenario_order"] == EXPECTED_SCENARIOS, "GPRT scenario order drift")
    require(contract["negative_floor"]["current_stage5g_f_minimum"] >= 390,
            "negative floor drift")
    require(contract["cleanup_settlement_ledger"] == "per_request_terminal_nonexecution_required",
            "cleanup settlement ledger contract drift")
    require(contract["cleanup_truth_pending_authority_binding"] is True,
            "cleanup pending authority binding disabled")
    require(contract["cleanup_execution_race_policy"] == "position_truth_required",
            "cleanup execution race policy drift")
    require(contract["debug_release_gprt_artifact"] == "stage5g-f-gprt-artifact.json",
            "debug/release GPRT artifact contract drift")
    require(contract["frozen_stage5f_source_semantics"]["bar_ohlc_completion_authority"] is False,
            "bar OHLC authority opened")
    require(all(value is False for value in contract["closed_surfaces"].values()),
            "closed surface opened")

    for scenario in EXPECTED_SCENARIOS:
        require(source.count(scenario) >= 1, f"missing scenario string {scenario}")
    variants = rust_enum_variants(source, "Stage5gProtectiveScenarioId")
    require(variants == EXPECTED_SCENARIO_VARIANTS,
            "Stage5gProtectiveScenarioId variant inventory/order drift")
    require("pub const ALL: [Stage5gProtectiveScenarioId; 8] = [" in source,
            "GPRT ALL inventory drift")
    require(source.count("TP_OTHER") >= 2, "GPRT06 wrong-order-id witness drift")
    require('"Triggered",\n                    nonflat_position_truth()' in source,
            "GPRT07 non-flat trigger witness drift")
    require('"Canceled",\n                    flat_position_truth()' in source,
            "GPRT08 non-execution terminal witness drift")


def check(root: Path, check_git: bool) -> None:
    if check_git:
        parent = subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=root, text=True).strip()
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip()
        require(parent == BASE, "HEAD is not one direct successor to 7dde2ac")
        require(branch == BRANCH, "wrong branch")

    source = read(root, SOURCE)
    prod = production_source(source)
    restart = read(root, RESTART)
    stage5c = read(root, STAGE5C)
    stage5c_bridge = stage5c_bridge_source(stage5c)
    lib = read(root, LIB)
    gprt_bin = read(root, GPRT_BIN)
    design = read(root, DESIGN)
    gate = read(root, GATE)
    negative = read(root, NEGATIVE)
    preseal = read(root, PRESEAL)
    handoff = read(root, HANDOFF)

    check_contract(root, source)

    require("mod stage5g_protective_completion;" in lib, "module not linked")
    require("pub use stage5g_protective_completion::" in lib, "public Stage 5G-f facade missing")
    require("Stage5gProtectiveCompletionAuthority" in lib, "authority export missing")
    require("Stage5gProtectiveCompletionTransition" in lib, "transition export missing")
    require("issue_stage5g_canonical_protective_evidence" in lib,
            "canonical evidence issuer export missing")
    require("restore_stage5g_protective_completion_continuation" in lib,
            "protective continuation restore export missing")
    require("apply_stage5g_protective_cleanup_completion" in lib,
            "cleanup completion export missing")
    for raw_export in [
        "Stage5gProtectiveCompletionEvidence",
        "Stage5gProtectiveExecutionEvidence",
        "Stage5gProtectivePositionTruth",
        "Stage5gProtectiveSiblingCleanupEvidence",
        "Stage5gProtectiveSiblingTerminalEvidence",
        "validate_stage5g_protective_completion_evidence",
    ]:
        require(raw_export not in lib, f"raw protective evidence export reopened: {raw_export}")

    for marker in REQUIRED_SOURCE_MARKERS:
        require(marker in source, f"missing source marker: {marker}")
    require("pub(crate) fn accept_stage5g_canonical_protective_broker_truth(" in prod,
            "canonical broker truth issuer must be production-reachable inside crate")
    require("#[cfg(test)]\npub(crate) fn accept_stage5g_canonical_protective_broker_truth(" not in source,
            "canonical broker truth issuer must not be test-only")
    for marker in [
        "Stage5gProtectiveCleanupSettlementLedgerV1",
        "Stage5gProtectiveCleanupRequestSettlementV1",
        "Stage5gProtectiveCleanupOutcome",
        "stage5g_pending_cleanup_authority_sha256",
        "cleanup_ledger_fingerprint_before_sha256",
        "pending_cleanup_authority_sha256",
        "Stage5gProtectiveBlockReason::CleanupAuthorityMismatch",
        "Stage5gProtectiveCleanupTransition::CleanupPositionTruthRequired",
        "stage5g_cleanup_ledger_all_terminal_non_execution",
        "stage5g_f_gprt_artifact_json_pretty",
    ]:
        require(marker in source or marker in lib, f"missing R5 cleanup marker: {marker}")
    for marker in [
        "pub struct Stage5gProtectiveCleanupSettlementLedgerV1",
        "pub struct Stage5gProtectiveCleanupRequestSettlementV1",
        "pub enum Stage5gProtectiveCleanupOutcome",
        "pub enum Stage5gProtectiveCleanupSettlementState",
        "pub struct Stage5gProtectiveGprtArtifactRow",
    ]:
        require(marker in source, f"missing R5 source type marker: {marker}")
    require("pending_authority != accepted.evidence.pending_cleanup_authority_sha256" in source,
            "cleanup apply must bind accepted token to consumed pending authority")
    require("fn stage5g_pending_cleanup_authority_sha256(" in source,
            "pending cleanup authority helper missing")
    require("fn stage5g_cleanup_ledger_fingerprint(" in source,
            "cleanup ledger fingerprint helper missing")
    require(source.count("stage5g_cleanup_ledger_fingerprint") >= 4,
            "cleanup ledger fingerprint call coverage drift")
    require("pub fn stage5g_f_gprt_artifact_json_pretty(" in source,
            "GPRT artifact source API missing")
    require("strategy_runtime_core::stage5g_f_gprt_artifact_json_pretty()" in gprt_bin,
            "GPRT artifact bin emitter no longer calls artifact API")
    require(source.count('Some("completed")') >= 1,
            "GPRT artifact positive rows must include completed phase-b coverage")
    require(source.count("if !stage5g_cleanup_ledger_is_valid(") >= 5,
            "cleanup ledger validity guard coverage drift")
    require("pub cleanup_ledger_fingerprint_before_sha256: String" in source,
            "cleanup ledger-before evidence field missing")
    require("pub pending_cleanup_authority_sha256: String" in source,
            "pending cleanup authority evidence field missing")
    require("accepted.evidence.cleanup_ledger_fingerprint_before_sha256" in source,
            "cleanup apply must consume ledger-before evidence field")
    require('crate::stage5c_paper_host::Stage5gSourceBaseAction::DeleteStopLimit => "delete_stop_limit"' in source,
            "DeleteStopLimit cleanup action name must remain exact")
    for marker in [
        "Stage5gProtectiveBlockReason::CleanupAuthorityMismatch",
        "Stage5gProtectiveBlockReason::CleanupConflict",
    ]:
        require(marker in source, f"missing R5 block reason use: {marker}")
    require("    CleanupPositionTruthRequired," in source,
            "cleanup position-truth-required block reason variant missing")
    require("    CleanupPositionTruthRequired(Box<Stage5gProtectiveRestoredFlatCleanupPending>)," in source,
            "cleanup position-truth-required transition variant missing")
    require("if !stage5g_cleanup_ledger_all_terminal_non_execution(&pending.cleanup_settlement_ledger)" in source,
            "Completed must require all cleanup requests terminal-nonexecution")
    require("Stage5gProtectiveCleanupOutcome::Pending => {\n            Stage5gProtectiveCleanupSettlementState::Pending" in source,
            "cleanup Pending outcome must remain Pending settlement state")
    require("Stage5gProtectiveCleanupOutcome::ExecutionObserved => {\n            Stage5gProtectiveCleanupSettlementState::ExecutionObserved" in source,
            "cleanup execution-observed outcome must require execution-observed settlement state")
    require("cleanup_settlement_fingerprint_sha256: Some(\n            pending.cleanup_settlement_ledger.ledger_fingerprint_sha256,\n        )" in source,
            "completed cleanup must preserve cleanup settlement fingerprint")
    require("cleanup_settlement_fingerprint_sha256: completed.cleanup_settlement_fingerprint_sha256" in source,
            "completed restart projection must preserve cleanup settlement fingerprint")
    require('"filled" | "executed" | "triggered" | "completed-as-execution"' in source,
            "cleanup execution race taxonomy missing")
    require("Stage5gProtectiveCleanupOutcome::ExecutionObserved" in source,
            "execution-observed cleanup outcome missing")
    require("stage5g_f_r5_cleanup_token_is_bound_to_exact_pending_authority" in source,
            "cross-pending cleanup authority test missing")
    require("stage5g_f_r5_cleanup_execution_race_requires_position_truth" in source,
            "execution-race cleanup test missing")
    require("crates/strategy-runtime-core/src/bin/stage5g_f_gprt_artifact.rs" in preseal,
            "GPRT artifact emitter not sealed by scripts")
    require("stage5g-f-gprt-artifact.debug.json" in gate,
            "gate missing debug GPRT artifact emission")
    require("stage5g-f-gprt-artifact.release.json" in gate,
            "gate missing release GPRT artifact emission")
    require('cmp "$artifact_dir/stage5g-f-gprt-artifact.debug.json" "$artifact_dir/stage5g-f-gprt-artifact.release.json"' in gate,
            "gate missing debug/release GPRT artifact cmp")
    require('shasum -a 256 "$artifact_dir/stage5g-f-gprt-artifact.debug.json"' in gate,
            "gate missing GPRT artifact sha256 emission")
    require("removed-stage5g-f-gprt-artifact" not in gate,
            "removed GPRT artifact filename survived in gate")
    for forbidden_drift_marker in [
        "cleanup_batch_restart_projection_removed",
        "cleanup_settlement_fingerprint_removed",
        "generated_cleanup_batch_removed",
        "generated_cleanup_batch_summary_removed",
        "settled_batch_history_removed",
        "if false && !stage5g_cleanup_ledger_all_terminal_non_execution",
        "if false && !stage5g_cleanup_ledger_is_valid",
        "removed_stage5g_cleanup_ledger_fingerprint",
        "false && pending_authority != accepted.evidence.pending_cleanup_authority_sha256",
        "== accepted.evidence.cleanup_ledger_fingerprint_before_sha256",
        "== accepted.evidence.batch_fingerprint_sha256",
        "false && entry.target_protective_id != accepted.evidence.target_protective_id",
        "false && entry.base_action != accepted.evidence.base_action",
        "false && entry.expected_attribution != accepted.evidence.expected_attribution",
        "Stage5gProtectiveCleanupOutcome::Canceled => {\n            Stage5gProtectiveCleanupSettlementState::Pending",
        "Stage5gProtectiveCleanupOutcome::Canceled => {\n            Stage5gProtectiveCleanupSettlementState::ExecutionObserved",
        "cleanup_settlement_fingerprint_sha256: None,\n            post_state",
        "removed_accept_stage5g_protective_cleanup_truth",
        "removed_apply_stage5g_protective_cleanup_completion",
        ".find(|_record| true)",
        "if false && target_protective_id != record.target_protective_id",
        "|| false && received_ts_utc < record.source_event_ts",
        "cleanup_projection.batch_fingerprint\n                    == crate::stage5c_paper_host::stage5g_protective_cleanup_batch_projection_fingerprint",
        "projection.batch_fingerprint\n            == stage5g_protective_cleanup_batch_projection_fingerprint(projection)",
    ]:
        require(forbidden_drift_marker not in source, f"removed marker survived: {forbidden_drift_marker}")
    for marker in REQUIRED_RESTART_MARKERS:
        require(marker in restart, f"missing clean-restart marker: {marker}")
    require(source.count("post_state: Stage5gProtectiveCommittedState") >= 5,
            "owned post-state coverage drift")
    require(source.count("post_state: callback.post_state,") >= 2,
            "post-callback runtime ownership drift")
    require(source.count("settled_batch_history: Vec<crate::Stage5cPaperIntentBatchSummary>") >= 2,
            "settled batch history ownership drift")
    for marker in REQUIRED_STAGE5C_BRIDGE_MARKERS:
        require(marker in stage5c_bridge, f"missing Stage 5C bridge marker: {marker}")
    for marker in [
        "pub struct Stage5gProtectiveCleanupBatchRestartProjectionV1",
        "pub struct Stage5gProtectiveCleanupBatchRestartRecordV1",
        "pub(crate) fn stage5g_protective_cleanup_batch_restart_projection(",
        "pub(crate) fn restore_stage5g_protective_cleanup_batch_from_projection(",
        "pub(crate) fn stage5g_protective_cleanup_batch_projection_fingerprint(",
    ]:
        require(marker in stage5c, f"missing Stage 5C cleanup projection marker: {marker}")
    for forbidden_stage5c_marker in [
        "pub(crate) fn removed_stage5g_protective_cleanup_batch_restart_projection(",
        "pub(crate) fn removed_restore_stage5g_protective_cleanup_batch_from_projection(",
        "pub(crate) fn removed_stage5g_protective_cleanup_batch_projection_fingerprint(",
    ]:
        require(forbidden_stage5c_marker not in stage5c,
                f"removed Stage 5C cleanup projection marker survived: {forbidden_stage5c_marker}")
    stage5c_cleanup_section = bounded_section(
        stage5c,
        "pub(crate) fn stage5g_protective_cleanup_batch_restart_projection(",
        "pub(crate) fn stage5g_protective_cleanup_batch_projection_fingerprint(",
    )
    require("if intent_class != crate::BrokerNeutralHybridIntentClass::CancelCleanup" in stage5c,
            "Stage 5C cleanup projection must require CancelCleanup intent class")
    require("_ => return Err(Stage5cIntentSettlementError::UnsupportedIntentAction)," in stage5c,
            "Stage 5C cleanup projection must reject non-cleanup actions")
    require("projection.request_ids.get(index) != Some(&record.request_id)" in stage5c,
            "Stage 5C cleanup reconstructor must preserve request_id order")
    require("projection.batch_fingerprint\n            != stage5g_protective_cleanup_batch_projection_fingerprint(projection)" in stage5c,
            "Stage 5C cleanup reconstructor must verify projection fingerprint")
    for stage5c_field_binding in [
        "target_protective_id,",
        "source_event_ts: record.source_event_ts",
        "expected_attribution: record.expected_attribution.clone()",
    ]:
        require(stage5c_cleanup_section.count(stage5c_field_binding) >= 2,
                f"Stage 5C cleanup projection/restoration field binding missing: {stage5c_field_binding}")
    for forbidden_stage5c_bypass in [
        "if false && intent_class != crate::BrokerNeutralHybridIntentClass::CancelCleanup",
        "_ => crate::BrokerNeutralHybridIntent::Cancel { order_id: BrokerOrderId::new(record.target_protective_id.clone()) },",
        "false && projection.request_ids.get(index) != Some(&record.request_id)",
        "projection.batch_fingerprint\n            == stage5g_protective_cleanup_batch_projection_fingerprint(projection)",
        "target_protective_id: String::new(),",
        "expected_attribution: None",
        "source_event_ts: 0",
    ]:
        require(forbidden_stage5c_bypass not in stage5c_cleanup_section,
                f"Stage 5C cleanup-only bypass survived: {forbidden_stage5c_bypass}")
    require(stage5c_bridge.count("stage5g_protective_merge_generated_intents(") >= 3,
            "Stage 5C bridge generated-intent merge call coverage drift")
    for marker in FORBIDDEN_PRODUCTION_MARKERS:
        require(marker not in prod, f"forbidden production marker: {marker}")
    for marker in FORBIDDEN_R1_PRODUCTION_MARKERS:
        require(marker not in prod, f"forbidden R1 production marker: {marker}")
    for marker in FORBIDDEN_R2_STAGE5G_PRODUCTION_MARKERS:
        require(marker not in prod, f"forbidden Stage 5G-f boundary marker: {marker}")
    require("Stage 5F F12–F15 remain no-bar-exit" in design,
            "design lost F12-F15 no-bar-exit statement")
    require("Only after independent Stage 5G-f acceptance may Stage 5G-g begin" in design,
            "design lost Stage 5G-g closure")
    require("R5 cleanup ledger closure" in design,
            "design lost R5 cleanup ledger closure statement")
    require("Completed requires every generated cleanup request to reach terminal non-execution" in design,
            "design lost R5 Completed policy")

    for test in REQUIRED_TESTS:
        require(f"fn {test}(" in source, f"missing focused test {test}")

    require("python3 scripts/stage5g_f_check.py" in gate, "gate missing checker")
    require("python3 scripts/stage5g_f_negative_harness.py" in gate, "gate missing negative")
    require("python3 scripts/stage5g_f_preseal_check.py" in gate, "gate missing preseal")
    require("cargo test -p strategy-runtime-core --lib stage5g_f_" in gate,
            "gate missing focused debug")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_f_" in gate,
            "gate missing focused release")
    require("python3 scripts/stage5g_edc_r3_check.py" in gate,
            "gate missing detached e-d-c R3 checker")
    require("python3 scripts/stage5g_edc_r3_negative_harness.py" in gate,
            "gate missing detached e-d-c R3 negative")
    require("python3 scripts/stage5g_edc_r3_preseal_check.py" in gate,
            "gate missing detached e-d-c R3 preseal")
    require("cargo test -p strategy-runtime-core --lib stage5g_edc_r3_" in gate,
            "gate missing detached e-d-c R3 debug tests")
    require("cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_" in gate,
            "gate missing detached e-d-c R3 release tests")
    require(ACCEPTED_R1 in gate, "gate missing accepted a28cedd R1 lineage verification")
    require(SUBMITTED_R4 in gate, "gate missing detached submitted 430bae6 R4 verification")
    require(SUBMITTED_R3 in gate, "gate missing detached submitted 7dde2ac R3 verification")
    require(SUBMITTED_R2 in gate, "gate missing detached submitted 34ecc95 R2 verification")
    require("stage5g-f-r5-gate: PASS" in gate, "gate PASS marker drift")
    require(">= 390" in negative or "390" in negative, "negative harness lost floor")
    require("EXPECTED = sorted([" in preseal, "preseal expected-path allowlist missing")
    require('["bash", "scripts/stage5g_f_r5_gate.sh"]' in handoff,
            "handoff builder does not run Stage 5G-f R5 gate")
    require("stage5g-f-gprt-artifact.sha256" in handoff,
            "handoff builder must include GPRT artifact SHA256 sidecar")
    require("removed-stage5g-f-gprt-artifact.sha256" not in handoff,
            "removed GPRT artifact SHA256 sidecar survived in handoff")

    print("stage5g-f-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
