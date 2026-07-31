#!/usr/bin/env python3
"""Fail-closed Stage 5F-b fixture/input contract checker."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import subprocess
import sys
import uuid
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = "docs/stage-5/stage5f-b-fixture-inventory.json"
B0_INVENTORY_PATH = "docs/stage-5/stage5f-b0-source-reachability-inventory.json"
PLAN_PATH = "docs/stage-5/5f-b-fixture-input-fingerprint-contract.md"
OBSERVATION_PATH = "docs/stage-5/stage5f-controlled-observation-extension.json"
FIXTURE_ROOT = "tests/fixtures/stage5/stage5f/v1"
SCENARIO_PATH = f"{FIXTURE_ROOT}/scenarios/atomic-hybrid-scenarios.json"
STATE_PATH = f"{FIXTURE_ROOT}/states/imoexf-hybrid-state-seeds.json"
RISKGATE_PATH = f"{FIXTURE_ROOT}/riskgate/imoexf-high180-riskgate-seeds.json"

STATE_SHA256 = "bb732fcebc0da78d3acdc88a3ceeb3db11a6a5a0719a92aeb91bcdcaf11729b4"
RISKGATE_SHA256 = "20e95ace0c1d92746c2198083d6b73fd0e78e1e58bc0b9b4bbcebf696fb5a1fc"
SCENARIO_SHA256 = "e83f10b58ba6c72efbf95d561edc9f7de84ce8e092129f6a9b449d2683e84184"
OBSERVATION_SHA256 = "0c9fa2ba5c509c57e3fa239b582d1f9a7938677a2bd9a23207acaaf32ec724ff"

TARGET = {
    "strategy_id": "hybrid_imoexf",
    "account_id": "ACC_TEST_0001",
    "instrument": {
        "symbol": "IMOEXF",
        "venue_symbol": "IMOEXF@RTSX",
        "exchange": "Moex",
        "market": "Futures",
    },
    "profile": "imoexf_primary_riskgate_high180_lb120",
    "paper_only": True,
}
DECIMAL = re.compile(r"^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$")
ROW_KEYS = {
    "bar",
    "case_id",
    "clock",
    "expected",
    "group_id",
    "owning_test",
    "pre_state",
    "riskgate",
    "row_id",
    "scenario_id",
    "schema_version",
    "target",
}
BAR_KEYS = {
    "close",
    "close_time_utc",
    "high",
    "is_final",
    "low",
    "open",
    "origin",
    "timeframe_sec",
    "volume",
}
CLOCK_KEYS = {"callback_ts_utc", "event_ts_utc", "lifecycle_ts_utc"}
REFERENCE_KEYS = {"catalog_path", "catalog_sha256", "seed_id"}
EXPECTED_KEYS = {
    "accepted_post_state_fingerprint",
    "b3f_outcome",
    "callback_count",
    "characterization_status",
    "disposition",
    "ordered_intent_vector",
    "ordered_intent_vector_sha256",
    "pre_state_fingerprint",
    "settlement_attempt_count",
}
STATE_SEED_KEYS = {
    "active_cycle_id",
    "current_owner",
    "current_side",
    "deferred_entry",
    "deferred_exit",
    "last_processed_bar_ts_utc",
    "orchestrator_state",
    "overnight_exit_armed_date",
    "pending_entry",
    "pending_exit",
    "position_qty",
    "seed_id",
    "state_class",
    "was_long_today",
    "was_short_today",
}
EXPECTED_STATE_IDS = {
    "flat_ready",
    "bo_long_open",
    "bo_short_open",
    "bo_long_carried",
    "mr_long_open",
    "mr_short_open",
    "bo_owner_active",
    "mr_owner_active",
    "duplicate_bar_flat",
    "post_cleanup_flat",
    "pending_entry",
    "pending_exit",
    "deferred_entry",
    "deferred_exit",
}
EXPECTED_RISKGATE_IDS = {
    "valid_normal_append",
    "missing_authority",
    "inconsistent_authority",
    "materialization_terminal",
}


class CheckFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            fail(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(), object_pairs_hook=strict_object
        )
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {relative}: {exc}")
    if not isinstance(value, dict):
        fail(f"{relative} must contain an object")
    return value


def exact_int(value: object, expected: int, label: str) -> None:
    if type(value) is not int or value != expected:
        fail(f"{label} must be exact JSON integer {expected}")


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    if set(value) != expected:
        fail(f"{label} key set drift")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_timestamp(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be RFC3339 UTC ending in Z")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        fail(f"{label} is not a valid timestamp: {exc}")
    if parsed.microsecond != 0:
        fail(f"{label} must use whole seconds")
    return parsed


def parse_decimal(value: object, label: str) -> float:
    if not isinstance(value, str) or not DECIMAL.fullmatch(value):
        fail(f"{label} must be a canonical decimal string")
    parsed = float(value)
    if not math.isfinite(parsed):
        fail(f"{label} must be finite")
    if parsed == 0.0 and value.startswith("-"):
        fail(f"{label} must not be negative zero")
    return parsed


def validate_inventory(root: Path) -> dict[str, Any]:
    inventory = read_json(root, INVENTORY_PATH)
    expected_keys = {
        "characterization_state_machine",
        "closed_surfaces",
        "current_characterization_state",
        "current_outputs_are_acceptance_evidence",
        "design_inputs",
        "fixture_catalogs",
        "ordered_intent_projection",
        "row_bindings",
        "schema_version",
        "source_reachability_commit",
        "source_reachability_inventory_sha256",
        "stage",
        "status",
    }
    exact_keys(inventory, expected_keys, "inventory")
    exact_int(inventory["schema_version"], 1, "inventory.schema_version")
    if inventory["stage"] != "5F-b1-fixture-input-fingerprint-contract":
        fail("inventory stage drift")
    if inventory["status"] != "contract_complete_outputs_pending_source_characterization":
        fail("inventory status drift")
    if inventory["source_reachability_commit"] != "d71af08804c9fc44c4f056cfa24396386d9ed94d":
        fail("source reachability commit drift")
    if inventory["source_reachability_inventory_sha256"] != sha256(
        root / B0_INVENTORY_PATH
    ):
        fail("source reachability inventory hash drift")
    if inventory["characterization_state_machine"] != [
        "pending_source_characterization",
        "candidate_source_characterized",
        "frozen_golden",
    ]:
        fail("characterization state machine drift")
    if inventory["current_characterization_state"] != "pending_source_characterization":
        fail("Stage 5F-b may only contain pending outputs")
    if inventory["current_outputs_are_acceptance_evidence"] is not False:
        fail("pending outputs may not be acceptance evidence")
    closed = inventory["closed_surfaces"]
    if not isinstance(closed, dict) or not closed or any(v is not False for v in closed.values()):
        fail("all Stage 5F-b closed surfaces must remain false")

    catalogs = exact_keys(
        inventory["fixture_catalogs"],
        {"observation_design", "riskgate", "scenarios", "states"},
        "fixture catalogs",
    )
    expected_catalogs = {
        "scenarios": (SCENARIO_PATH, SCENARIO_SHA256, 34),
        "states": (STATE_PATH, STATE_SHA256, 14),
        "riskgate": (RISKGATE_PATH, RISKGATE_SHA256, 4),
        "observation_design": (OBSERVATION_PATH, OBSERVATION_SHA256, 3),
    }
    for name, (path, digest, count) in expected_catalogs.items():
        entry = exact_keys(catalogs[name], {"path", "record_count", "sha256"}, name)
        if entry != {"path": path, "sha256": digest, "record_count": count}:
            fail(f"catalog inventory drift: {name}")
        if sha256(root / path) != digest:
            fail(f"catalog content hash drift: {name}")

    projection = inventory["ordered_intent_projection"]
    if not isinstance(projection, dict):
        fail("ordered projection must be an object")
    exact_int(projection.get("schema_version"), 1, "projection.schema_version")
    if projection.get("hash_domain") != "moex.stage5f.ordered-intent-vector.v1":
        fail("ordered projection domain drift")
    for key in (
        "sort_allowed",
        "non_finite_allowed",
        "negative_zero_allowed",
        "raw_sensitive_identifiers_allowed",
    ):
        if projection.get(key) is not False:
            fail(f"projection must keep {key}=false")
    if projection.get("floating_point_encoding") != "f64_to_bits_16_lowercase_hex":
        fail("floating-point projection encoding drift")
    return inventory


def validate_state_catalog(root: Path) -> set[str]:
    catalog = read_json(root, STATE_PATH)
    exact_keys(catalog, {"fixture_kind", "schema_version", "seed_defaults", "seeds", "target"}, "state catalog")
    exact_int(catalog["schema_version"], 1, "state catalog schema")
    if catalog["fixture_kind"] != "stage5f-hybrid-state-seed-catalog":
        fail("state catalog kind drift")
    target = dict(TARGET)
    target.pop("paper_only")
    if catalog["target"] != target:
        fail("state catalog target drift")
    defaults = exact_keys(
        catalog["seed_defaults"],
        {
            "current_day_close",
            "current_day_high",
            "current_day_low",
            "day_before_close",
            "entry_ready",
            "last_bar_close",
            "last_day_local",
            "next_cycle_seq",
            "prev_day_close",
            "prev_day_range",
            "safe_mode_close_only",
            "today_start_local",
        },
        "state seed defaults",
    )
    for key in (
        "current_day_close",
        "current_day_high",
        "current_day_low",
        "day_before_close",
        "last_bar_close",
        "prev_day_close",
        "prev_day_range",
    ):
        parse_decimal(defaults[key], f"state defaults.{key}")
    exact_int(defaults["next_cycle_seq"], 7, "state defaults.next_cycle_seq")
    if defaults["entry_ready"] is not True or defaults["safe_mode_close_only"] is not False:
        fail("state readiness defaults drift")
    parse_timestamp(defaults["today_start_local"] + "Z", "state defaults.today_start_local")

    seeds = catalog["seeds"]
    if not isinstance(seeds, list) or len(seeds) != 14:
        fail("state catalog must have 14 seeds")
    ids: set[str] = set()
    for seed in seeds:
        seed = exact_keys(seed, STATE_SEED_KEYS, "state seed")
        seed_id = seed["seed_id"]
        if not isinstance(seed_id, str) or not seed_id or seed_id in ids:
            fail("state seed IDs must be unique non-empty strings")
        ids.add(seed_id)
        parse_decimal(seed["position_qty"], f"{seed_id}.position_qty")
        parse_timestamp(seed["last_processed_bar_ts_utc"], f"{seed_id}.last_processed_bar_ts_utc")
        for bool_key in ("was_long_today", "was_short_today"):
            if type(seed[bool_key]) is not bool:
                fail(f"{seed_id}.{bool_key} must be bool")
        cycle = seed["active_cycle_id"]
        if cycle is not None and (not isinstance(cycle, str) or len(cycle.encode()) != 10):
            fail(f"{seed_id}.active_cycle_id must be null or exactly 10 bytes")
        for pending_name in ("pending_entry", "pending_exit"):
            pending = seed[pending_name]
            if pending is not None:
                expected = {"created_ts_utc", "cycle_id", "owner", "request_id", "side"}
                pending = exact_keys(pending, expected, f"{seed_id}.{pending_name}")
                try:
                    uuid.UUID(pending["request_id"])
                except (ValueError, TypeError, AttributeError) as exc:
                    fail(f"{seed_id}.{pending_name}.request_id invalid: {exc}")
                parse_timestamp(pending["created_ts_utc"], f"{seed_id}.{pending_name}.created_ts_utc")
        deferred_entry = seed["deferred_entry"]
        if deferred_entry is not None:
            deferred_entry = exact_keys(
                deferred_entry,
                {
                    "cycle_id",
                    "deferred_ts_utc",
                    "entry_style",
                    "original_request_id",
                    "owner",
                    "reason",
                    "side",
                    "stop_price",
                    "take_price",
                },
                f"{seed_id}.deferred_entry",
            )
            parse_decimal(deferred_entry["stop_price"], f"{seed_id}.deferred_entry.stop_price")
            parse_decimal(deferred_entry["take_price"], f"{seed_id}.deferred_entry.take_price")
            parse_timestamp(deferred_entry["deferred_ts_utc"], f"{seed_id}.deferred_entry.deferred_ts_utc")
        deferred_exit = seed["deferred_exit"]
        if deferred_exit is not None:
            deferred_exit = exact_keys(
                deferred_exit,
                {"cycle_id", "deferred_ts_utc", "original_request_id", "owner", "reason"},
                f"{seed_id}.deferred_exit",
            )
            parse_timestamp(deferred_exit["deferred_ts_utc"], f"{seed_id}.deferred_exit.deferred_ts_utc")
    if ids != EXPECTED_STATE_IDS:
        fail("state seed ID set drift")
    return ids


def validate_riskgate_catalog(root: Path) -> set[str]:
    catalog = read_json(root, RISKGATE_PATH)
    exact_keys(catalog, {"fixture_kind", "identity", "schema_version", "seeds"}, "riskgate catalog")
    exact_int(catalog["schema_version"], 1, "riskgate catalog schema")
    if catalog["fixture_kind"] != "stage5f-riskgate-seed-catalog":
        fail("riskgate catalog kind drift")
    identity = catalog["identity"]
    if not isinstance(identity, dict) or identity.get("profile_id") != "imoexf_primary_high180_lb120":
        fail("riskgate identity/profile drift")
    seeds = catalog["seeds"]
    if not isinstance(seeds, list) or len(seeds) != 4:
        fail("riskgate catalog must have four seeds")
    ids: set[str] = set()
    required_keys = {
        "authority_state",
        "enforced_for_entry",
        "expected_pre_callback_disposition",
        "last_finalized_session_date",
        "ledger_rows_count",
        "mr_enabled_current_session",
        "mr_enabled_next_session",
        "mr_gate_policy",
        "risk_gate_mode",
        "rolling_sum_lb120",
        "seed_id",
    }
    for seed in seeds:
        seed = exact_keys(seed, required_keys, "riskgate seed")
        seed_id = seed["seed_id"]
        if not isinstance(seed_id, str) or seed_id in ids:
            fail("riskgate seed IDs must be unique strings")
        ids.add(seed_id)
        if seed["risk_gate_mode"] != "normal_append" or seed["enforced_for_entry"] is not False:
            fail(f"{seed_id} must remain non-enforced normal_append")
        if seed["rolling_sum_lb120"] is not None:
            parse_decimal(seed["rolling_sum_lb120"], f"{seed_id}.rolling_sum_lb120")
        if type(seed["ledger_rows_count"]) is not int:
            fail(f"{seed_id}.ledger_rows_count must be exact integer")
    if ids != EXPECTED_RISKGATE_IDS:
        fail("riskgate seed ID set drift")
    return ids


def validate_scenarios(
    root: Path,
    inventory: dict[str, Any],
    state_ids: set[str],
    riskgate_ids: set[str],
) -> None:
    catalog = read_json(root, SCENARIO_PATH)
    exact_keys(catalog, {"characterization_policy", "fixture_kind", "records", "schema_version"}, "scenario catalog")
    exact_int(catalog["schema_version"], 1, "scenario catalog schema")
    if catalog["fixture_kind"] != "stage5f-atomic-hybrid-scenario-catalog":
        fail("scenario catalog kind drift")
    policy = catalog["characterization_policy"]
    if policy != {
        "current_status": "pending_source_characterization",
        "pending_outputs_are_acceptance_evidence": False,
        "source_callback_allowed_in_stage5f_b": False,
        "candidate_outputs_created_in_stage5f_c": True,
        "candidate_outputs_require_separate_freeze": True,
    }:
        fail("scenario characterization policy drift")
    records = catalog["records"]
    if not isinstance(records, list) or len(records) != 34:
        fail("scenario catalog must contain exactly 34 records")

    b0_rows = read_json(root, B0_INVENTORY_PATH)["rows"]
    if not isinstance(b0_rows, list):
        fail("b0 rows missing")
    b0_by_id = {row["row_id"]: row for row in b0_rows}
    bindings = inventory["row_bindings"]
    if not isinstance(bindings, list) or len(bindings) != 34:
        fail("inventory must bind exactly 34 rows")

    seen_scenarios: set[str] = set()
    seen_tests: set[str] = set()
    seen_rows: list[str] = []
    for index, record in enumerate(records, start=1):
        record = exact_keys(record, ROW_KEYS, f"scenario record {index}")
        exact_int(record["schema_version"], 1, f"scenario {index}.schema_version")
        row_id = record["row_id"]
        expected_row_id = f"F{index:02d}"
        if row_id != expected_row_id:
            fail(f"scenario row order drift: expected {expected_row_id}, got {row_id}")
        seen_rows.append(row_id)
        b0 = b0_by_id.get(row_id)
        if not isinstance(b0, dict):
            fail(f"scenario row absent from b0 audit: {row_id}")
        if record["group_id"] != b0["group_id"] or record["case_id"] != b0["case_id"]:
            fail(f"scenario identity drift from b0: {row_id}")
        scenario_id = record["scenario_id"]
        owning_test = record["owning_test"]
        if not isinstance(scenario_id, str) or scenario_id in seen_scenarios:
            fail(f"duplicate/invalid scenario_id at {row_id}")
        if not isinstance(owning_test, str) or owning_test in seen_tests:
            fail(f"duplicate/invalid owning_test at {row_id}")
        seen_scenarios.add(scenario_id)
        seen_tests.add(owning_test)
        if record["target"] != TARGET:
            fail(f"target contract drift at {row_id}")

        bar = exact_keys(record["bar"], BAR_KEYS, f"{row_id}.bar")
        if bar["origin"] != "Live" or bar["is_final"] is not True:
            fail(f"{row_id} must use final Live bar")
        exact_int(bar["timeframe_sec"], 600, f"{row_id}.bar.timeframe_sec")
        bar_ts = parse_timestamp(bar["close_time_utc"], f"{row_id}.bar.close_time_utc")
        for key in ("open", "high", "low", "close", "volume"):
            parse_decimal(bar[key], f"{row_id}.bar.{key}")
        if float(bar["high"]) < float(bar["low"]):
            fail(f"{row_id} bar high below low")

        clock = exact_keys(record["clock"], CLOCK_KEYS, f"{row_id}.clock")
        event = parse_timestamp(clock["event_ts_utc"], f"{row_id}.clock.event")
        callback = parse_timestamp(clock["callback_ts_utc"], f"{row_id}.clock.callback")
        lifecycle = parse_timestamp(clock["lifecycle_ts_utc"], f"{row_id}.clock.lifecycle")
        if event != bar_ts or not event <= callback <= lifecycle:
            fail(f"{row_id} clock chronology drift")

        state_ref = exact_keys(record["pre_state"], REFERENCE_KEYS, f"{row_id}.pre_state")
        if state_ref["catalog_path"] != STATE_PATH or state_ref["catalog_sha256"] != STATE_SHA256:
            fail(f"{row_id} state catalog binding drift")
        if state_ref["seed_id"] not in state_ids:
            fail(f"{row_id} unknown state seed")
        risk_ref = exact_keys(record["riskgate"], REFERENCE_KEYS, f"{row_id}.riskgate")
        if risk_ref["catalog_path"] != RISKGATE_PATH or risk_ref["catalog_sha256"] != RISKGATE_SHA256:
            fail(f"{row_id} riskgate catalog binding drift")
        if risk_ref["seed_id"] not in riskgate_ids:
            fail(f"{row_id} unknown riskgate seed")

        expected = exact_keys(record["expected"], EXPECTED_KEYS, f"{row_id}.expected")
        if expected["characterization_status"] != "pending_source_characterization":
            fail(f"{row_id} output must remain pending during Stage 5F-b")
        for key in (
            "pre_state_fingerprint",
            "accepted_post_state_fingerprint",
            "ordered_intent_vector",
            "ordered_intent_vector_sha256",
        ):
            if expected[key] is not None:
                fail(f"{row_id}.{key} must be null before source characterization")
        disposition = expected["disposition"]
        if disposition != b0["matrix_disposition"]:
            fail(f"{row_id} disposition drift from b0")
        counts = {
            "accepted": (1, 1),
            "blocked_before_callback": (0, 0),
            "terminal_after_callback": (1, 1),
        }.get(disposition)
        if counts is None:
            fail(f"{row_id} unknown disposition")
        exact_int(expected["callback_count"], counts[0], f"{row_id}.callback_count")
        exact_int(
            expected["settlement_attempt_count"],
            counts[1],
            f"{row_id}.settlement_attempt_count",
        )
        binding = bindings[index - 1]
        if binding != {
            "row_id": row_id,
            "scenario_id": scenario_id,
            "disposition": disposition,
            "owning_test": owning_test,
        }:
            fail(f"inventory/scenario binding drift at {row_id}")

    if seen_rows != [f"F{index:02d}" for index in range(1, 35)]:
        fail("scenario row set/order drift")
    if Counter(record["expected"]["disposition"] for record in records) != Counter(
        {"accepted": 27, "blocked_before_callback": 3, "terminal_after_callback": 4}
    ):
        fail("scenario disposition count drift")


def validate_observation_design(root: Path) -> None:
    design = read_json(root, OBSERVATION_PATH)
    if design.get("status") != "design_only_not_implemented":
        fail("observation design must remain unimplemented in Stage 5F-b")
    regions = design.get("approved_regions")
    if not isinstance(regions, list) or len(regions) != 3:
        fail("observation design must authorize exactly three regions")
    invariants = design.get("invariants")
    if not isinstance(invariants, dict):
        fail("observation invariants missing")
    expected_true = {
        "cfg_test_only",
        "crate_private",
        "returned_vector_unchanged",
        "single_callback",
        "single_observer_consume",
    }
    expected_false = {
        "frozen_b3f_source_changed",
        "frozen_stage5c_source_changed",
        "observer_controls_runtime_flow",
        "observer_survives_scenario",
        "raw_sensitive_fields_exported",
    }
    if any(invariants.get(key) is not True for key in expected_true):
        fail("positive observation invariant drift")
    if any(invariants.get(key) is not False for key in expected_false):
        fail("negative observation invariant drift")


def validate_no_unbound_files(root: Path) -> None:
    actual = sorted(
        path.relative_to(root).as_posix()
        for path in (root / FIXTURE_ROOT).rglob("*.json")
    )
    expected = sorted([RISKGATE_PATH, SCENARIO_PATH, STATE_PATH])
    if actual != expected:
        fail(f"unbound Stage 5F fixture file set: {actual!r}")


def validate_contract_only_source(root: Path) -> None:
    if (root / "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs").exists():
        fail("observer implementation is forbidden in Stage 5F-b")
    if (root / ".git").exists():
        changed = subprocess.run(
            [
                "git",
                "diff",
                "--name-only",
                "d71af08804c9fc44c4f056cfa24396386d9ed94d",
                "--",
                "crates",
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.splitlines()
        if changed:
            fail(f"Stage 5F-b changed Rust/crate source: {changed!r}")


def validate_plan(root: Path) -> None:
    text = (root / PLAN_PATH).read_text()
    for fragment in (
        "pending_source_characterization",
        "candidate_source_characterized",
        "frozen_golden",
        "Those requirements are circular",
        "sha256(serde_json::to_vec(StrategyState))",
        "Vector order is never sorted.",
        "Redis, FINAM, HTTP POST/DELETE",
    ):
        if fragment not in text:
            fail(f"fixture contract document fragment missing: {fragment}")


def check(root: Path) -> None:
    inventory = validate_inventory(root)
    state_ids = validate_state_catalog(root)
    riskgate_ids = validate_riskgate_catalog(root)
    validate_scenarios(root, inventory, state_ids, riskgate_ids)
    validate_observation_design(root)
    validate_no_unbound_files(root)
    validate_contract_only_source(root)
    validate_plan(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (CheckFailure, OSError, subprocess.CalledProcessError) as exc:
        print(f"stage5f-fixture-contract-check: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        "stage5f-fixture-contract-check: ok "
        "groups=16 rows=34 state_seeds=14 riskgate_seeds=4 outputs=pending"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
