#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3d callback-authority design."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3d-callback-authority-design.md"
INVENTORY = (
    ROOT / "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
BASELINE_REF = "ff1344f170b8457df91a6038d670087eef3cc1dc"
STAGE = "5E-b3d-callback-authority-design"

EXPECTED_INVENTORY_SHA256 = (
    "730b208b4a90de62557d1713726a32c01ec2cd2196fd25ef0bc5c98b583ec3f8"
)
EXPECTED_PLAN_SHA256 = (
    "e37ca8fe9377690935f4513c2e339bde0f6fa25c6060a376f98ac71df98a7c42"
)
EXPECTED_PREDECESSOR_CHECKER_SHA256 = (
    "cd1453de67401d28ce9c320fc76a2fe4051c57dd41fd9c306fe1bf7e12ece2f5"
)
EXPECTED_PRIVATE_PREDECESSOR_CHECKER_SHA256 = (
    "621e0a78aa40db732698d63b405b5eea4b8a2ff9a836de01cc66ea2c363ba955"
)
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3d-callback-authority-design.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5e_b3d_callback_authority_design_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]
EXPECTED_PROTECTED_SOURCE_SHA256 = {
    "crates/broker-core/src/stage4_bootstrap.rs": (
        "33455bd4447193f723aa5a749707739d89e2d2ca58b083d416c268a24613bdd7"
    ),
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": (
        "7f5e3ad070c1bbc3ddca1e642d59b3f4cf75b9bb0d1651068df363323f1cd427"
    ),
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "7457a1b9a2318d84b48dc5dda168782547eeb8e6c5a5bbd3640bb3804b7a8bb8"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "ea75d47c0852a7e031787eeb9af77b73cfc628b1f3da37f8962e839677179671"
    ),
    "Cargo.toml": "1c3e7dd1b83a6a8942e02cb520d49f33ed3ef77f2970854b9fdcddc7f261bc3e",
    "Cargo.lock": "ff535d0490a848e43631906ee8abd8633630d162714299f7628c0e5fe8a0b36b",
}


