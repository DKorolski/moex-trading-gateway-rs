#!/usr/bin/env python3
"""Fail-closed checker for the design-only Stage 5G-a entry package."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
STAGE = "5G-a-lifecycle-entry"
ACCEPTED_STAGE5F = "fb8245e2f91cfc1678548a1228e8558d9adc2181"
CLOSURE_COMMIT = "cac83da38725aeadd6d029a3078157c2ab7fa004"
PLAN = "docs/stage-5/5g-lifecycle-design-and-implementation-plan.md"
ADR = "docs/adr/adr-stage5g-paper-mock-development-governance.md"
INVENTORY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"
CLOSURE = "docs/stage-5/stage5f-closure-descriptor.json"
STATUS = "docs/current-status.md"
EXPECTED_PLAN_SHA256 = "96df2bb97182d68bcea0432549453ee6e7415a18bfa053e802166113d06549a2"
EXPECTED_ADR_SHA256 = "0fb2e41ed76e5a9f6c9ecf99ccedf605e895576d3ac583c83cf0c90e04c18dc7"
EXPECTED_INVENTORY_SHA256 = "d9a2e3e9cb7a0aae5f26a68be784ef7b0f1ce7bfbc017eaee29b991d2ad91539"
EXPECTED_CLOSURE_SHA256 = "5b93866a92f6ed946bd31e3defe47b1f89c00697c41f60a3b8cdb2831a75a613"
EXPECTED_STATUS_SHA256 = "811ccbc76e4d0199ebd101d2b7b57ce264b596fbe8261b6cc6919a88375e76e1"

EXPECTED_CHANGED_PATHS = [
    "docs/adr/adr-stage5g-paper-mock-development-governance.md",
    "docs/current-status.md",
    "docs/stage-5/5g-lifecycle-design-and-implementation-plan.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "scripts/make_stage5g_entry_handoff_archive.py",
    "scripts/stage5g_entry_handoff_safety_check.py",
    "scripts/stage5g_entry_plan_check.py",
    "scripts/stage5g_entry_plan_negative_harness.py",
]

EXPECTED_TARGET = {
    "strategy_id": "hybrid_imoexf",
    "account_id": "ACC_TEST_0001",
    "symbol": "IMOEXF",
    "venue_symbol": "IMOEXF@RTSX",
    "timeframe_sec": 600,
    "trade_mode": "paper",
    "mock_feedback_only": True,
}

EXPECTED_GOVERNANCE = {
    "branch": "stage5g-lifecycle",
    "main_untouched": True,
    "direct_branch_push_is_release_authority": False,
    "implementation_requires_stage5g_a_acceptance": True,
    "main_merge_requires_separate_governance_decision": True,
    "deployment_authorized": False,
}

EXPECTED_AUTHORITIES = [
    (
        "STAGE5F_CLOSURE",
        CLOSURE,
        EXPECTED_CLOSURE_SHA256,
        "accepted_semantic_predecessor",
        "frozen",
    ),
    (
        "STAGE5F_FINAL_INVENTORY",
        "docs/stage-5/stage5f-final-scenario-inventory.json",
        "92330d1b54ff8a88ae437f6c43d894c35a0ea58f93195ded4edb63e2f5723136",
        "accepted_semantic_matrix_contract",
        "frozen",
    ),
    (
        "STAGE5F_GOLDEN",
        "docs/stage-5/stage5f-d-golden-results.json",
        "aed7e21d7a524fd3dfd2bc6c2b128b379ff812b91556f0021fc70ba3cbf33a3d",
        "accepted_semantic_golden",
        "frozen",
    ),
    (
        "BROKER_COMMAND_ACK",
        "crates/broker-core/src/command.rs",
        "a1d39d7585bda16df4c2e22486874d7658f7bd2a0aaf8cbb7aa909c0fbeb4e6b",
        "broker_neutral_ack_identity_contract",
        "frozen_for_5g_entry",
    ),
    (
        "RUNTIME_ACK_POLICY",
        "crates/broker-core/src/runtime_state.rs",
        "09bb0f6fd343e28a1032dcc9ffcba81731bf31e34d753ae914633ffd5451141a",
        "pending_and_ack_policy",
        "frozen_for_5g_entry",
    ),
    (
        "BROKER_OPERATIONAL_TRUTH",
        "crates/broker-core/src/operational_snapshot.rs",
        "53e78a922b1c1a7948485f3016acdbcd64c3766618274a3b039233fc67d541ca",
        "order_trade_position_truth",
        "frozen_for_5g_entry",
    ),
    (
        "STAGE5C_PAPER_HOST",
        "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "93c0b48e1b564ef1763354579885bea3cd5b448133afccbc611584184bb13f2d",
        "sole_feedback_callback_and_timer_type_state",
        "frozen",
    ),
    (
        "STAGE5D_PERSISTENCE",
        "crates/strategy-runtime-core/src/stage5d_persistence.rs",
        "f790a907d6730e26e731a78ef89c58f993b39acde6ce934602e2fe603d90f083",
        "canonical_restart_authority",
        "frozen",
    ),
    (
        "STAGE5F_SEMANTIC_ROUTE",
        "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs",
        "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508",
        "accepted_ordered_semantic_intent_route",
        "frozen",
    ),
]

EXPECTED_OWNERSHIP_RULES = [
    "stage5f_owns_semantic_intent_generation",
    "stage5c_owns_feedback_callbacks_and_type_state",
    "stage5d_owns_canonical_restart_package",
    "broker_core_owns_ack_identity_and_broker_truth",
    "stage5g_owns_only_mock_event_admission_ordering_correlation_and_reconciliation",
    "broker_truth_wins_over_persisted_working_set_hints",
    "strategy_request_id_is_required_to_clear_pending",
    "broker_order_id_remains_exact_string",
    "pre_callback_validation_is_atomic",
    "post_callback_failure_is_terminal",
    "callback_generated_intents_reenter_same_lifecycle",
]

EXPECTED_SUB_STAGES = [
    ("5G-a", "entry_contract_and_authority_inventory", "design_review_candidate"),
    ("5G-b", "mock_ack_attachment", "blocked_pending_5g_a_acceptance"),
    ("5G-c", "order_trade_position_convergence", "blocked_pending_5g_b_acceptance"),
    ("5G-d", "timer_and_continuation_arbitration", "blocked_pending_5g_c_acceptance"),
    ("5G-e", "deterministic_restart_and_reconciliation", "blocked_pending_5g_d_acceptance"),
    ("5G-f", "paper_protective_completion", "blocked_pending_5g_e_acceptance"),
    ("5G-g", "lifecycle_matrix_and_fingerprint_freeze", "blocked_pending_5g_f_acceptance"),
    ("5G-h", "aggregate_acceptance_and_closure", "blocked_pending_5g_g_acceptance"),
]

EXPECTED_FAMILIES = {
    "ACK": (
        "5G-b",
        [
            "GACK01_PLACE_ACCEPTED_EXACT_IDS",
            "GACK02_SUBMITTED_MISSING_BROKER_ID_KEEPS_PENDING",
            "GACK03_RECOVERED_EXACT_BROKER_ID",
            "GACK04_REJECTED_EXACT_REQUEST_CLEARS_PENDING",
            "GACK05_TIMEOUT_KEEPS_PENDING",
            "GACK06_UNKNOWN_PENDING_KEEPS_PENDING",
            "GACK07_DUPLICATE_REQUIRES_PRIOR_OUTCOME",
            "GACK08_EXPIRED_REQUIRES_EXACT_NO_SEND_PROOF",
            "GACK09_REQUEST_OR_CLIENT_ID_MISMATCH_BLOCKS",
            "GACK10_BROKER_ORDER_ID_CONFLICT_BLOCKS",
        ],
    ),
    "ORDER_POSITION": (
        "5G-c",
        [
            "GOP01_WORKING_ORDER_REMAINS_ACTIVE",
            "GOP02_PARTIAL_FILL_ADVANCES_MONOTONICALLY",
            "GOP03_PARTIAL_FILL_REGRESSION_BLOCKS",
            "GOP04_FILLED_REQUIRES_TARGET_POSITION_CONFIRMATION",
            "GOP05_CANCELED_TERMINATES_WITHOUT_POSITION_CHANGE",
            "GOP06_REJECTED_TERMINATES_WITHOUT_POSITION_CHANGE",
            "GOP07_EXPIRED_TERMINATES_WITHOUT_POSITION_CHANGE",
            "GOP08_UNKNOWN_ORDER_STATUS_BLOCKS",
            "GOP09_IDENTICAL_EVENT_REPLAY_IS_IDEMPOTENT",
            "GOP10_CONFLICTING_DUPLICATE_EVENT_BLOCKS",
            "GOP11_NON_TARGET_EVENT_CANNOT_SETTLE_TARGET",
            "GOP12_ACCOUNT_WIDE_ACTIVE_ORDER_IS_SAFETY_GUARD",
            "GOP13_TARGET_POSITION_SIDE_MISMATCH_BLOCKS",
            "GOP14_TARGET_POSITION_OVERFILL_BLOCKS",
            "GOP15_CORRELATED_TRADE_SUPPORTS_FILL_TRUTH",
            "GOP16_TRADE_IDENTITY_OR_QUANTITY_MISMATCH_BLOCKS",
        ],
    ),
    "TIMER": (
        "5G-d",
        [
            "GTMR01_MONOTONIC_ZERO_INTENT_TIMER_CONTINUES",
            "GTMR02_EQUAL_OR_REVERSED_TIMER_BLOCKS",
            "GTMR03_TIMER_INTENT_REENTERS_ACK_LIFECYCLE",
            "GTMR04_TIMER_CLEANUP_PRESERVES_ATTRIBUTION",
            "GTMR05_CHECKPOINT_IS_SINGLE_CONSUME",
            "GTMR06_BAR_TIMER_RACE_HAS_ONE_DETERMINISTIC_WINNER",
            "GTMR07_GENERATED_BATCH_BLOCKS_UNRELATED_CONTINUATION",
            "GTMR08_NO_AUTONOMOUS_LOOP_OR_CLOCK_READ",
        ],
    ),
    "RESTART": (
        "5G-e",
        [
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
        ],
    ),
    "PROTECTIVE": (
        "5G-f",
        [
            "GPRT01_F12_MR_LONG_TARGET_COMPLETES_FLAT",
            "GPRT02_F13_MR_SHORT_TARGET_COMPLETES_FLAT",
            "GPRT03_F14_MR_LONG_STOP_COMPLETES_FLAT",
            "GPRT04_F15_MR_SHORT_STOP_COMPLETES_FLAT",
            "GPRT05_WRONG_OWNER_OR_CYCLE_BLOCKS",
            "GPRT06_WRONG_INSTRUMENT_OR_ORDER_ID_BLOCKS",
            "GPRT07_TRIGGER_WITHOUT_FLAT_POSITION_BLOCKS",
            "GPRT08_NON_EXECUTION_TERMINAL_CANNOT_INVENT_EXIT",
        ],
    ),
}

EXPECTED_ENTRY_GATES = [
    "stage5g_entry_plan_check",
    "stage5g_entry_plan_negative_harness",
    "cargo_fmt",
    "workspace_tests",
    "workspace_doctests",
    "workspace_clippy",
    "stage5f_forbidden_no_rg_snapshot_gate",
    "stage5g_entry_handoff_safety",
]

EXPECTED_REVIEW_CHECKPOINTS = [
    "5G-a-entry-design",
    "5G-c-ack-order-position-convergence",
    "5G-f-restart-protective-convergence",
    "5G-h-aggregate-closure",
]

EXPECTED_CLOSED_SURFACES = {
    "real_finam_post": False,
    "real_finam_delete": False,
    "finam_transport": False,
    "redis_live_consumer": False,
    "redis_consumer_groups": False,
    "broker_dispatch": False,
    "broker_execution": False,
    "runtime_live": False,
    "live_ready": False,
    "unattended_execution": False,
    "real_orders": False,
    "native_stop_sltp_bracket": False,
    "stage6_durable_command_chain": False,
    "stage7_runtime_command_consumer": False,
    "stage8_real_execution": False,
}


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(encoding="utf-8"),
            object_pairs_hook=strict_object,
        )
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def sha256(root: Path, relative: str) -> str:
    try:
        return hashlib.sha256((root / relative).read_bytes()).hexdigest()
    except OSError as exc:
        fail(f"cannot hash {relative}: {exc}")


def require(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", CLOSURE_COMMIT, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", CLOSURE_COMMIT, "--"],
                cwd=root,
                text=True,
            ).splitlines()
        )
        untracked = set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=root,
                text=True,
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cannot verify Stage 5G-a lineage: {exc}")
    require(sorted(changed | untracked), EXPECTED_CHANGED_PATHS, "5G-a changed paths")
    forbidden_suffixes = (".rs", ".toml", ".lock", ".yml", ".yaml")
    if any(path.endswith(forbidden_suffixes) for path in changed | untracked):
        fail("Stage 5G-a must not change Rust, Cargo or workflow files")


def validate_closure(root: Path) -> None:
    require(sha256(root, CLOSURE), EXPECTED_CLOSURE_SHA256, "Stage 5F closure SHA-256")
    closure = read_json(root, CLOSURE)
    require(closure.get("stage"), "5F", "closure stage")
    require(closure.get("status"), "accepted_and_closed", "closure status")
    accepted = exact_keys(
        closure.get("accepted_source"),
        {
            "branch",
            "source_ref",
            "source_commit_short",
            "archive_name",
            "archive_sha256",
            "archive_member_count",
            "archive_unique_member_count",
            "archive_duplicate_entries",
            "archive_path_traversal_entries",
            "archive_absolute_paths",
            "archive_symlinks",
            "archive_special_files",
            "handoff_safety_check",
            "preseal_exit_code",
        },
        "closure accepted source",
    )
    require(accepted["source_ref"], ACCEPTED_STAGE5F, "accepted Stage 5F source")
    require(accepted["archive_sha256"], "23b320a9ff829eebc5cb064138f5f848146229da75c87738c30c11f517d02ad7", "accepted archive SHA-256")
    require(accepted["handoff_safety_check"], "PASS", "accepted handoff safety")
    transition = closure.get("transition")
    if not isinstance(transition, dict):
        fail("closure transition missing")
    require(transition.get("stage5g_predecessor_ref"), ACCEPTED_STAGE5F, "5G predecessor")
    require(transition.get("stage5g_review_status"), "unlocked_for_paper_mock_design_and_implementation", "5G review status")
    require(transition.get("macro_stage_status"), "active", "macro Stage 5 status")


def validate_inventory(root: Path) -> None:
    require(sha256(root, STATUS), EXPECTED_STATUS_SHA256, "current status SHA-256")
    require(sha256(root, PLAN), EXPECTED_PLAN_SHA256, "Stage 5G plan SHA-256")
    require(sha256(root, ADR), EXPECTED_ADR_SHA256, "Stage 5G ADR SHA-256")
    require(sha256(root, INVENTORY), EXPECTED_INVENTORY_SHA256, "Stage 5G inventory SHA-256")
    inventory = read_json(root, INVENTORY)
    exact_keys(
        inventory,
        {
            "schema_version",
            "stage",
            "status",
            "predecessor",
            "target",
            "governance",
            "reuse_authorities",
            "ownership_rules",
            "sub_stages",
            "scenario_families",
            "scenario_case_count",
            "required_entry_gates",
            "review_checkpoints",
            "closed_surfaces",
            "next_transition",
        },
        "inventory",
    )
    if type(inventory["schema_version"]) is not int:
        fail("schema_version must be an exact JSON integer")
    require(inventory["schema_version"], 1, "schema version")
    require(inventory["stage"], STAGE, "stage")
    require(inventory["status"], "design_review_candidate", "status")
    predecessor = exact_keys(
        inventory["predecessor"],
        {
            "accepted_stage5f_source_ref",
            "stage5f_closure_commit",
            "stage5f_closure_descriptor",
            "stage5f_closure_descriptor_sha256",
            "accepted_handoff_archive",
            "accepted_handoff_sha256",
            "verdict",
        },
        "predecessor",
    )
    require(predecessor["accepted_stage5f_source_ref"], ACCEPTED_STAGE5F, "predecessor source")
    require(predecessor["stage5f_closure_commit"], CLOSURE_COMMIT, "closure commit")
    require(predecessor["stage5f_closure_descriptor_sha256"], EXPECTED_CLOSURE_SHA256, "closure binding")
    require(predecessor["verdict"], "ACCEPTED", "predecessor verdict")
    require(inventory["target"], EXPECTED_TARGET, "target")
    require(inventory["governance"], EXPECTED_GOVERNANCE, "governance")

    authorities = inventory["reuse_authorities"]
    if not isinstance(authorities, list):
        fail("reuse_authorities must be an array")
    actual_authorities = []
    for index, item in enumerate(authorities):
        authority = exact_keys(
            item,
            {"id", "path", "sha256", "role", "mutability"},
            f"authority[{index}]",
        )
        actual_authorities.append(
            (
                authority["id"],
                authority["path"],
                authority["sha256"],
                authority["role"],
                authority["mutability"],
            )
        )
        require(sha256(root, authority["path"]), authority["sha256"], f"authority file {authority['id']}")
    require(actual_authorities, EXPECTED_AUTHORITIES, "authority inventory")
    require(inventory["ownership_rules"], EXPECTED_OWNERSHIP_RULES, "ownership rules")

    actual_sub_stages = []
    for index, item in enumerate(inventory["sub_stages"]):
        sub_stage = exact_keys(
            item,
            {"id", "name", "status", "rust_changes_allowed_before_5g_a_acceptance"},
            f"sub_stage[{index}]",
        )
        if sub_stage["rust_changes_allowed_before_5g_a_acceptance"] is not False:
            fail("all later Stage 5G slices must remain blocked at entry")
        actual_sub_stages.append((sub_stage["id"], sub_stage["name"], sub_stage["status"]))
    require(actual_sub_stages, EXPECTED_SUB_STAGES, "sub-stage sequence")

    families = inventory["scenario_families"]
    if not isinstance(families, list):
        fail("scenario_families must be an array")
    actual_families: dict[str, tuple[str, list[str]]] = {}
    all_case_ids: list[str] = []
    for index, item in enumerate(families):
        family = exact_keys(item, {"id", "owner_stage", "case_ids"}, f"family[{index}]")
        if family["id"] in actual_families:
            fail(f"duplicate family id: {family['id']}")
        if not isinstance(family["case_ids"], list) or not all(isinstance(case, str) for case in family["case_ids"]):
            fail(f"invalid case ids for family {family['id']}")
        actual_families[family["id"]] = (family["owner_stage"], family["case_ids"])
        all_case_ids.extend(family["case_ids"])
    require(actual_families, EXPECTED_FAMILIES, "scenario families")
    if len(all_case_ids) != len(set(all_case_ids)):
        fail("scenario case ids must be globally unique")
    require(len(all_case_ids), 54, "scenario case count")
    if type(inventory["scenario_case_count"]) is not int:
        fail("scenario_case_count must be an exact JSON integer")
    require(inventory["scenario_case_count"], 54, "declared scenario count")
    require(inventory["required_entry_gates"], EXPECTED_ENTRY_GATES, "entry gates")
    require(inventory["review_checkpoints"], EXPECTED_REVIEW_CHECKPOINTS, "review checkpoints")
    require(inventory["closed_surfaces"], EXPECTED_CLOSED_SURFACES, "closed surfaces")
    next_transition = exact_keys(
        inventory["next_transition"],
        {"after_independent_acceptance", "before_independent_acceptance", "stage5h_open", "stage6_open"},
        "next transition",
    )
    require(next_transition["after_independent_acceptance"], "5G-b-mock-ack-attachment", "post-review transition")
    require(next_transition["before_independent_acceptance"], "design_only", "pre-review boundary")
    require(next_transition["stage5h_open"], False, "Stage 5H boundary")
    require(next_transition["stage6_open"], False, "Stage 6 boundary")


def validate(root: Path, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_closure(root)
    validate_inventory(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--skip-lineage", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), verify_lineage=not args.skip_lineage)
    except CheckFailure as exc:
        print(f"stage5g-entry-plan-check: failed: {exc}", file=sys.stderr)
        return 1
    print("stage5g-entry-plan-check: ok cases=54 design_only=true stage5f_closed=true")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
