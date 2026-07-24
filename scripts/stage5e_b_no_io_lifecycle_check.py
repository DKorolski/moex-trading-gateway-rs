#!/usr/bin/env python3
"""Pin the Stage 5E-b foundation to an explicit no-I/O scope."""

from __future__ import annotations

import json
import hashlib
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md"
INVENTORY = ROOT / "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json"
FREEZE_REF = "eb03695dc407b02bb8327de57fde6acea077d96b"
BASELINE_REF = "0ffeb6aefe790efeaa6d99157104bd5aef8ff35e"
EXPECTED_TOP_LEVEL_KEYS = {
    "allowed_changed_paths", "baseline_ref", "closed_surfaces", "schema_version",
    "contract_invariants", "source_stage5d_aggregate_closure_r2_ref", "stage",
    "stage5e_a_freeze_ref", "status",
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/make_handoff_archive.sh",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
]
CLOSED = {
    "redis", "finam", "transport", "dispatch", "runtime_live",
    "broker_execution", "strategy_intent_sink", "autonomous_event_loop",
}
EXPECTED_CONTRACT_INVARIANTS = {
    "market_freshness_relation": "strict_lt",
    "first_live_bar_mode": "observed_after_history_only",
    "callback_count": 0,
    "intent_count": 0,
    "calls_strategy": False,
    "creates_executable_intent": False,
}
BRIDGE_BEGIN = "// STAGE5E-NO-IO-BRIDGE-BEGIN: contextual-observation-v1"
BRIDGE_END = "// STAGE5E-NO-IO-BRIDGE-END: contextual-observation-v1"
EXPECTED_BRIDGE_SHA256 = "220f5ff64eb93b30c0de67872a9e8204f469933536d1c474a2ea11e26306701e"
VALIDATOR_BEGIN = "// STAGE5E-NO-IO-VALIDATOR-BEGIN: contextual-admission-v1"
VALIDATOR_END = "// STAGE5E-NO-IO-VALIDATOR-END: contextual-admission-v1"
EXPECTED_VALIDATOR_SHA256 = "8ebad6268be99e5c7995668ee08290cdd058ede6f38d424476d5df0897f39f4c"


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
    if inventory.get("status") != "contextual_no_io_type_state":
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
    if "last_history_bar_close <= observed_live_bar_close" in text:
        fail("market freshness inequality weakened")
    for marker in (
        "Stage 5E-b", "no-I/O", "observed-live-bar-after-history",
        "last_history_bar_close < observed_live_bar_close",
        "does not prove a market-data gap", "callback count == 0",
        "intent count == 0", "does not call the strategy",
        "does not create an executable intent",
    ):
        if marker not in text:
            fail(f"plan marker missing: {marker}")
    for contradiction in (
        "callback count == 1",
        "intent count == 1",
        "this slice calls the strategy",
        "the first bar is executable, not observation-only",
    ):
        if contradiction in text:
            fail("plan contradicts machine-readable contract")
    runtime_root = ROOT / "crates/strategy-runtime-core/src"
    module = runtime_root / "stage5e_no_io_lifecycle.rs"
    lib = runtime_root / "lib.rs"
    host = runtime_root / "stage5c_paper_host.rs"
    handoff_builder = ROOT / "scripts/make_handoff_archive.sh"
    if not module.is_file() or not lib.is_file() or not host.is_file():
        fail("missing Stage 5E-b1 private runtime boundary")
    module_text = module.read_text()
    if module_text.count(VALIDATOR_BEGIN) != 1 or module_text.count(VALIDATOR_END) != 1:
        fail("Stage 5E-b1 validator region markers must occur exactly once")
    validator = module_text.split(VALIDATOR_BEGIN, 1)[1].split(VALIDATOR_END, 1)[0]
    for condition in (
        "if bar_instrument != target_instrument {",
        "if bar_tick_size.to_bits() != admission_tick_size.to_bits() {",
        "if lifecycle_now > admission_expires_at {",
        "if bar_close > lifecycle_now.timestamp() {",
    ):
        if condition not in validator:
            fail(f"Stage 5E-b1 contextual condition missing: {condition}")
    for marker in (
        "Stage5eObservedLiveBarAfterHistory",
        "HybridRuntimeBarOrigin::Live",
        "validate_contextual_live_bar_after_history",
        "bar_close <= last_history_bar_close",
        "InstrumentMismatch",
        "TickSizeMismatch",
        "AdmissionExpired",
        "FutureBar",
        "callback_count",
        "intent_count",
    ):
        if marker not in module_text:
            fail(f"missing Stage 5E-b1 marker: {marker}")
    for forbidden in (
        "on_broker_bar",
        "BrokerNeutralHybridIntent",
        "redis",
        "finam",
        "reqwest",
        "tokio",
    ):
        haystack = module_text.lower() if forbidden in {"redis", "finam", "reqwest", "tokio"} else module_text
        if forbidden in haystack:
            fail(f"forbidden Stage 5E-b1 surface: {forbidden}")
    if "FirstFresh" in module_text or "first fresh" in module_text.lower():
        fail("Stage 5E-b1 must not claim first-fresh or market-gap proof")
    if "pub use stage5e_no_io_lifecycle" in lib.read_text():
        fail("Stage 5E-b1 private module leaked into public API")
    host_text = host.read_text()
    if host_text.count(BRIDGE_BEGIN) != 1 or host_text.count(BRIDGE_END) != 1:
        fail("Stage 5E-b1 bridge region markers must occur exactly once")
    bridge = host_text.split(BRIDGE_BEGIN, 1)[1].split(BRIDGE_END, 1)[0]
    for forbidden in (
        "on_broker_bar", "BrokerNeutralHybridIntent", "intent sink", "dispatch",
        "redis", "FinamRestClient", "reqwest", "tokio", "std::fs", "std::net",
    ):
        haystack = bridge.lower() if forbidden in {"redis", "reqwest", "tokio"} else bridge
        if forbidden in haystack:
            fail(f"forbidden Stage 5E-b1 bridge surface: {forbidden}")
    if hashlib.sha256(bridge.encode()).hexdigest() != EXPECTED_BRIDGE_SHA256:
        fail("Stage 5E-b1 bridge region hash mismatch")
    if "Stage5eNoIoBridgeSeal" not in host_text:
        fail("missing Stage 5E-b1 single-construction seal")
    if "stage5e_try_observe_live_bar_after_history" not in host_text:
        fail("missing Stage 5E-b1 retryable consuming bridge")
    if "into_retry" not in host_text:
        fail("missing Stage 5E-b1 blocked-state retry return")
    if "#[cfg(test)]\npub(crate) fn stage5e_try_observe_live_bar_after_history_at" not in host_text:
        fail("Stage 5E-b1 deterministic clock seam must be test-only")
    if hashlib.sha256(validator.encode()).hexdigest() != EXPECTED_VALIDATOR_SHA256:
        fail("Stage 5E-b1 validator region hash mismatch")
    if "(\n  set -euo pipefail\n  cd \"$repo_root\"\n  cargo fmt --check" not in handoff_builder.read_text():
        fail("Stage 5E-b1 cargo runner must fail closed per command")
    print("stage5e-b-no-io-lifecycle-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
