#!/usr/bin/env python3
"""Validate Stage 5E-a lifecycle/event-time attachment inventory.

Stage 5E-a is intentionally design/inventory-only. This checker makes the
first Stage 5E boundary explicit while preserving all closed execution
surfaces.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"
INVENTORY = ROOT / "docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
EXPECTED_STAGE5D_REF = "9ebbfd29d0346be5149dac746225866f0c8d0257"
EXPECTED_CHAIN = [
    "validated_broker_truth",
    "runtime_state_restore",
    "bootstrap_notification",
    "restored_state_notification",
    "canonical_history_warmup",
    "pending_stream_recovery",
    "first_eligible_strategy_callback",
]
EXPECTED_CLOSED_SURFACES = {
    "redis",
    "finam",
    "transport",
    "dispatch",
    "runtime_live",
    "broker_execution",
    "strategy_intent_sink",
    "autonomous_event_loop",
}
EXPECTED_CHECKS = {
    "callback_after_validated_truth_and_stage5d_restore",
    "canonical_final_m10_only",
    "monotonic_event_time_watermarks",
    "warmup_sufficiency_source_compatible",
    "reconnect_gap_proof_before_first_fresh_bar",
    "session_day_rollover_and_weekend_policy",
    "blocked_report_zero_callbacks",
    "restart_replay_determinism",
    "exact_numeric_and_semantic_adr_entry_enforcement",
}
REQUIRED_DOC_MARKERS = [
    "Stage 5E-a lifecycle/event-time attachment plan",
    "design/inventory-only",
    EXPECTED_STAGE5D_REF,
    "validated broker truth",
    "runtime state restore",
    "bootstrap notification",
    "restored-state notification",
    "canonical history warmup",
    "pending stream recovery",
    "first eligible strategy callback",
    "canonical final M10",
    "first fresh semantic bar",
    "Stage 5E-a keeps these surfaces closed",
]
REQUIRED_BINDING_DOCS = {
    "docs/stage-5/5d-final-restart-r3-aggregate-closure-r2-review-summary.md",
    "docs/adr/adr-stage5d-exact-numeric-persistence.md",
    "docs/adr/adr-stage5d-semantic-compatibility-policy.md",
    "docs/stage-5-real-strategy-semantics-plan.md",
    "docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md",
}


def fail(message: str) -> None:
    print(f"stage5e-lifecycle-event-time-freeze-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"missing file: {path.relative_to(ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"top-level JSON is not object: {path.relative_to(ROOT)}")
    return value


def require_file(path: Path) -> None:
    if not path.is_file():
        fail(f"missing file: {path.relative_to(ROOT)}")


def main() -> int:
    require_file(DOC)
    inventory = load_json(INVENTORY)
    doc_text = DOC.read_text()

    for marker in REQUIRED_DOC_MARKERS:
        if marker not in doc_text:
            fail(f"document marker missing: {marker}")

    if inventory.get("schema_version") != 1:
        fail("schema_version must be 1")
    if inventory.get("stage") != "5E-a-lifecycle-event-time-attachment-plan":
        fail("unexpected stage")
    if inventory.get("status") != "review_candidate_design_inventory_only":
        fail("unexpected status")
    if inventory.get("source_stage5d_aggregate_closure_r2_ref") != EXPECTED_STAGE5D_REF:
        fail("Stage 5D aggregate closure r2 source ref mismatch")

    if inventory.get("lifecycle_chain") != EXPECTED_CHAIN:
        fail("lifecycle chain drift")

    closed = inventory.get("closed_surfaces")
    if not isinstance(closed, dict):
        fail("closed_surfaces must be an object")
    if set(closed) != EXPECTED_CLOSED_SURFACES:
        fail("closed_surfaces key set drift")
    opened = [name for name, value in closed.items() if value is not False]
    if opened:
        fail(f"closed surfaces opened: {opened}")

    claims = inventory.get("stage5e_a_claims")
    if not isinstance(claims, dict):
        fail("stage5e_a_claims must be an object")
    if claims.get("design_inventory_only") is not True:
        fail("Stage 5E-a must remain design/inventory-only")
    false_claims = [
        "callback_implementation_added",
        "redis_opened",
        "finam_opened",
        "transport_opened",
        "dispatch_opened",
        "runtime_live_opened",
        "broker_execution_opened",
    ]
    for key in false_claims:
        if claims.get(key) is not False:
            fail(f"claim must be false: {key}")

    binding_docs = set(inventory.get("binding_documents", []))
    if binding_docs != REQUIRED_BINDING_DOCS:
        fail("binding document set drift")
    for rel_path in sorted(binding_docs):
        require_file(ROOT / rel_path)

    checks = inventory.get("required_future_executable_checks")
    if not isinstance(checks, list):
        fail("required_future_executable_checks must be a list")
    observed = set()
    for row in checks:
        if not isinstance(row, dict):
            fail("required check row must be an object")
        observed.add(row.get("id"))
        if row.get("status") != "planned_no_io_check":
            fail(f"required check has unexpected status: {row.get('id')}")
    if observed != EXPECTED_CHECKS:
        fail("required future executable check set drift")

    print("stage5e-lifecycle-event-time-freeze-check: ok")
    print(f"stage5e_a_source_ref={EXPECTED_STAGE5D_REF}")
    print("closed_surfaces=redis,finam,transport,dispatch,runtime_live,broker_execution,strategy_intent_sink,autonomous_event_loop")
    print("lifecycle_chain=validated_broker_truth->runtime_state_restore->bootstrap_notification->restored_state_notification->canonical_history_warmup->pending_stream_recovery->first_eligible_strategy_callback")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
