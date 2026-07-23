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
CLOSED = {
    "redis", "finam", "transport", "dispatch", "runtime_live",
    "broker_execution", "strategy_intent_sink", "autonomous_event_loop",
}


def fail(message: str) -> None:
    print(f"stage5e-b-no-io-lifecycle-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    if not PLAN.is_file() or not INVENTORY.is_file():
        fail("missing Stage 5E-b plan or inventory")
    inventory = json.loads(INVENTORY.read_text())
    if inventory.get("schema_version") != 1:
        fail("schema_version must be 1")
    if inventory.get("stage") != "5E-b-no-io-lifecycle-capability":
        fail("unexpected stage")
    if inventory.get("status") != "implementation_foundation":
        fail("unexpected status")
    if inventory.get("baseline_ref") != "40ec10372013a616d793623307293d5419f3a6d2":
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
    text = PLAN.read_text()
    for marker in ("Stage 5E-b", "no-I/O", "first-fresh-live", "not call the strategy"):
        if marker not in text:
            fail(f"plan marker missing: {marker}")
    print("stage5e-b-no-io-lifecycle-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
