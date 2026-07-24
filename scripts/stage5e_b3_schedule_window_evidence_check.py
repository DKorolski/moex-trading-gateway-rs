#!/usr/bin/env python3
"""Fail-closed governance gate for Stage 5E-b3a."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3-schedule-window-evidence-plan.md"

EXPECTED_BASELINE_REF = "04431096e269daaf9715e253b2354b1ac8fcc3e8"
EXPECTED_LINEAGE_ROOT_REF = "9ebbfd29d0346be5149dac746225866f0c8d0257"
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3-schedule-window-evidence-plan.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
    "scripts/stage5e_descriptor.py",
]

def fail(message: str) -> None:
    print(f"stage5e-b3-schedule-window-evidence-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)

def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3-schedule-window-evidence":
        fail("inventory identity drift")
    if payload.get("baseline_ref") != EXPECTED_BASELINE_REF:
        fail("baseline drift")
    if payload.get("source_stage5d_aggregate_closure_r2_ref") != EXPECTED_LINEAGE_ROOT_REF:
        fail("lineage root drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("allowed changed paths drift")
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
