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
    "crates/broker-core/src/stage4_bootstrap.rs",
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
    "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md",
    "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5d_additive_freeze_check.py",
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
# b3c may live only as a marked, separately pinned nested region.  Removing
# that exact region must reconstruct the original b3b-r2 bytes verbatim.
EXPECTED_REGION_SHA256 = "5615eefda64c694c3a7aa3a35cc22e90b1a9a497f7fa4bc539ebd2d8ea0dba63"
B3C_BRIDGE_BEGIN = "// STAGE5E-B3C-EVIDENCE-BEGIN: private-no-io-v1"
B3C_BRIDGE_END = "// STAGE5E-B3C-EVIDENCE-END: private-no-io-v1"
EXPECTED_B3C_BRIDGE_SHA256 = "e36e98fbcf9e9825a1af549994da7099c804c48a6774a7298795a7f1495a5b0f"


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
    if region.count(B3C_BRIDGE_BEGIN) != 1 or region.count(B3C_BRIDGE_END) != 1:
        fail("b3c bridge region marker cardinality drift")
    bridge_marker_start = region.index(B3C_BRIDGE_BEGIN)
    bridge_start = region.rfind("\n", 0, bridge_marker_start) + 1
    if bridge_start > 0 and region[bridge_start - 1] == "\n":
        bridge_start -= 1
    bridge_end = region.index(B3C_BRIDGE_END) + len(B3C_BRIDGE_END)
    if region[bridge_end:bridge_end + 1] == "\n":
        bridge_end += 1
    bridge = region[bridge_marker_start:bridge_end]
    core_region = region[:bridge_start] + region[bridge_end:]
    if hashlib.sha256(core_region.encode()).hexdigest() != EXPECTED_REGION_SHA256:
        fail("b3 schedule evidence region hash mismatch")
    if hashlib.sha256(bridge.encode()).hexdigest() != EXPECTED_B3C_BRIDGE_SHA256:
        fail("b3c bridge region hash mismatch")
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
        if marker not in core_region:
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
        haystack = core_region.lower() if forbidden in {"redis", "reqwest", "tokio"} else core_region
        if forbidden in haystack:
            fail(f"forbidden b3 no-I/O surface: {forbidden}")
    print("stage5e-b3-schedule-window-evidence-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
