#!/usr/bin/env python3
"""Fail-closed semantic gate for the Stage 5E-b3b-r2 no-I/O binding contract."""

import hashlib
import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3-schedule-window-evidence-plan.md"
MODULE = ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"

EXPECTED_BASELINE_REF = "04431096e269daaf9715e253b2354b1ac8fcc3e8"
EXPECTED_LINEAGE_ROOT_REF = "9ebbfd29d0346be5149dac746225866f0c8d0257"
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3-schedule-window-evidence-plan.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]
EXPECTED_TOP_LEVEL_KEYS = {
    "allowed_changed_paths",
    "baseline_ref",
    "closed_surfaces",
    "contract_invariants",
    "schema_version",
    "source_stage5d_aggregate_closure_r2_ref",
    "stage",
    "status",
}
EXPECTED_CLOSED_SURFACES = {
    "redis",
    "finam",
    "transport",
    "dispatch",
    "runtime_live",
    "broker_execution",
    "strategy_callback",
    "strategy_intent_sink",
    "strategy_state_mutation",
    "autonomous_event_loop",
}
EXPECTED_CONTRACT_INVARIANTS = {
    "session_window_bounds": "inclusive_closed",
    "callback_count": 0,
    "intent_count": 0,
    "requires_stage4_schedule_evidence": True,
    "requires_validated_normalized_snapshot": True,
    "requires_accepted_registry_identity": True,
    "revalidates_expiry_at_mapping": True,
    "stage4_normalized_relation": "conjunctive_independent_evidence",
    "rejects_shared_inclusive_endpoint": True,
    "requires_stage4_report_not_future": True,
    "trusted_b3b_observed_bar_binding": True,
    "requires_exact_observed_bar_instrument": True,
    "revalidates_window_expiry_at_binding": True,
    "returns_linear_inputs_on_binding_block": True,
    "rejects_future_observed_bar_at_binding": True,
    "captures_production_binding_clock": True,
    "rejects_clock_before_effective_evidence_observation": True,
    "successful_binding_is_monotonic": True,
    "binding_fingerprint_present": True,
    "actual_consuming_binding_tested": True,
    "requires_mandatory_stage5c_ownership": True,
    "actual_stage5c_ownership_retention_tested": True,
}
REGION_BEGIN = "// STAGE5E-B3-SCHEDULE-WINDOW-BEGIN: sealed-contract-v5"
REGION_END = "// STAGE5E-B3-SCHEDULE-WINDOW-END: sealed-contract-v5"
EXPECTED_REGION_SHA256 = "982d7cc67b295ef633ddffa5f767067a7d5c05da1ed5b8b77b31a581d9b7be94"


def fail(message: str) -> None:
    print(f"stage5e-b3-schedule-window-evidence-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3-schedule-window-evidence":
        fail("inventory identity drift")
    if payload.get("status") != "mandatory_stage5c_ownership_bound_no_io":
        fail("inventory status drift")
    if set(payload) != EXPECTED_TOP_LEVEL_KEYS:
        fail("inventory key set drift")
    if payload.get("baseline_ref") != EXPECTED_BASELINE_REF:
        fail("baseline drift")
    if payload.get("source_stage5d_aggregate_closure_r2_ref") != EXPECTED_LINEAGE_ROOT_REF:
        fail("lineage root drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("allowed changed paths drift")
    closed = payload.get("closed_surfaces")
    if not isinstance(closed, dict) or set(closed) != EXPECTED_CLOSED_SURFACES:
        fail("closed surface key set drift")
    if any(value is not False for value in closed.values()):
        fail("closed surface opened")
    if payload.get("contract_invariants") != EXPECTED_CONTRACT_INVARIANTS:
        fail("contract invariant drift")
    text = PLAN.read_text()
    for marker in (
        "Stage 5E-b3b-r2",
        "validated opaque snapshot",
        "accepted Stage 4 schedule evidence",
        "inclusive",
        "conjunctive independent evidence",
        "ticker@mic",
        "Stage5eBoundScheduleWindowForObservedLiveBar",
        "Stage5eObservedLiveBarAfterHistory",
        "linear inputs",
        "monotonic",
        "Utc::now()",
        "b3b",
        "mandatory Stage 5C ownership",
        "Stage5eNoIoBridgeSeal",
        "Stage 5C admission → bootstrap → restore → history warmup",
        "strategy-state fingerprint",
    ):
        if marker not in text:
            fail(f"plan marker missing: {marker}")
    module = MODULE.read_text()
    if module.count(REGION_BEGIN) != 1 or module.count(REGION_END) != 1:
        fail("b3 region marker cardinality drift")
    region = module.split(REGION_BEGIN, 1)[1].split(REGION_END, 1)[0]
    if hashlib.sha256(region.encode()).hexdigest() != EXPECTED_REGION_SHA256:
        fail("b3 schedule evidence region hash mismatch")
    for marker in (
        "ValidatedNormalizedInstrumentScheduleSnapshot",
        "SealedInstrumentRegistryBridgeInput",
        "AcceptedInstrumentRegistryEvidence",
        "accept_instrument_registry_evidence",
        "project_accepted_stage4_schedule",
        "validate_stage4_projection_times",
        "map_trusted_schedule_window",
        "split_canonical_broker_symbol",
        "normalized_snapshot_payload_fingerprint",
        "session.start.0 <= end",
        "ReportCheckedInFuture",
        "lifecycle_now.0 > stage4.expires_at.0",
        "lifecycle_now.0 > validated.snapshot.source_expires_at.0",
        "NoTradableOpenForRequestedBar",
        "ScheduleWindowObservedBarBindingError",
        "bind_schedule_window_to_observed_live_bar",
        "validate_schedule_window_for_observed_bar",
        "Stage5eBoundScheduleWindowForObservedLiveBar",
        "Stage5eScheduleWindowObservedBarBlocked",
        "ObservedBarInFuture",
        "WindowExpired",
        "ClockBeforeEffectiveEvidenceObservation",
        "bind_schedule_window_to_observed_live_bar_with_now",
        "bind_schedule_window_to_observed_live_bar_at",
        "ScheduleObservedBarBindingFingerprint",
        "schedule_observed_bar_binding_fingerprint",
        "ownership_fingerprint",
        "stage5e_test_observed_live_bar_after_history_at",
    ):
        if marker not in region:
            fail(f"b3 semantic marker missing: {marker}")
    for forbidden in (
        "on_broker_bar",
        "BrokerNeutralHybridIntent",
        "intent sink",
        "redis",
        "reqwest",
        "tokio",
        "std::fs",
        "std::net",
    ):
        haystack = region.lower() if forbidden in {"redis", "reqwest", "tokio"} else region
        if forbidden in haystack:
            fail(f"forbidden b3 no-I/O surface: {forbidden}")
    print("stage5e-b3-schedule-window-evidence-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
