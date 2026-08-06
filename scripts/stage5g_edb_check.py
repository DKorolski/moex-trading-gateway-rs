#!/usr/bin/env python3
"""Current-head contract and closed-surface checker for Stage 5G-e-d-b R1."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


BASE_REF = "8a02f2a6b6e27587539d1e4e4717301bf010e6a1"
ACCEPTED_R6_REF = "4ece2c7c83ca5575dbca306b5fa29a48dae2bd47"
CONTRACT = Path("docs/stage-5/stage5g-e-d-b-reducer-contract.json")
MAIN_CONTRACT = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json")
DESIGN = Path("docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md")
REDUCER_DOC = Path("docs/stage-5/stage5g-e-d-b-reducer-contract.md")
STATUS = Path("docs/current-status.md")
ONBOARDING = Path("docs/reviewer-onboarding-and-roadmap.md")
PARENT_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs")
REDUCER_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs")
CLEAN_RESTART_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs")
ORDER_POSITION_SOURCE = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")

SCENARIOS = [
    "GRST01_RESTART_BEFORE_ACK",
    "GRST02_RESTART_AFTER_ACK_BEFORE_ORDER",
    "GRST03_RESTART_WITH_WORKING_ORDER",
    "GRST04_RESTART_AFTER_PARTIAL_FILL",
    "GRST05_RESTART_FILLED_BEFORE_POSITION",
    "GRST06_RESTART_AFTER_TERMINAL_POSITION_APPLIED",
    "GRST07_RESTART_AT_TIMER_CHECKPOINT",
    "GRST08_RESTART_WITH_GENERATED_INTENT_ESCROW",
    "GRST09_EXACT_REPLAY_IS_IDEMPOTENT",
    "GRST10_CONFLICTING_REPLAY_BLOCKS",
    "GRST11_FRESH_BROKER_TRUTH_OVERRIDES_STALE_HINT",
    "GRST12_MISSING_OR_AMBIGUOUS_TRUTH_REQUIRES_RECONCILIATION",
]
DISPOSITIONS = [
    "ExactReplay", "ContinueFromCommittedCheckpoint", "ApplyOwnedCandidate",
    "AwaitFreshBrokerTruth", "ReconciliationRequired", "ManualInterventionRequired",
    "TerminalInconsistency",
]
REASONS = [
    "ExactPackageReplay", "ExactHistoricalReplay", "FreshWorkingOrderMatched",
    "FreshTerminalOrderMatched", "PartialFillPositionConverged",
    "TerminalPositionAlreadyApplied", "TimerCheckpointExact",
    "GeneratedIntentEscrowRetained", "OrdersTruthIncomplete", "TradesTruthIncomplete",
    "PositionsTruthIncomplete", "AuthoritativeOrderMissing",
    "ClientOrderIdentityConflict", "BrokerOrderIdentityConflict", "TradeIdentityConflict",
    "PositionQuantityMismatch", "PositionDirectionMismatch", "UnexpectedTargetPosition",
    "ReplayFingerprintConflict", "OperationalIdentityConflict",
    "UnsupportedLifecycleCombination", "TerminalContradiction",
]
CROSS_BINDINGS = [
    "broker_id", "account_id", "strategy_definition_id", "strategy_instance_id",
    "deployment_id", "deployment_generation", "gateway_instance_id",
    "config_fingerprint_sha256", "instrument_map_fingerprint_sha256",
    "market_data_generation", "command_consumer_generation", "target_instrument_exact",
    "reconstructed_runtime_state_fingerprint_sha256", "restart_replay_commitment_sha256",
    "request_client_broker_identity",
]
CLOSED_SURFACES = {
    "strategy_callback": False,
    "runtime_mutation": False,
    "stage5d_persistence_mutation": False,
    "candidate_application": False,
    "redis": False,
    "finam": False,
    "http_post_delete": False,
    "broker_dispatch": False,
    "runtime_live": False,
    "real_orders": False,
    "stage5g_e_d_c": False,
    "stage5g_f": False,
    "stage6": False,
}
EXPECTED_DELTA = [
    ("M", "crates/strategy-runtime-core/src/stage5g_clean_restart.rs"),
    ("M", "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs"),
    ("M", "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs"),
    ("M", "crates/strategy-runtime-core/src/stage5g_order_position.rs"),
    ("M", "docs/current-status.md"),
    ("M", "docs/reviewer-onboarding-and-roadmap.md"),
    ("M", "docs/stage-5/stage5g-e-d-b-reducer-contract.json"),
    ("M", "docs/stage-5/stage5g-e-d-b-reducer-contract.md"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md"),
    ("M", "scripts/make_stage5g_ed_handoff_archive.py"),
    ("M", "scripts/stage5g_edb_check.py"),
    ("M", "scripts/stage5g_edb_gate.sh"),
    ("M", "scripts/stage5g_edb_negative_harness.py"),
    ("M", "scripts/stage5g_edb_preseal_check.py"),
]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage5g-edb-check: FAIL: {message}")


def load_json(root: Path, relative: Path) -> dict:
    try:
        value = json.loads((root / relative).read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage5g-edb-check: FAIL: cannot read {relative}: {error}") from error
    require(isinstance(value, dict), f"{relative} must contain an object")
    return value


def enum_variants(source: str, name: str) -> list[str]:
    match = re.search(rf"enum\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.S)
    require(match is not None, f"enum {name} missing")
    return re.findall(r"(?m)^\s{4}([A-Z][A-Za-z0-9_]*)\s*,\s*$", match.group(1))


def strip_comments(source: str) -> str:
    source = re.sub(r"/\*.*?\*/", "", source, flags=re.S)
    return re.sub(r"//[^\n]*", "", source)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact_git_delta(root: Path) -> list[tuple[str, str]]:
    output = subprocess.check_output(
        ["git", "diff", "--name-status", f"{BASE_REF}..HEAD"], cwd=root, text=True
    ).strip()
    rows: list[tuple[str, str]] = []
    for line in output.splitlines():
        fields = line.split("\t")
        require(len(fields) == 2 and fields[0] in {"A", "M"}, f"bad delta row: {line}")
        rows.append((fields[0], fields[1]))
    return rows


def check(root: Path, check_git: bool) -> None:
    for relative in [
        CONTRACT, MAIN_CONTRACT, DESIGN, REDUCER_DOC, STATUS, ONBOARDING,
        PARENT_SOURCE, REDUCER_SOURCE, CLEAN_RESTART_SOURCE, ORDER_POSITION_SOURCE,
        Path("scripts/make_stage5g_ed_handoff_archive.py"),
        Path("scripts/stage5g_edb_gate.sh"), Path("scripts/stage5g_edb_negative_harness.py"),
        Path("scripts/stage5g_edb_preseal_check.py"),
    ]:
        require((root / relative).is_file() and not (root / relative).is_symlink(),
                f"required regular file missing: {relative}")

    if check_git:
        require(subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=root, text=True).strip() == BASE_REF,
                "HEAD must be one direct successor to accepted R6")
        require(exact_git_delta(root) == EXPECTED_DELTA, "exact e-d-b changed-path allowlist drifted")

    contract = load_json(root, CONTRACT)
    require(contract.get("stage") == "5G-e-d-b-r1", "contract stage drifted")
    require(contract.get("accepted_base") == BASE_REF, "accepted base drifted")
    require(contract.get("owning_entry_point") == "reduce_stage5g_fresh_broker_truth",
            "owning reducer entry drifted")
    require(contract.get("scenario_ids") == SCENARIOS, "scenario order/content drifted")
    require(contract.get("dispositions") == DISPOSITIONS, "dispositions drifted")
    require(contract.get("reasons") == REASONS, "reason taxonomy drifted")
    require(contract.get("cross_bindings") == CROSS_BINDINGS, "cross-bindings drifted")
    require(contract.get("closed_surfaces") == CLOSED_SURFACES, "closed surfaces drifted")
    determinism = contract.get("determinism")
    require(isinstance(determinism, dict) and determinism == {
        "frozen_order_count": 12,
        "sequential": True,
        "canonical_row_order": True,
        "parallel_no_shared_state": True,
        "owning_input_evidence_byte_identical": True,
        "multi_trade_row_order_canonical": True,
        "wall_clock_reads": False,
        "exact_replay_semantic_noop": True,
    }, "determinism contract drifted")

    main = load_json(root, MAIN_CONTRACT)
    require(main.get("stage") == "5G-e-d-b-r1", "main contract stage drifted")
    require(main.get("accepted_stage5g_e_d_a_r6_commit") == ACCEPTED_R6_REF,
            "main contract R6 binding drifted")
    require(main.get("implemented_restart_case_ids") == SCENARIOS,
            "implemented GRST list drifted")
    require(main.get("next_slice") == "5G-e-d-c_after_independent_e_d_b_acceptance",
            "next slice drifted")
    require(main.get("closed_surfaces", {}).get("reconciliation_reducer") is True,
            "main contract does not open only reducer")
    for key, expected in CLOSED_SURFACES.items():
        if key in main.get("closed_surfaces", {}):
            require(main["closed_surfaces"][key] is expected, f"main closed surface drifted: {key}")

    reducer = (root / REDUCER_SOURCE).read_text()
    parent = (root / PARENT_SOURCE).read_text()
    clean = (root / CLEAN_RESTART_SOURCE).read_text()
    order_position = (root / ORDER_POSITION_SOURCE).read_text()
    require(parent.count("mod reducer;") == 1, "child reducer registration drifted")
    require(reducer.count("pub(crate) fn reduce_stage5g_fresh_broker_truth(") == 1,
            "there must be exactly one owning reducer entry")
    require(enum_variants(reducer, "Stage5gFreshTruthReductionReason") == REASONS,
            "Rust reason taxonomy drifted")
    require("_restart: Stage5gCleanRestartedCapability" in reducer
            and "_truth: Stage5gRestartBoundFreshBrokerTruthPackage" in reducer,
            "linear input ownership retention drifted")
    require("candidate: Option<Stage5gOwnedReconciliationCandidate>" in reducer,
            "opaque candidate ownership drifted")
    require("fn candidate_is_self_consistent(" in reducer,
            "candidate self-consistency validation missing")
    require("fn cross_binding_matches(" in reducer, "cross-binding function missing")
    for marker in [
        "Stage5gRestartBoundFreshBrokerTruthPackage",
        "stage5g_operational_binding_commitment(",
        "stage5g_restart_replay_commitment(",
        "restart.account_id == package.operational_identity.account_id",
        "restart.strategy_id == package.operational_identity.strategy_definition_id.as_str()",
        "restart.config_fingerprint_sha256",
        "restart.instrument_id == package.operational_identity.target_instrument",
        "ReplayFingerprintConflict",
        "stage5g_exact_trade_order_linkage(order, trade)",
        "stage5g_expected_post_position_qty(slot.pre_position_qty, order)",
        "stage5g_intent_position_is_compatible(",
        "OrderStatus::New | OrderStatus::Working",
        "OrderStatus::PartiallyFilled",
        "OrderStatus::Filled",
        "OrderStatus::Rejected",
        "OrderStatus::Canceled | OrderStatus::Expired",
        "positions_complete",
        "generated_intent_escrow_fingerprint_sha256",
    ]:
        require(marker in reducer, f"reducer invariant anchor missing: {marker}")
    for marker, minimum in [
        ("stage5g_operational_binding_commitment(", 2),
        ("OrderStatus::New | OrderStatus::Working => {", 2),
        ("OrderStatus::PartiallyFilled => {", 2),
        ("OrderStatus::Filled => {", 2),
        ("OrderStatus::Rejected => {", 2),
        ("OrderStatus::Canceled | OrderStatus::Expired => {", 2),
        ("if !truth.positions_complete {", 6),
    ]:
        require(reducer.count(marker) >= minimum,
                f"reducer repeated safety anchor weakened: {marker}")
    for marker in [
        "pub(crate) struct Stage5gFreshTruthReplayAuthorityV1",
        "pub(crate) struct Stage5gRestartBoundFreshBrokerTruthPackage",
        "pub(crate) fn bind_stage5g_fresh_truth_to_clean_restart(",
        "restart_replay_authority_matches(",
    ]:
        require(marker in parent, f"restart-owned authority anchor missing: {marker}")
    for field in CROSS_BINDINGS[:11]:
        require(field in parent, f"full operational identity field missing: {field}")
    require("pub(crate) struct Stage5gFreshTruthRestartProjection" in clean,
            "narrow restart projection missing")
    require("pub(crate) struct Stage5gFreshTruthRestartSlotProjection" in order_position,
            "narrow slot projection missing")
    for marker in [
        "intent_class: Stage5gRestartIntentClass",
        "pre_position_qty: Decimal",
        "target_qty: Option<Decimal>",
        "pub(crate) fn stage5g_exact_trade_order_linkage(",
        "pre_position_qty + signed_fill",
    ]:
        require(marker in order_position, f"canonical slot/linkage anchor missing: {marker}")
    require(order_position.count("pre_position_qty + signed_fill") >= 2,
            "source-relative position formula coverage weakened")

    for type_name in ["Stage5gFreshTruthReduction", "Stage5gOwnedReconciliationCandidate"]:
        prefix = reducer.split(f"struct {type_name}", 1)[0][-240:]
        require(not re.search(r"derive\([^)]*(Clone|Copy|Serialize|Deserialize|Default)", prefix),
                f"linear authority type gained forbidden derive: {type_name}")

    production = strip_comments(reducer.split("#[cfg(test)]", 1)[0]).lower()
    for forbidden in [
        "reqwest", "redis::", "finam", "tokio", "utc::now", "systemtime",
        ".post(", ".delete(", "stage5d_export", "stage5c_", "dispatch_order",
        "strategy::on", "intent_sink",
    ]:
        require(forbidden not in production, f"closed production surface opened: {forbidden}")

    tests = reducer.split("#[cfg(test)]", 1)[1]
    for index in range(1, 13):
        require(tests.count(f"fn stage5g_edb_grst{index:02d}()") == 1,
                f"GRST{index:02d} positive test missing/duplicated")
    for witness in [
        "stage5g_edb_matrix_executes_frozen_ids_once_in_order",
        "stage5g_edb_sequential_and_row_order_are_deterministic",
        "stage5g_edb_exact_replay_is_semantic_noop",
        "stage5g_edb_parallel_execution_has_no_shared_mutable_state",
        "stage5g_edb_r1_all_operational_identity_fields_are_commitment_bound",
        "stage5g_edb_r1_restart_replay_commitment_and_conflict_are_enforced",
        "stage5g_edb_r1_exact_trade_linkage_rejects_secondary_id_conflicts",
        "stage5g_edb_r1_source_relative_entry_exit_and_terminal_matrix",
        "stage5g_edb_r1_owning_timer_ready_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_awaiting_runs_export_decode_restore_validate_bind_reduce",
        "stage5g_edb_r1_owning_status_paths_cover_working_filled_terminal_and_missing",
        "stage5g_edb_r1_owning_generated_intent_escrow_is_retained",
        "stage5g_edb_r1_owning_exact_current_and_historical_replay_are_noops",
        "stage5g_edb_r1_owning_row_order_and_parallel_evidence_are_deterministic",
    ]:
        require(tests.count(f"fn {witness}()") == 1, f"focused witness drifted: {witness}")

    for relative in [DESIGN, REDUCER_DOC, STATUS, ONBOARDING]:
        text = (root / relative).read_text()
        require("8a02f2a" in text, f"rejected e-d-b base reference missing: {relative}")
        require("4ece2c7" in text, f"accepted R6 reference missing: {relative}")
        require("Stage 5G-e-d-c" in text or "e-d-c" in text,
                f"next closed e-d-c boundary missing: {relative}")
    reducer_doc = (root / REDUCER_DOC).read_text()
    for marker in ["classification", "opaque", "callback", "Redis", "FINAM", "runtime-live"]:
        require(marker in reducer_doc, f"reducer documentation marker missing: {marker}")

    source_hashes = contract.get("source_sha256")
    require(isinstance(source_hashes, dict), "source SHA map missing")
    expected_hash_paths = [
        str(REDUCER_SOURCE), str(PARENT_SOURCE), str(CLEAN_RESTART_SOURCE),
        str(ORDER_POSITION_SOURCE),
    ]
    require(list(source_hashes) == expected_hash_paths, "source SHA path/order drifted")
    for relative, expected in source_hashes.items():
        require(sha256(root / relative) == expected, f"source SHA drifted: {relative}")

    print("stage5g-edb-check: PASS")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    check(args.root.resolve(), not args.skip_git)


if __name__ == "__main__":
    main()
