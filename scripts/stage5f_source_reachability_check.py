#!/usr/bin/env python3
"""Fail-closed Stage 5F-c R3 runtime-consumed reachability checker."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timedelta
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
STAGE = "5F-c-R3-runtime-consumed-reachability-closure"
PREDECESSOR = "a1ae44e872348f3a942674e1472268d0eb1bfd84"
SCENARIOS = "tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json"
STATES = "tests/fixtures/stage5/stage5f/v2/states/imoexf-hybrid-state-seeds.json"
CONFIG = "tests/fixtures/stage5/stage5f/v2/config/imoexf-target-config.json"
B0_INVENTORY = "docs/stage-5/stage5f-b0-source-reachability-inventory.json"
R3_INVENTORY = "docs/stage-5/stage5f-c-r3-runtime-consumed-reachability-inventory.json"
MAPPING = "docs/stage-5/stage5f-c-r3-row-semantics-mapping.json"
REPORT = "docs/stage-5/5f-c-r3-runtime-consumed-reachability-closure.md"
CANDIDATE = "docs/stage-5/stage5f-c-r1-candidate-results.json"
NEGATIVE_HARNESS = "scripts/stage5f_source_reachability_negative_harness.py"

EXPECTED_SOURCE_BINDINGS = {
    "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs": "a3b125f282f201b66dfa8d2685f22aa94048856a5145d537b76dc8934a5f9ae5",
    "crates/strategy-runtime-core/src/hybrid_intraday/high180.rs": "e1f39a3afdf9745682682da0083f97ac0fa5361f979525d5ea383d6a6aa64456",
    "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs": "1e784411d348fcf090887f7f50062b0cbd34494912288100c1ca1d851d8d5bd9",
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": "aa514c2479a2720a585ce0c386ab91674e125582e013912fba49fe529f8bdd2d",
    "crates/strategy-runtime-core/src/stage5d_persistence.rs": "f790a907d6730e26e731a78ef89c58f993b39acde6ce934602e2fe603d90f083",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": "a2dad58a52b6fed8f3593861a4cc398dcea6355ddc1b3782296395f29c0416e3",
    "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs": "649bf7f10e92b57c7b15709f86829e64b1fd746241ae29578d685deedb7ef593",
}
EXPECTED_FIXTURE_BINDINGS = {
    CONFIG: "3c46aa4bdfb5a6ac3350d0f3b52ad5050abc472c653bacda512dffebfeb07e41",
    SCENARIOS: "a9cfe51f3393b2f0d45881029e251455d727bf233fde9751fd3f2278e793e55e",
    STATES: "93d45c1c003a23ddd0384250a555e694f8d87c7645853bad83598a9df8061901",
    B0_INVENTORY: "fe9a8f7fd82f1a938c0632710ffb01122ba51f9ba1748020c5f970753083522c",
    "docs/stage-5/stage5f-c-r1-schema-owner-inventory.json": "3c18d5711bdedde27d0ea3cc11e1d642cebca3426882f01cc14755349a225f6c",
    CANDIDATE: "c4b4da6a399ae83aa55d45e42c75750a4f3ffca3248b06be7033bc9e4195b954",
}
EXPECTED_DOCUMENT_HASHES = {
    R3_INVENTORY: "7c248ccd62083f3c7d0badfec1774500996723f6b4a2e2c947ccc290db3b66cd",
    MAPPING: "b1ae5c94a5c76ef8e9a71e6370f61aabbbc13ebf01297035ca8f99fd7eda2f51",
    REPORT: "00a73a9d0bf2db022af5525bfeab0c331e3bbc465bd046e319cd6c10b3960df4",
    NEGATIVE_HARNESS: "dc7a50f47bf22bbd11381ce5913bb2aa6107ac5d95bc7d13f49d695f7bb503ba",
}
CORRECTED_ROWS = ["F03", "F05", "F12", "F13", "F14", "F15", "F16", "F17", "F19", "F26"]
R3_ROWS = ["F12", "F13", "F14", "F15", "F19", "F26"]
SEVEN_ROWS = ["F01", "F02", "F04", "F24", "F31", "F32", "F33"]
CLASSIFICATION = {
    "row_count": 34,
    "source_callback_accepted": 22,
    "source_callback_no_bar_exit_invariant": 4,
    "source_profile_structural_invariant": 1,
    "source_chain_blocked_before_callback": 3,
    "test_negative_terminal_after_callback": 4,
    "protective_completion_deferred_to_stage5g": 4,
}
EXPECTED_CASES = {
    "F03": "bo_short_entry",
    "F05": "bo_short_stop2_exit",
    "F12": "mr_long_favorable_extreme_no_bar_exit",
    "F13": "mr_short_favorable_extreme_no_bar_exit",
    "F14": "mr_long_adverse_extreme_no_bar_exit",
    "F15": "mr_short_adverse_extreme_no_bar_exit",
    "F16": "active_profile_bo_mr_windows_do_not_overlap",
    "F17": "bo_selected_when_mr_ineligible",
    "F19": "mr_owner_suppresses_bo",
    "F26": "pending_entry_no_new_entry_or_fake_feedback",
}
EXPECTED_OWNER_STAGES = {
    "F03": "Stage5FBarCallback",
    "F05": "Stage5FBarCallback",
    "F12": "Stage5FInvariantStage5GCompletion",
    "F13": "Stage5FInvariantStage5GCompletion",
    "F14": "Stage5FInvariantStage5GCompletion",
    "F15": "Stage5FInvariantStage5GCompletion",
    "F16": "Stage5FStructuralInvariant",
    "F17": "Stage5FBarCallback",
    "F19": "Stage5FBarCallback",
    "F26": "Stage5FBarCallback",
}


class ReachabilityFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ReachabilityFailure(message)


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
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def require(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def sha256_file(root: Path, relative: str) -> str:
    try:
        return hashlib.sha256((root / relative).read_bytes()).hexdigest()
    except OSError as exc:
        fail(f"cannot hash {relative}: {exc}")


def decimal(value: object, label: str) -> Decimal:
    if not isinstance(value, str):
        fail(f"{label} must be an exact decimal string")
    try:
        parsed = Decimal(value)
    except InvalidOperation as exc:
        fail(f"{label} is invalid: {exc}")
    if not parsed.is_finite():
        fail(f"{label} must be finite")
    return parsed


def parse_utc(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be UTC")
    try:
        return datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        fail(f"{label} is invalid: {exc}")


def parse_local(value: object, label: str) -> datetime:
    if not isinstance(value, str):
        fail(f"{label} must be a local datetime string")
    try:
        return datetime.fromisoformat(value)
    except ValueError as exc:
        fail(f"{label} is invalid: {exc}")


def function_region(source: str, start: str, end: str, label: str) -> str:
    if source.count(start) != 1:
        fail(f"{label} start anchor drift")
    begin = source.index(start)
    finish = source.find(end, begin + len(start))
    if finish < 0:
        fail(f"{label} end anchor drift")
    return source[begin:finish]


def validate_source_bindings(root: Path, check_hashes: bool) -> None:
    if check_hashes:
        for relative, expected in EXPECTED_SOURCE_BINDINGS.items():
            require(sha256_file(root, relative), expected, f"source binding {relative}")
    breakout = (root / "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs").read_text()
    high180 = (root / "crates/strategy-runtime-core/src/hybrid_intraday/high180.rs").read_text()
    orchestrator = (root / "crates/strategy-runtime-core/src/hybrid_intraday/orchestrator.rs").read_text()
    runtime = (root / "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs").read_text()
    stage5d = (root / "crates/strategy-runtime-core/src/stage5d_persistence.rs").read_text()
    stage5e = (root / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs").read_text()
    stage5f = (root / "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs").read_text()
    for snippet in (
        "delta_h >= self.config.wait_hours",
        "if close > stop2_level",
        "if dt.minute() == 50",
    ):
        if snippet not in breakout:
            fail(f"BO source rule missing: {snippet}")
    for snippet in (
        "entry_end_time: NaiveTime::from_hms_opt(11, 59, 59)",
        "max_hold: Duration::minutes(180)",
        "if ts.time() > self.config.entry_end_time",
    ):
        if snippet not in high180:
            fail(f"High180 source rule missing: {snippet}")
    owner_region = function_region(
        orchestrator,
        "pub fn on_bar_with_mr_override(",
        "if bar.has_open_position && self.current_owner == Some(Owner::IntradayBreakout)",
        "MR owner guard",
    )
    if "let Some(reason) = mr_exit_reason else {\n                return actions;" not in owner_region:
        fail("MR owner/open-position guard drift")
    exit_region = function_region(
        runtime,
        "fn high180_exit_reason(",
        "fn author41_boundary_short_entry_signal(",
        "High180 integrated exit owner",
    )
    if "ReasonCode::MeanRevTimeCutoff" not in exit_region or ".evaluate_exit(" in exit_region:
        fail("integrated High180 bar exit ownership drift")
    gc_region = function_region(
        runtime,
        "fn clear_stale_pending_tail(",
        "fn clear_boot_stale_pending_tail(",
        "pending GC",
    )
    if (
        "if !self.working_orders.is_empty() || !self.working_stop_orders.is_empty()" not in gc_region
        or "now_ts.saturating_sub(created) > timeout" not in gc_region
    ):
        fail("pending GC source rule drift")
    for snippet in (
        "stage5f_f19_mr_owner_suppresses_paired_source_valid_bo_candidate",
        "let control = characterize_input(control_input, Stage5fMutation::None);",
        "assert_eq!(control.ordered_intent_vector.len(), 1);",
        "assert_eq!(control_input.bar, owner_input.bar);",
        "assert!(owner.ordered_intent_vector.is_empty());",
        "stage5f_f26_working_order_reaches_runtime_and_retains_stale_pending",
        ".broker_truth_active_orders",
        "broker_truth_active_orders,\n    }",
        "expected_working_order_ids: input",
        ".stage5d_apply_runtime_private_extension(&private)",
        "F26 callback must retain the broker-truth working order",
        "F26 callback must retain the exact pending request",
        "F26 callback must preserve the active cycle",
        "assert!(result.ordered_intent_vector.is_empty());",
    ):
        if snippet not in stage5f:
            fail(f"Stage 5F runtime-consumption proof missing: {snippet}")
    for snippet in (
        ".stage5d_test_with_target_active_orders(target_active_orders)",
        "assert_eq!(broker_truth_order_ids, expected_active_order_ids);",
        "strict_envelope.recovery_indexes.known_order_ids",
    ):
        if snippet not in stage5d:
            fail(f"Stage 5D active-order chain proof missing: {snippet}")
    for snippet in (
        "pub(crate) fn test_runtime_private_extension",
        "pub(crate) fn test_strategy_state_value",
    ):
        if snippet not in stage5e:
            fail(f"Stage 5E post-callback proof missing: {snippet}")


def validate_documents(root: Path, check_hashes: bool) -> None:
    if check_hashes:
        for relative, expected in EXPECTED_DOCUMENT_HASHES.items():
            require(sha256_file(root, relative), expected, f"document binding {relative}")
    inventory = read_json(root, R3_INVENTORY)
    exact_keys(
        inventory,
        {
            "classification_summary",
            "closed_surfaces",
            "corrected_rows",
            "fixture_bindings",
            "inherited_seven_row_candidate",
            "next_stage",
            "predecessor_ref",
            "schema_version",
            "source_bindings",
            "stage",
            "status",
            "target",
        },
        "R3 inventory",
    )
    require(inventory["schema_version"], 1, "R3 inventory schema")
    require(inventory["stage"], STAGE, "R3 inventory stage")
    require(inventory["status"], "review_required_before_5f_d", "R3 status")
    require(inventory["predecessor_ref"], PREDECESSOR, "R3 predecessor")
    require(inventory["source_bindings"], EXPECTED_SOURCE_BINDINGS, "R3 source bindings")
    require(inventory["fixture_bindings"], EXPECTED_FIXTURE_BINDINGS, "R3 fixture bindings")
    require(inventory["classification_summary"], CLASSIFICATION, "R3 classification")
    require(inventory["corrected_rows"], R3_ROWS, "R3 corrected row order")
    if any(value is not False for value in inventory["closed_surfaces"].values()):
        fail("R3 inventory opened a closed surface")
    require(inventory["next_stage"]["allowed_before_independent_review"], False, "5F-d hold")
    inherited = inventory["inherited_seven_row_candidate"]
    require(inherited["row_ids"], SEVEN_ROWS, "seven-row inheritance")
    require(inherited["results_array_sha256"], "e02643a004e9b325a276a1a65e41eda45347e8bb6efe0b0269e838099d909c81", "seven-row digest")
    require(inherited["semantic_outputs_changed"], False, "seven-row semantic immutability")

    mapping = read_json(root, MAPPING)
    exact_keys(
        mapping,
        {"closed_decisions", "predecessor_ref", "rows", "schema_version", "stage", "status"},
        "row mapping",
    )
    require(mapping["stage"], STAGE, "mapping stage")
    require(mapping["predecessor_ref"], PREDECESSOR, "mapping predecessor")
    require([row.get("row_id") for row in mapping["rows"]], R3_ROWS, "mapping row order")
    for row in mapping["rows"]:
        exact_keys(
            row,
            {"correction", "new_case_id", "old_case_id", "owner_stage", "row_id"},
            f"{row.get('row_id')} mapping row",
        )
        require(row["new_case_id"], EXPECTED_CASES[row["row_id"]], f"{row['row_id']} mapped case")
        require(
            row["owner_stage"],
            EXPECTED_OWNER_STAGES[row["row_id"]],
            f"{row['row_id']} mapped owner stage",
        )
    if any(value is not False for value in mapping["closed_decisions"].values()):
        fail("row mapping opened a forbidden decision")

    b0 = read_json(root, B0_INVENTORY)
    require(b0["status"], "r3_runtime_consumed_reachability_non_golden", "B0 status")
    require(b0["classification_summary"], CLASSIFICATION | {"official_group_count": 16}, "B0 classification")
    b0_rows = {row["row_id"]: row for row in b0["rows"]}
    for row_id, case_id in EXPECTED_CASES.items():
        require(b0_rows[row_id]["case_id"], case_id, f"{row_id} B0 case")
    for row_id in ("F12", "F13", "F14", "F15"):
        require(b0_rows[row_id]["reachability"], "source_callback_no_bar_exit_invariant", f"{row_id} ownership")
    require(b0_rows["F16"]["reachability"], "source_profile_structural_invariant", "F16 reachability")

    candidate = read_json(root, CANDIDATE)
    require(candidate["status"], "candidate_source_characterized_not_golden", "candidate status")
    require([row["row_id"] for row in candidate["results"]], SEVEN_ROWS, "candidate rows")
    require(candidate["generation"]["results_array_sha256"], "e02643a004e9b325a276a1a65e41eda45347e8bb6efe0b0269e838099d909c81", "candidate digest")


def validate_reachability(root: Path) -> None:
    scenarios = read_json(root, SCENARIOS)
    states = read_json(root, STATES)
    config = read_json(root, CONFIG)
    require(scenarios["status"], "canonical_r3_non_golden", "scenario revision")
    records = scenarios["records"]
    if not isinstance(records, list) or len(records) != 34:
        fail("scenario catalog must contain 34 rows")
    require([row.get("row_id") for row in records], [f"F{i:02d}" for i in range(1, 35)], "scenario order")
    rows = {row["row_id"]: row for row in records}
    seeds = {seed["seed_id"]: seed for seed in states["seeds"]}

    timezone_offset = config["timezone_offset_hours"]
    if type(timezone_offset) is not int:
        fail("timezone offset must be an integer")
    session_start = parse_local(states["seed_defaults"]["today_start_local"], "today start")
    wait_seconds = int(decimal(config["breakout"]["wait_hours"], "wait_hours") * Decimal(3600))
    bo_earliest = session_start + timedelta(seconds=wait_seconds)
    def bo_eligible(observed: datetime) -> bool:
        return observed >= bo_earliest

    if not bo_eligible(bo_earliest) or bo_eligible(bo_earliest - timedelta(microseconds=1)):
        fail("BO exact-boundary proof failed")
    prev_close = decimal(states["seed_defaults"]["prev_day_close"], "prev close")
    prev_range = decimal(states["seed_defaults"]["prev_day_range"], "prev range")
    k = decimal(config["breakout"]["k"], "BO k")
    stop2 = decimal(config["breakout"]["stop2_range"], "BO stop2")
    long_level = prev_close + k * prev_range
    short_level = prev_close - k * prev_range
    short_stop2 = prev_close + stop2 * prev_range
    high180_end = datetime.strptime("11:59:59", "%H:%M:%S").time()
    max_hold = timedelta(minutes=180)
    if not bo_earliest.time() > high180_end:
        fail("active-profile BO/High180 windows unexpectedly overlap")

    for row in records:
        exact_keys(
            row,
            {"bar", "broker_truth", "case_id", "clock", "expected", "group_id", "owning_test", "pre_state", "riskgate", "row_id", "scenario_id", "schema_version", "target"},
            f"{row['row_id']} scenario",
        )
        truth = exact_keys(row["broker_truth"], {"active_orders"}, f"{row['row_id']} broker truth")
        orders = truth["active_orders"]
        if not isinstance(orders, list):
            fail(f"{row['row_id']} active-order evidence must be an array")
        order_ids = []
        for order in orders:
            exact_keys(
                order,
                {
                    "account_id", "broker_order_id", "client_order_id", "existing",
                    "filled_qty", "instrument", "limit_price", "order_type",
                    "ownership_comment", "qty", "received_ts_utc", "side",
                    "source_ts_utc", "status",
                },
                f"{row['row_id']} active order",
            )
            require(order["account_id"], row["target"]["account_id"], f"{row['row_id']} order account")
            require(order["instrument"], row["target"]["instrument"], f"{row['row_id']} order instrument")
            require((order["status"], order["side"], order["order_type"], order["existing"]), ("working", "buy", "limit", True), f"{row['row_id']} active order semantics")
            if not isinstance(order["broker_order_id"], str) or not order["broker_order_id"]:
                fail(f"{row['row_id']} broker order id invalid")
            if not isinstance(order["client_order_id"], str) or not 1 <= len(order["client_order_id"]) <= 20:
                fail(f"{row['row_id']} client order id invalid")
            if not isinstance(order["ownership_comment"], str) or not order["ownership_comment"].startswith("HYB|sid=hybrid_imoexf|"):
                fail(f"{row['row_id']} ownership attribution invalid")
            qty = decimal(order["qty"], f"{row['row_id']} order qty")
            filled = decimal(order["filled_qty"], f"{row['row_id']} filled qty")
            if not qty > 0 or not Decimal(0) <= filled < qty:
                fail(f"{row['row_id']} active order quantities invalid")
            decimal(order["limit_price"], f"{row['row_id']} limit price")
            source_ts = parse_utc(order["source_ts_utc"], f"{row['row_id']} order source ts")
            received_ts = parse_utc(order["received_ts_utc"], f"{row['row_id']} order received ts")
            if source_ts > received_ts:
                fail(f"{row['row_id']} order chronology invalid")
            order_ids.append(order["broker_order_id"])
        if len(order_ids) != len(set(order_ids)):
            fail(f"{row['row_id']} duplicate active broker order id")
        event = parse_utc(row["bar"]["close_time_utc"], f"{row['row_id']} bar")
        if event.minute % 10 != 0 or event.second != 0 or event.microsecond != 0:
            fail(f"{row['row_id']} is not aligned to the 10-minute bar grid")
        require(row["clock"]["event_ts_utc"], row["bar"]["close_time_utc"], f"{row['row_id']} event binding")

    def local_bar(row_id: str) -> datetime:
        return parse_utc(rows[row_id]["bar"]["close_time_utc"], row_id).replace(tzinfo=None) + timedelta(hours=timezone_offset)

    for row_id in ("F03", "F17", "F19"):
        if local_bar(row_id) < bo_earliest:
            fail(f"{row_id} occurs before the BO wait boundary")
    if not decimal(rows["F03"]["bar"]["close"], "F03 close") < short_level:
        fail("F03 does not cross the strict BO short threshold")
    if not decimal(rows["F05"]["bar"]["close"], "F05 close") > short_stop2:
        fail("F05 does not cross the strict BO short stop2 threshold")
    require(rows["F05"]["case_id"], EXPECTED_CASES["F05"], "F05 reason")
    if not local_bar("F17").time() > high180_end:
        fail("F17 does not prove MR ineligibility after the High180 cutoff")
    f17_close = decimal(rows["F17"]["bar"]["close"], "F17 close")
    if not (f17_close > long_level or f17_close < short_level):
        fail("F17 has no source-valid BO candidate")

    f19 = rows["F19"]
    f19_close = decimal(f19["bar"]["close"], "F19 close")
    if not f19_close > long_level:
        fail("F19 paired control does not cross the strict BO long threshold")
    f19_seed = seeds[f19["pre_state"]["seed_id"]]
    require((f19_seed["current_owner"], f19_seed["orchestrator_state"]), ("mean_reversion", "open"), "F19 owner state")
    if decimal(f19_seed["position_qty"], "F19 position") == 0:
        fail("F19 must own an open MR position")
    cycle_created = parse_utc(f19_seed["active_cycle"]["created_ts_utc"], "F19 cycle")
    cycle_age = parse_utc(f19["bar"]["close_time_utc"], "F19 bar") - cycle_created
    if not timedelta(0) < cycle_age < max_hold:
        fail("F19 MR cycle is not recent enough to isolate owner suppression")
    require((f19_seed["was_long_today"], f19_seed["was_short_today"]), (False, False), "F19 paired control side history")
    min_range = decimal(config["breakout"]["min_range"], "BO min range")
    if prev_range < min_range:
        fail("F19 paired control is blocked by the BO minimum-range filter")
    prev_return = decimal(states["seed_defaults"]["prev_day_return"], "previous-day return")
    big_move = decimal(config["breakout"]["big_move_threshold"], "BO big-move threshold")
    if prev_return < -big_move:
        fail("F19 paired BO long control is blocked by big-move eligibility")
    f19_local = local_bar("F19")
    if config["breakout"]["exclude_weekends"] is not True or f19_local.weekday() >= 5:
        fail("F19 paired control is not weekday/session eligible")

    no_bar_cases = {
        "F12": ("mean_reversion", "long", "favorable"),
        "F13": ("mean_reversion", "short", "favorable"),
        "F14": ("mean_reversion", "long", "adverse"),
        "F15": ("mean_reversion", "short", "adverse"),
    }
    for row_id, (owner, side, direction) in no_bar_cases.items():
        row = rows[row_id]
        require(row["case_id"], EXPECTED_CASES[row_id], f"{row_id} case")
        seed = seeds[row["pre_state"]["seed_id"]]
        require((seed["current_owner"], seed["current_side"]), (owner, side), f"{row_id} MR ownership")
        age = parse_utc(row["bar"]["close_time_utc"], f"{row_id} bar") - parse_utc(seed["active_cycle"]["created_ts_utc"], f"{row_id} cycle")
        if not timedelta(0) < age < max_hold:
            fail(f"{row_id} accidentally reaches the bar-owned max-hold exit")
        close = decimal(row["bar"]["close"], f"{row_id} close")
        expected_direction = close > prev_close if (side, direction) in {("long", "favorable"), ("short", "adverse")} else close < prev_close
        if not expected_direction:
            fail(f"{row_id} price direction does not match its no-bar-exit invariant")
        require((row["expected"]["disposition"], row["expected"]["callback_count"], row["expected"]["settlement_attempt_count"]), ("accepted", 1, 1), f"{row_id} callback invariant")

    f16 = rows["F16"]
    require(f16["case_id"], EXPECTED_CASES["F16"], "F16 case")
    require((f16["expected"]["disposition"], f16["expected"]["callback_count"], f16["expected"]["settlement_attempt_count"]), ("structural_invariant", 0, 0), "F16 structural proof")

    f26 = rows["F26"]
    pending = seeds[f26["pre_state"]["seed_id"]]["pending_entry"]
    if pending is None:
        fail("F26 pending entry is absent")
    pending_age = parse_utc(f26["bar"]["close_time_utc"], "F26 bar") - parse_utc(pending["created_ts_utc"], "F26 pending")
    timeout = timedelta(seconds=config["pending_timeout_sec"])
    working_orders = [
        order["broker_order_id"] for order in f26["broker_truth"]["active_orders"]
    ]
    if pending_age > timeout and not working_orders:
        fail("F26 stale pending would be garbage-collected without working-order evidence")
    require(working_orders, ["ORDER_F26_WORKING"], "F26 synthetic working-order evidence")
    def pending_survives(age: timedelta, has_working_order: bool) -> bool:
        return has_working_order or age <= timeout

    if (
        timeout != timedelta(seconds=60)
        or not pending_survives(timeout, False)
        or pending_survives(timeout + timedelta(seconds=1), False)
        or not pending_survives(timeout + timedelta(seconds=1), True)
    ):
        fail("pending exact-timeout/timeout-plus-one proof failed")


def run(root: Path, check_hashes: bool) -> None:
    validate_source_bindings(root, check_hashes)
    if check_hashes:
        for relative, expected in EXPECTED_FIXTURE_BINDINGS.items():
            require(sha256_file(root, relative), expected, f"fixture binding {relative}")
    validate_documents(root, check_hashes)
    validate_reachability(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--isolated-negative-harness", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        run(args.root.resolve(), check_hashes=not args.isolated_negative_harness)
    except (ReachabilityFailure, OSError, UnicodeDecodeError, ValueError, KeyError, TypeError) as exc:
        print(f"stage5f-source-reachability-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print("stage5f-source-reachability-check: ok rows=34 corrected=10 stage5f_d=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
