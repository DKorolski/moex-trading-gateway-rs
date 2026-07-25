#!/usr/bin/env python3
"""Fail-closed design gate for the Stage 5E-b3c eligibility extension seam."""

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md"
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"

EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]
EXPECTED_CLOSED_SURFACES = {
    "redis", "finam", "transport", "dispatch", "runtime_live",
    "broker_execution", "strategy_callback", "strategy_intent_sink",
    "strategy_state_mutation", "autonomous_event_loop", "executable_intents",
    "calendar_inference", "market_gap_inference",
}
EXPECTED_INVARIANTS = {
    "b3b_core_remains_hash_pinned": True,
    "future_eligibility_consumes_b3b_receipt_linearly": True,
    "requires_separate_session_calendar_sequence_receipts": True,
    "requires_new_instrument_scoped_session_evidence": True,
    "b2_session_receipt_not_repurposed_as_continuation_authority": True,
    "requires_expiry_revalidation_at_continuation": True,
    "requires_same_full_instrument_id": True,
    "requires_same_bar_identity": True,
    "requires_same_schedule_fingerprint": True,
    "revalidates_session_freshness_at_continuation": True,
    "revalidates_calendar_freshness_at_continuation": True,
    "revalidates_sequence_freshness_at_continuation": True,
    "blocks_clock_before_any_evidence_observation": True,
    "blocked_transition_returns_all_inputs": True,
    "successful_transition_is_monotonic": True,
    "calendar_inference_allowed": False,
    "market_gap_inference_allowed": False,
    "requires_explicit_calendar_and_market_sequence_receipts": True,
    "binding_fingerprint_model": "event_key_identity",
    "full_accepted_bar_digest_deferred_to_audit_or_replay_scope": True,
    "callback_ready": False,
    "execution_ready": False,
    "calls_strategy": False,
    "creates_executable_intent": False,
    "callback_count": 0,
    "intent_count": 0,
}


def fail(message: str) -> None:
    print(f"stage5e-b3c-private-eligibility-seam-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if set(payload) != {
        "schema_version", "stage", "status", "baseline_ref",
        "source_stage5d_aggregate_closure_r2_ref", "closed_surfaces",
        "contract_invariants", "allowed_changed_paths",
    }:
        fail("inventory key set drift")
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3c-private-eligibility-seam":
        fail("inventory identity drift")
    if payload.get("status") != "exact_contract_frozen_no_runtime_code":
        fail("inventory status drift")
    if payload.get("baseline_ref") != "95861577ce3acc11963104bb5a313a82f6f82bdb":
        fail("baseline drift")
    if payload.get("source_stage5d_aggregate_closure_r2_ref") != "9ebbfd29d0346be5149dac746225866f0c8d0257":
        fail("lineage root drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("allowed changed paths drift")
    if set(payload.get("closed_surfaces", {})) != EXPECTED_CLOSED_SURFACES or any(payload["closed_surfaces"].values()):
        fail("closed surface drift")
    if payload.get("contract_invariants") != EXPECTED_INVARIANTS:
        fail("contract invariant drift")
    if json.loads(ACTIVE.read_text()) != {"schema_version": 1, "stage": "5E-b3c-private-eligibility-seam"}:
        fail("active descriptor drift")
    plan = PLAN.read_text()
    for marker in (
        "Stage 5E-b3c-r1", "Stage5eFreshOpenSessionEvidence",
        "Stage5eCalendarEligibilityEvidence", "Stage5eMarketSequenceEvidence",
        "Stage5eBoundSessionCalendarSequenceForObservedLiveBar", "same full `InstrumentId`",
        "revalidate every evidence expiry", "callback_ready=false", "execution_ready=false",
        "Calendar eligibility must never be inferred", "Market-gap status must come",
    ):
        if marker not in plan:
            fail(f"plan marker missing: {marker}")
    for contradiction in (
        "b2 session receipt may be repurposed",
        "calendar may be inferred from timestamps",
        "callback_count becomes 1",
    ):
        if contradiction in plan:
            fail(f"forbidden plan contradiction: {contradiction}")
    predecessor = subprocess.run(
        [sys.executable, "scripts/stage5e_b3_schedule_window_evidence_check.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if predecessor.returncode != 0:
        fail("b3b predecessor freeze failed")
    print("stage5e-b3c-private-eligibility-seam-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