def fail(message: str) -> None:
    print(
        f"stage5e-b3d-callback-authority-design-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def run_predecessor() -> None:
    private_checker = (
        ROOT / "scripts/stage5e_b3c_private_eligibility_seam_check.py"
    )
    if sha256(private_checker) != EXPECTED_PRIVATE_PREDECESSOR_CHECKER_SHA256:
        fail("accepted private predecessor checker drift")
    checker = ROOT / "scripts/stage5e_b3c_source_authority_freeze_extension_check.py"
    if sha256(checker) != EXPECTED_PREDECESSOR_CHECKER_SHA256:
        fail("accepted predecessor checker drift")
    result = subprocess.run(
        [sys.executable, str(checker)],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail("accepted predecessor implementation gate failed")


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
        active = json.loads(ACTIVE.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid design governance: {exc}")

    if canonical_sha256(inventory) != EXPECTED_INVENTORY_SHA256:
        fail("design inventory drift")
    if sha256(PLAN) != EXPECTED_PLAN_SHA256:
        fail("design plan drift")
    if active != {"schema_version": 1, "stage": STAGE}:
        fail("active descriptor drift")
    if inventory.get("schema_version") != 1 or inventory.get("stage") != STAGE:
        fail("design identity drift")
    if inventory.get("status") != "design_only_pending_review":
        fail("design status drift")
    if inventory.get("baseline_ref") != BASELINE_REF:
        fail("design baseline drift")
    if inventory.get("predecessor_ref") != BASELINE_REF:
        fail("accepted predecessor reference drift")
    if inventory.get("expected_provenance_case_count") != 214:
        fail("design negative-matrix count drift")
    if inventory.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("design changed-path contract drift")
    if inventory.get("protected_source_sha256") != EXPECTED_PROTECTED_SOURCE_SHA256:
        fail("protected source contract drift")

    for rel, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
        if sha256(ROOT / rel) != expected:
            fail(f"design-only stage changed protected source: {rel}")

    if (ROOT / ".git").exists():
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", BASELINE_REF, "--"],
                cwd=ROOT,
                text=True,
            ).splitlines()
        )
        changed.update(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=ROOT,
                text=True,
            ).splitlines()
        )
        if sorted(changed) != sorted(EXPECTED_ALLOWED_CHANGED_PATHS):
            fail("design review diff drift")

    expected_output = {
        "type": "Stage5eCallbackAuthorityReadyPaperStrategy",
        "callback_ready": True,
        "callback_invoked": False,
        "execution_ready": False,
        "calls_strategy": False,
        "mutates_strategy": False,
        "creates_executable_intent": False,
        "intent_count": 0,
        "successful_unbinding_allowed": False,
    }
    if inventory.get("future_output") != expected_output:
        fail("callback authority vector drift")

    transition = inventory.get("future_transition")
    if not isinstance(transition, dict):
        fail("future transition contract missing")
    if transition.get("visibility") != "crate_private":
        fail("future transition visibility widened")
    if transition.get("production_clock") != "captured_inside_transition":
        fail("production clock ownership drift")
    if transition.get("caller_supplied_production_clock_allowed") is not False:
        fail("caller-supplied production clock opened")
    if transition.get("test_clock_seam") != "cfg_test_only":
        fail("test clock seam escaped production boundary")
    if (
        transition.get("consume_order")
        != "borrowed_non_decomposable_preflight_then_single_linear_consume"
    ):
        fail("linear consume order drift")

    block = inventory.get("block_contract")
    if not isinstance(block, dict):
        fail("block type-state contract missing")
    if block.get("retryable_type") != "Stage5eCallbackAuthorityRetryableBlock":
        fail("retryable blocker type drift")
    if block.get("retryable_conversion") != "into_retry_same_receipt":
        fail("retry conversion drift")
    if block.get("refresh_type") != "Stage5eCallbackAuthorityRefreshEvidenceBlock":
        fail("refresh blocker type drift")
    if block.get("refresh_conversion") != "into_refresh_input":
        fail("refresh conversion drift")
    if block.get("terminal_type") != "Stage5eCallbackAuthorityTerminalBlock":
        fail("terminal blocker type drift")
    if block.get("terminal_retry_or_refresh_conversion_allowed") is not False:
        fail("terminal retry or refresh conversion opened")
    if block.get("autonomous_retry_authorized") is not False:
        fail("autonomous retry opened")

    closed = inventory.get("closed_surfaces")
    if (
        not isinstance(closed, dict)
        or set(closed)
        != {
            "strategy_callback",
            "strategy_state_mutation",
            "executable_intents",
            "strategy_intent_sink",
            "redis",
            "finam_io",
            "transport",
            "dispatch",
            "runtime_live",
            "broker_execution",
            "autonomous_event_loop",
            "schedule_provider_attachment",
            "venue_calendar_inference",
        }
        or any(value is not False for value in closed.values())
    ):
        fail("closed surface opened")

    providers = inventory.get("deferred_provider_gates")
    if not isinstance(providers, dict) or any(
        value is not False for value in providers.values()
    ):
        fail("provider or venue-calendar gate opened")

    plan = PLAN.read_text()
    for marker in (
        "design-only, pending review",
        "Stage5eBoundSessionCalendarSequenceForObservedLiveBar",
        "Stage5eCallbackAuthorityReadyPaperStrategy",
        "callback_ready = true",
        "callback_invoked = false",
        "calls_strategy = false",
        "Stage5eCallbackAuthorityRetryableBlock",
        "Stage5eCallbackAuthorityRefreshEvidenceBlock",
        "Stage5eCallbackAuthorityTerminalBlock",
        "No autonomous retry loop is authorized",
        "on_broker_bar",
        "Actual callback invocation requires a separate implementation review",
    ):
        if marker not in plan:
            fail(f"required design marker missing: {marker}")

    protected_source = "\n".join(
        (ROOT / rel).read_text(errors="replace")
        for rel in EXPECTED_PROTECTED_SOURCE_SHA256
        if rel.endswith(".rs")
    )
    if "Stage5eCallbackAuthorityReadyPaperStrategy" in protected_source:
        fail("design-only callback authority type entered production source")

    run_predecessor()
    print("stage5e-b3d-callback-authority-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
