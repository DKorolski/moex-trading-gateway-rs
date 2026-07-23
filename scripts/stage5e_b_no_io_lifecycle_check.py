#!/usr/bin/env python3
"""Pin the Stage 5E-b foundation to an explicit no-I/O scope."""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md"
INVENTORY = ROOT / "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json"
FREEZE_REF = "eb03695dc407b02bb8327de57fde6acea077d96b"
BASELINE_REF = "ce08d71f2ab763a4915e90385c7487bec1581c25"
EXPECTED_TOP_LEVEL_KEYS = {
    "allowed_changed_paths", "baseline_ref", "closed_surfaces", "schema_version",
    "contract_invariants", "source_stage5d_aggregate_closure_r2_ref", "stage",
    "stage5e_a_freeze_ref", "status",
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md",
    "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
    "scripts/stage5e_descriptor.py",
]
CLOSED = {
    "redis", "finam", "transport", "dispatch", "runtime_live",
    "broker_execution", "strategy_intent_sink", "autonomous_event_loop",
}
EXPECTED_CONTRACT_INVARIANTS = {
    "market_freshness_relation": "strict_lt",
    "first_live_bar_mode": "observation_only",
    "callback_count": 0,
    "intent_count": 0,
    "calls_strategy": False,
    "creates_executable_intent": False,
}


def fail(message: str) -> None:
    print(f"stage5e-b-no-io-lifecycle-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    if not PLAN.is_file() or not INVENTORY.is_file():
        fail("missing Stage 5E-b plan or inventory")
    inventory = json.loads(INVENTORY.read_text())
    if set(inventory) != EXPECTED_TOP_LEVEL_KEYS:
        fail("inventory key set drift")
    if inventory.get("schema_version") != 1:
        fail("schema_version must be 1")
    if inventory.get("stage") != "5E-b-no-io-lifecycle-capability":
        fail("unexpected stage")
    if inventory.get("status") != "implementation_foundation":
        fail("unexpected status")
    if inventory.get("baseline_ref") != BASELINE_REF:
        fail("Stage 5E-b baseline reference mismatch")
    if inventory.get("stage5e_a_freeze_ref") != FREEZE_REF:
        fail("Stage 5E-a freeze reference mismatch")
    if inventory.get("source_stage5d_aggregate_closure_r2_ref") != "9ebbfd29d0346be5149dac746225866f0c8d0257":
        fail("Stage 5D source reference mismatch")
    closed = inventory.get("closed_surfaces")
    if not isinstance(closed, dict) or set(closed) != CLOSED:
        fail("closed surface set drift")
    if any(value is not False for value in closed.values()):
        fail("a closed surface was opened")
    if inventory.get("contract_invariants") != EXPECTED_CONTRACT_INVARIANTS:
        fail("contract invariants drift")
    allowed = inventory.get("allowed_changed_paths")
    if not isinstance(allowed, list) or not all(isinstance(path, str) for path in allowed):
        fail("allowed_changed_paths must be a string list")
    if len(allowed) != len(set(allowed)):
        fail("allowed_changed_paths contains duplicates")
    if allowed != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("allowed_changed_paths drift")
    text = PLAN.read_text()
    for marker in (
        "Stage 5E-b", "no-I/O", "first-fresh-live",
        "last_history_bar_close < first_fresh_live_bar_close",
        "observation-only first fresh live bar", "callback count == 0",
        "intent count == 0", "does not call the strategy",
        "does not create an executable intent",
    ):
        if marker not in text:
            fail(f"plan marker missing: {marker}")
    if "last_history_bar_close <= first_fresh_live_bar_close" in text:
        fail("market freshness inequality weakened")
    for contradiction in (
        "callback count == 1",
        "intent count == 1",
        "this slice calls the strategy",
        "the first bar is executable, not observation-only",
    ):
        if contradiction in text:
            fail("plan contradicts machine-readable contract")
    print("stage5e-b-no-io-lifecycle-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
