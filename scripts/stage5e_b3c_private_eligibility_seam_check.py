#!/usr/bin/env python3
"""Fail-closed design gate for the Stage 5E-b3c eligibility extension seam."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md"
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
RUNTIME_SOURCE = ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"

B3C_EVIDENCE_BEGIN = "// STAGE5E-B3C-EVIDENCE-BEGIN: private-no-io-v1"
B3C_EVIDENCE_END = "// STAGE5E-B3C-EVIDENCE-END: private-no-io-v1"
EXPECTED_B3C_EVIDENCE_SHA256 = "a298695c1fa1e4a4164402d3625750bdcd908de58724e36ba9bd637c210e5b3c"

EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
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
    "b3b_core_hash_reconstructed_after_b3c_region_removal": True,
    "b3c_is_nested_inside_schedule_window_private_module": True,
    "eligibility_consumes_b3b_receipt_linearly": True,
    "requires_separate_session_calendar_sequence_receipts": True,
    "requires_new_instrument_scoped_session_evidence": True,
    "b2_session_receipt_not_repurposed_as_continuation_authority": True,
    "requires_expiry_revalidation_at_continuation": True,
    "requires_same_full_instrument_id": True,
    "requires_same_event_key_fingerprint": True,
    "requires_same_venue_and_trading_day": True,
    "requires_same_schedule_fingerprint": True,
    "requires_same_continuation_epoch": True,
    "revalidates_session_freshness_at_continuation": True,
    "revalidates_calendar_freshness_at_continuation": True,
    "revalidates_sequence_freshness_at_continuation": True,
    "revalidates_b3b_schedule_expiry_at_continuation": True,
    "blocks_clock_before_any_evidence_observation": True,
    "blocks_future_b3b_observed_bar": True,
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
EXPECTED_SOURCE_AUTHORITIES_SHA256 = "9f631f3989c797d6a12477eea37536082c68b264c2bce9d5b81d7700d3171b8a"
EXPECTED_EVIDENCE_CONTRACTS_SHA256 = "9c72623683ecfb2a0a11a2a5e028176c92da21602b877b22ad241e553c564de6"
EXPECTED_TRANSITION_CONTRACT_SHA256 = "090a227c62c82602b5ad1d8902be34134080a06d795f41c979eb05fb39c356e2"
EXPECTED_BLOCK_REASONS_SHA256 = "a21aa880aa5721c6be7c98766387efedc0e0dba58962f98a7cbf72244dc59581"
EXPECTED_PLAN_SHA256 = "45cbaaf0f1f8f522022666ad02045ad009413ae7f600e74f4d5e60d9da2cb960"


def marked_region(text: str, begin: str, end: str) -> str:
    if text.count(begin) != 1 or text.count(end) != 1:
        fail("b3c evidence region marker drift")
    try:
        return text.split(begin, 1)[1].split(end, 1)[0]
    except IndexError:
        fail("b3c evidence region ordering drift")
    raise AssertionError("unreachable")

def canonical_sha256(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def fail(message: str) -> None:
    print(f"stage5e-b3c-private-eligibility-seam-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if set(payload) != {
        "schema_version", "stage", "status", "baseline_ref", "source_stage5d_aggregate_closure_r2_ref",
        "closed_surfaces", "contract_invariants", "source_authorities", "evidence_contracts",
        "transition_contract", "block_reasons", "implementation_seal", "expected_provenance_case_count", "allowed_changed_paths",
    }:
        fail("inventory key set drift")
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3c-private-eligibility-seam":
        fail("inventory identity drift")
    if payload.get("status") != "private_no_io_conjunctive_eligibility_binding_implemented":
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
    for key, expected in (
        ("source_authorities", EXPECTED_SOURCE_AUTHORITIES_SHA256),
        ("evidence_contracts", EXPECTED_EVIDENCE_CONTRACTS_SHA256),
        ("transition_contract", EXPECTED_TRANSITION_CONTRACT_SHA256),
        ("block_reasons", EXPECTED_BLOCK_REASONS_SHA256),
    ):
        if canonical_sha256(payload.get(key)) != expected:
            fail(f"exact {key} contract drift")
    if payload.get("expected_provenance_case_count") != 136:
        fail("expected provenance case count drift")
    implementation = payload.get("implementation_seal")
    expected_implementation = {
        "source_file": "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
        "module": "schedule_window_evidence::b3c_evidence",
        "region_marker": "private-no-io-v1",
        "region_sha256": EXPECTED_B3C_EVIDENCE_SHA256,
        "b3b_core_region_sha256": "982d7cc67b295ef633ddffa5f767067a7d5c05da1ed5b8b77b31a581d9b7be94",
        "source_input_types": [
            "AcceptedBrokerSessionSnapshotEvidence",
            "AcceptedBrokerCalendarSnapshotEvidence",
            "Stage5cAcceptedMarketSequenceReceipt",
        ],
        "receipt_types": [
            "Stage5eFreshOpenSessionEvidence",
            "Stage5eCalendarEligibilityEvidence",
            "Stage5eMarketSequenceEvidence",
        ],
        "output_type": "Stage5eBoundSessionCalendarSequenceForObservedLiveBar",
        "blocked_transition_type": "Stage5eSessionCalendarSequenceBlocked",
        "acceptance_functions": [
            "accept_open_session", "accept_calendar", "accept_sequence",
            "bind_session_calendar_sequence",
        ],
        "visibility": "nested_module_private",
        "forbidden_region_tokens": [
            "pub(crate)", "pub(super)", "on_broker_bar", "BrokerNeutralHybridIntent",
            "redis", "finam", "reqwest", "tokio", "std::fs", "std::net",
        ],
        "required_transition_markers": [
            "validate_continuation", "ClockBeforeEvidenceObservation",
            "B3bScheduleExpired", "B3bObservedBarInFuture",
            "ScheduleFingerprintMismatch", "BarIdentityMismatch",
            "ContinuationEpochMismatch", "into_inputs",
        ],
    }
    if implementation != expected_implementation:
        fail("implementation seal drift")
    region = marked_region(RUNTIME_SOURCE.read_text(), B3C_EVIDENCE_BEGIN, B3C_EVIDENCE_END)
    if hashlib.sha256(region.encode()).hexdigest() != EXPECTED_B3C_EVIDENCE_SHA256:
        fail("b3c evidence region hash mismatch")
    for token in implementation["forbidden_region_tokens"]:
        if token in region:
            fail(f"forbidden b3c evidence region token: {token}")
    for type_name in implementation["source_input_types"]:
        if f"struct {type_name}" not in region:
            fail(f"b3c source input missing: {type_name}")
    for type_name in implementation["receipt_types"]:
        if f"struct {type_name}" not in region:
            fail(f"b3c receipt missing: {type_name}")
        if f"impl Clone for {type_name}" in region or f"impl Copy for {type_name}" in region:
            fail(f"b3c receipt clone/copy surface: {type_name}")
    for function_name in implementation["acceptance_functions"]:
        if f"fn {function_name}(" not in region:
            fail(f"b3c acceptance function missing: {function_name}")
    for key in ("output_type", "blocked_transition_type"):
        if f"struct {implementation[key]}" not in region:
            fail(f"b3c transition type missing: {implementation[key]}")
    for marker in implementation["required_transition_markers"]:
        if marker not in region:
            fail(f"b3c transition marker missing: {marker}")
    contracts = payload.get("evidence_contracts")
    expected_types = {
        "fresh_open_session": "Stage5eFreshOpenSessionEvidence",
        "calendar": "Stage5eCalendarEligibilityEvidence",
        "market_sequence": "Stage5eMarketSequenceEvidence",
    }
    if not isinstance(contracts, dict) or set(contracts) != set(expected_types):
        fail("evidence contract set drift")
    for name, rust_type in expected_types.items():
        contract = contracts[name]
        if contract.get("rust_type") != rust_type or contract.get("linear") is not True:
            fail("evidence type or linearity drift")
        if any(contract.get(flag) is not False for flag in ("clone", "copy", "serialization")):
            fail("evidence construction seal drift")
        if contract.get("constructors") != ["sealed_checked_transition_only"]:
            fail("evidence constructor authority drift")
        if not isinstance(contract.get("required_fields"), list) or not isinstance(contract.get("fingerprint_fields"), list):
            fail("evidence field schema drift")
    transition = payload.get("transition_contract")
    if not isinstance(transition, dict) or transition.get("output") != "Stage5eBoundSessionCalendarSequenceForObservedLiveBar":
        fail("transition schema drift")
    if transition.get("inputs") != ["Stage5eBoundScheduleWindowForObservedLiveBar", "Stage5eFreshOpenSessionEvidence", "Stage5eCalendarEligibilityEvidence", "Stage5eMarketSequenceEvidence", "continuation_time"]:
        fail("transition input schema drift")
    if set(transition.get("output_authority", {})) != {"callback_ready", "execution_ready", "calls_strategy", "creates_executable_intent"} or any(transition["output_authority"].values()):
        fail("transition authority drift")
    reasons = payload.get("block_reasons")
    if not isinstance(reasons, dict) or set(reasons) != {"producer_rejections", "retryable", "terminal"} or reasons.get("terminal") != [] or len(reasons.get("producer_rejections", [])) != 6 or len(reasons.get("retryable", [])) != 15:
        fail("blocker taxonomy drift")
    if json.loads(ACTIVE.read_text()) != {"schema_version": 1, "stage": "5E-b3c-private-eligibility-seam"}:
        fail("active descriptor drift")
    plan = PLAN.read_text()
    for marker in (
        "Stage 5E-b3c-r3", "Stage5eFreshOpenSessionEvidence",
        "Stage5eCalendarEligibilityEvidence", "Stage5eMarketSequenceEvidence",
        "Stage5eBoundSessionCalendarSequenceForObservedLiveBar", "same full `InstrumentId`",
        "revalidate every evidence expiry", "callback_ready=false", "execution_ready=false",
        "Calendar eligibility must never be inferred", "Market-gap status must come",
        "private-no-io-v1", "byte-for-byte", "nested inside",
        "event-key fingerprint", "continuation epoch",
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
    if hashlib.sha256(PLAN.read_bytes()).hexdigest() != EXPECTED_PLAN_SHA256:
        fail("plan projection hash drift")
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
