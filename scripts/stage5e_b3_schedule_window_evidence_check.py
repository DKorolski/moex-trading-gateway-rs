#!/usr/bin/env python3
"""Fail-closed governance gate for Stage 5E-b3a."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3-schedule-window-evidence-plan.md"

def fail(message: str) -> None:
    print(f"stage5e-b3-schedule-window-evidence-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)

def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3-schedule-window-evidence":
        fail("inventory identity drift")
    if payload.get("baseline_ref") != "04431096e269daaf9715e253b2354b1ac8fcc3e8":
        fail("baseline drift")
    if any(value is not False for value in payload.get("closed_surfaces", {}).values()):
        fail("closed surface opened")
    expected = {"session_window_bounds": "inclusive_closed", "callback_count": 0, "intent_count": 0, "requires_stage4_schedule_evidence": True, "requires_trusted_schedule_definition": True}
    if payload.get("contract_invariants") != expected:
        fail("contract invariant drift")
    text = PLAN.read_text()
    for marker in ("Stage 5E-b3a", "accepted Stage 4 schedule evidence", "sealed trusted schedule definition", "inclusive", "b3b"):
        if marker not in text:
            fail(f"plan marker missing: {marker}")
    print("stage5e-b3-schedule-window-evidence-check: ok")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
