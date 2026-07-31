#!/usr/bin/env python3
"""Negative mutations for the Stage 5F-c R2 reachability contract."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage5f_source_reachability_check.py"
SCENARIOS = "tests/fixtures/stage5/stage5f/v2/scenarios/atomic-hybrid-scenarios.json"
STATES = "tests/fixtures/stage5/stage5f/v2/states/imoexf-hybrid-state-seeds.json"
B0 = "docs/stage-5/stage5f-b0-source-reachability-inventory.json"
MAPPING = "docs/stage-5/stage5f-c-r2-row-semantics-mapping.json"
R2_INVENTORY = "docs/stage-5/stage5f-c-r2-source-reachability-inventory.json"
CANDIDATE = "docs/stage-5/stage5f-c-r1-candidate-results.json"
BREAKOUT = "crates/strategy-runtime-core/src/hybrid_intraday/intraday_breakout.rs"
HIGH180 = "crates/strategy-runtime-core/src/hybrid_intraday/high180.rs"
RUNTIME = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"


def read_json(root: Path, relative: str) -> dict:
    return json.loads((root / relative).read_text(encoding="utf-8"))


def write_json(root: Path, relative: str, value: dict) -> None:
    (root / relative).write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def row(payload: dict, row_id: str) -> dict:
    return next(item for item in payload["records"] if item["row_id"] == row_id)


def set_bar_time(root: Path, row_id: str, event: str, lifecycle: str, callback: str) -> None:
    payload = read_json(root, SCENARIOS)
    target = row(payload, row_id)
    target["bar"]["close_time_utc"] = event
    target["clock"] = {
        "event_ts_utc": event,
        "lifecycle_ts_utc": lifecycle,
        "callback_ts_utc": callback,
    }
    write_json(root, SCENARIOS, payload)


def mutate_scenario(root: Path, row_id: str, callback: Callable[[dict], None]) -> None:
    payload = read_json(root, SCENARIOS)
    callback(row(payload, row_id))
    write_json(root, SCENARIOS, payload)


def mutate_json(root: Path, relative: str, callback: Callable[[dict], None]) -> None:
    payload = read_json(root, relative)
    callback(payload)
    write_json(root, relative, payload)


def replace_text(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor cardinality drift for {relative}: {old!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT,
        destination,
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "__pycache__", "*.pyc"),
    )


def mapping_row(payload: dict, row_id: str) -> dict:
    return next(item for item in payload["rows"] if item["row_id"] == row_id)


CASES: list[tuple[str, Callable[[Path], None], bool]] = [
    (
        "f03-before-bo-wait",
        lambda root: set_bar_time(root, "F03", "2026-01-06T08:50:00Z", "2026-01-06T08:50:01Z", "2026-01-06T08:50:02Z"),
        True,
    ),
    (
        "f17-before-bo-wait",
        lambda root: set_bar_time(root, "F17", "2026-01-06T08:50:00Z", "2026-01-06T08:50:01Z", "2026-01-06T08:50:02Z"),
        True,
    ),
    ("f03-at-nonstrict-short-threshold", lambda root: mutate_scenario(root, "F03", lambda value: value["bar"].__setitem__("close", "97.88")), True),
    ("f05-at-nonstrict-stop2-threshold", lambda root: mutate_scenario(root, "F05", lambda value: value["bar"].__setitem__("close", "101.4")), True),
    ("f05-reason-reverted-to-generic-exit", lambda root: mutate_scenario(root, "F05", lambda value: value.__setitem__("case_id", "bo_short_normal_exit")), True),
    ("f12-price-completion-returned-to-bar-route", lambda root: mutate_scenario(root, "F12", lambda value: value.__setitem__("case_id", "mr_long_target_exit")), True),
    ("f14-stop-completion-returned-to-bar-route", lambda root: mutate_scenario(root, "F14", lambda value: value.__setitem__("case_id", "mr_long_stop_exit")), True),
    (
        "f16-impossible-simultaneous-runtime-row-restored",
        lambda root: mutate_scenario(
            root,
            "F16",
            lambda value: (
                value.__setitem__("case_id", "simultaneous_bo_mr_frozen_priority_winner"),
                value["expected"].update({"disposition": "accepted", "callback_count": 1, "settlement_attempt_count": 1}),
            ),
        ),
        True,
    ),
    (
        "f16-b0-classified-as-callback",
        lambda root: mutate_json(root, B0, lambda value: mapping_row(value, "F16").__setitem__("reachability", "source_callback_accepted")),
        True,
    ),
    (
        "f19-before-bo-wait",
        lambda root: set_bar_time(root, "F19", "2026-01-06T08:50:00Z", "2026-01-06T08:50:01Z", "2026-01-06T08:50:02Z"),
        True,
    ),
    (
        "f19-stale-mr-cycle",
        lambda root: mutate_json(
            root,
            STATES,
            lambda value: next(seed for seed in value["seeds"] if seed["seed_id"] == "mr_owner_active")["active_cycle"].update(
                {"created_ts_utc": "2026-01-06T06:00:00Z", "value": "695ca4e003"}
            ),
        ),
        True,
    ),
    ("f19-no-bo-candidate", lambda root: mutate_scenario(root, "F19", lambda value: value["bar"].__setitem__("close", "100.0")), True),
    ("f26-working-order-removed", lambda root: mutate_scenario(root, "F26", lambda value: value["broker_truth"].__setitem__("working_order_ids", [])), True),
    ("f26-empty-working-order-id", lambda root: mutate_scenario(root, "F26", lambda value: value["broker_truth"].__setitem__("working_order_ids", [""])), True),
    (
        "f26-timeout-plus-one-without-working-order",
        lambda root: (
            mutate_scenario(root, "F26", lambda value: value["broker_truth"].__setitem__("working_order_ids", [])),
            mutate_json(
                root,
                STATES,
                lambda value: next(seed for seed in value["seeds"] if seed["seed_id"] == "pending_entry")["pending_entry"].__setitem__(
                    "created_ts_utc", "2026-01-06T06:18:59Z"
                ),
            ),
        ),
        True,
    ),
    (
        "stage5g-owned-row-returned-to-stage5f",
        lambda root: mutate_json(root, MAPPING, lambda value: mapping_row(value, "F12").__setitem__("owner_stage", "Stage5FBarCallback")),
        True,
    ),
    (
        "classification-count-drift",
        lambda root: mutate_json(root, B0, lambda value: value["classification_summary"].__setitem__("source_callback_accepted", 23)),
        True,
    ),
    (
        "bo-wait-comparator-made-strict",
        lambda root: replace_text(root, BREAKOUT, "delta_h >= self.config.wait_hours", "delta_h > self.config.wait_hours"),
        True,
    ),
    (
        "high180-entry-window-extended",
        lambda root: replace_text(root, HIGH180, "entry_end_time: NaiveTime::from_hms_opt(11, 59, 59)", "entry_end_time: NaiveTime::from_hms_opt(12, 0, 0)"),
        True,
    ),
    (
        "high180-price-exit-injected-into-bar-owner",
        lambda root: replace_text(root, RUNTIME, "let max_hold = self.high180_mr.config().max_hold;", "let _forbidden = self.high180_mr.evaluate_exit(\n            todo!(), todo!(), todo!(), todo!(), todo!(),\n        );\n        let max_hold = self.high180_mr.config().max_hold;"),
        True,
    ),
    (
        "pending-working-order-guard-removed",
        lambda root: replace_text(
            root,
            RUNTIME,
            "fn clear_stale_pending_tail(&mut self, now_ts: i64, position_qty: f64) {\n"
            "        if position_qty.abs() > f64::EPSILON {\n"
            "            return;\n"
            "        }\n"
            "        if !self.working_orders.is_empty() || !self.working_stop_orders.is_empty() {",
            "fn clear_stale_pending_tail(&mut self, now_ts: i64, position_qty: f64) {\n"
            "        if position_qty.abs() > f64::EPSILON {\n"
            "            return;\n"
            "        }\n"
            "        if !self.working_stop_orders.is_empty() {",
        ),
        True,
    ),
    (
        "bar-off-ten-minute-grid",
        lambda root: set_bar_time(root, "F03", "2026-01-06T09:11:00Z", "2026-01-06T09:11:01Z", "2026-01-06T09:11:02Z"),
        True,
    ),
    (
        "broker-truth-schema-widened",
        lambda root: mutate_scenario(root, "F26", lambda value: value["broker_truth"].__setitem__("working_stop_ids", [])),
        True,
    ),
    (
        "corrected-row-mapping-removed",
        lambda root: mutate_json(root, MAPPING, lambda value: value["rows"].pop()),
        True,
    ),
    (
        "stage5f-d-opened-before-review",
        lambda root: mutate_json(root, R2_INVENTORY, lambda value: value["closed_surfaces"].__setitem__("stage5f_d", True)),
        True,
    ),
    (
        "seven-row-candidate-promoted-to-golden",
        lambda root: mutate_json(root, CANDIDATE, lambda value: value.__setitem__("status", "frozen_golden")),
        True,
    ),
    (
        "source-hash-drift-normal-mode",
        lambda root: (root / BREAKOUT).write_text((root / BREAKOUT).read_text(encoding="utf-8") + "\n// drift\n", encoding="utf-8"),
        False,
    ),
]


def main() -> int:
    failures = 0
    for label, mutate, isolated in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5f-r2-negative-") as temp:
            working = Path(temp) / "repo"
            copy_root(working)
            mutate(working)
            command = [sys.executable, str(working / CHECKER), "--root", str(working)]
            if isolated:
                command.append("--isolated-negative-harness")
            result = subprocess.run(command, capture_output=True, text=True)
            if result.returncode == 0:
                print(f"FAIL {label}: checker accepted mutation")
                failures += 1
            else:
                print(f"PASS {label}")
    if failures:
        print(f"stage5f-source-reachability-negative-harness: FAIL failures={failures}", file=sys.stderr)
        return 1
    print(f"stage5f-source-reachability-negative-harness: ok cases={len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
