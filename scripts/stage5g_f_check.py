#!/usr/bin/env python3
"""Stage 5G-f protective completion checker."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "63e7f220f108ec539b61e73147938d461969daa8"
ACCEPTED_EDC_R3 = "c38d2e44e083e39552ea716823e43ebae775b881"
BRANCH = "stage5g-lifecycle"
SOURCE = Path("crates/strategy-runtime-core/src/stage5g_protective_completion.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
CONTRACT = Path("docs/stage-5/stage5g-f-protective-completion-contract.json")
DESIGN = Path("docs/stage-5/stage5g-f-protective-completion-contract.md")
GATE = Path("scripts/stage5g_f_r1_gate.sh")
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

REQUIRED_TESTS = [
    "stage5g_f_gprt01_mr_long_target_filled_plus_flat_completes",
    "stage5g_f_gprt02_mr_short_target_filled_plus_flat_completes",
    "stage5g_f_gprt03_mr_long_stop_execution_plus_flat_completes",
    "stage5g_f_gprt04_mr_short_stop_execution_plus_flat_completes",
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
    "stage5g_f_sibling_cleanup_requires_exact_paper_lifecycle_attribution",
    "stage5g_f_gprt_witnesses_are_frozen_and_ordered",
    "stage5g_f_debug_release_parallel_evidence_is_deterministic_in_process",
]

REQUIRED_SOURCE_MARKERS = [
    "pub const STAGE5G_PROTECTIVE_COMPLETION_SCHEMA_VERSION: u16 = 1;",
    "pub enum Stage5gProtectiveScenarioId",
    "pub const ALL: [Stage5gProtectiveScenarioId; 8]",
    "pub enum Stage5gProtectiveLeg",
    "pub enum Stage5gProtectiveDisposition",
    "pub enum Stage5gProtectiveBlockReason",
    "pub struct Stage5gProtectiveCompletionAuthority",
    "pub fn prepare_stage5g_protective_completion(",
    "restart: crate::Stage5gCleanRestartedCapability",
    ".into_stage5g_protective_completion_authority_input()",
    "pub(crate) fn admit_stage5g_protective_completion_authority(",
    "pub fn apply_stage5g_protective_completion(",
    "enum Stage5gProtectiveReplayClassification",
    "Stage5gProtectiveReplayClassification::ExactReplay",
    "Stage5gProtectiveReplayClassification::FingerprintConflict",
    "fn classify_replay(",
    "fn apply_canonical_stage5g_protective_completion_callbacks(",
    "crate::BrokerNeutralHybridStrategy::on_broker_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_stop_order",
    "crate::BrokerNeutralHybridStrategy::on_broker_position",
    "callback_count += 1;",
    "generated_cleanup_intents += intents.len();",
    "stage5g_protective_completion_callback_context",
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
    "HybridRuntimeOrderRole::Cancel",
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

FORBIDDEN_PRODUCTION_MARKERS = [
    "reqwest",
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "finam",
    "dispatch_order",
    "redis::",
    "xread",
    "xgroup",
    "runtime_live",
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
    return source.split("#[cfg(test)]", 1)[0]


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
    require(contract["negative_floor"]["current_stage5g_f_minimum"] >= 140,
            "negative floor drift")
    require(contract["frozen_stage5f_source_semantics"]["bar_ohlc_completion_authority"] is False,
            "bar OHLC authority opened")
    require(all(value is False for value in contract["closed_surfaces"].values()),
            "closed surface opened")

    for scenario in EXPECTED_SCENARIOS:
        require(source.count(scenario) >= 1, f"missing scenario string {scenario}")
    variants = rust_enum_variants(source, "Stage5gProtectiveScenarioId")
    require(len(variants) == 8, "Stage5gProtectiveScenarioId variant count drift")
    require("pub const ALL: [Stage5gProtectiveScenarioId; 8] = [" in source,
            "GPRT ALL inventory drift")


def check(root: Path, check_git: bool) -> None:
    if check_git:
        parent = subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=root, text=True).strip()
        branch = subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip()
        require(parent == BASE, "HEAD is not one direct successor to 63e7f22")
        require(branch == BRANCH, "wrong branch")

    source = read(root, SOURCE)
    prod = production_source(source)
    lib = read(root, LIB)
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

    for marker in REQUIRED_SOURCE_MARKERS:
        require(marker in source, f"missing source marker: {marker}")
    for marker in FORBIDDEN_PRODUCTION_MARKERS:
        require(marker not in prod, f"forbidden production marker: {marker}")
    for marker in FORBIDDEN_R1_PRODUCTION_MARKERS:
        require(marker not in prod, f"forbidden R1 production marker: {marker}")
    require("Stage 5F F12–F15 remain no-bar-exit" in design,
            "design lost F12-F15 no-bar-exit statement")
    require("Only after independent Stage 5G-f acceptance may Stage 5G-g begin" in design,
            "design lost Stage 5G-g closure")

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
    require("63e7f220f108ec539b61e73147938d461969daa8" in gate,
            "gate missing detached submitted 63e7f22 verification")
    require("stage5g-f-r1-gate: PASS" in gate, "gate PASS marker drift")
    require(">= 140" in negative or "140" in negative, "negative harness lost floor")
    require("EXPECTED = sorted([" in preseal, "preseal expected-path allowlist missing")
    require('["bash", "scripts/stage5g_f_r1_gate.sh"]' in handoff,
            "handoff builder does not run Stage 5G-f R1 gate")

    print("stage5g-f-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
