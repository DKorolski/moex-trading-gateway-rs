#!/usr/bin/env python3
"""Static contract gate for the residual Stage 8B-P1-e Foundation F01 fix."""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
LIB = ROOT / "crates/runtime-durable-service/src/lib.rs"
BOUNDARY = ROOT / "docs/stage-8/stage8b-p1e-i1-supervisor-foundation-r2-review-boundary.md"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p1e-i1-foundation-r2-check: FAIL: {message}")


def section(text: str, start: str, end: str) -> str:
    require(start in text, f"missing section start: {start}")
    value = text.split(start, 1)[1]
    require(end in value, f"missing section end: {end}")
    return value.split(end, 1)[0]


def main() -> None:
    source = SOURCE.read_text()
    lib = LIB.read_text()
    boundary = BOUNDARY.read_text()
    coordinate = section(source, "    pub fn coordinate(\n", "    fn authenticated_boundary_decision")
    owner_loss = section(
        coordinate,
        "            Stage8bP1eSupervisorEventV1::OwnerPanicked\n            | Stage8bP1eSupervisorEventV1::OwnerReturnedWithoutOwner => {",
        "            _ => {}",
    )

    for fragment in (
        "pub terminal_failure: Option<Stage8bP1eTerminalFailureV1>",
        "pub enum Stage8bP1eTerminalFailureV1",
        "OwnerLost",
        "RedisLifecycle",
        "Some(Stage8bP1eTerminalFailureV1::OwnerLost)",
        "coordinator_external_signal_then_owner_panic_is_fatal_without_losing_diagnostics",
        "coordinator_external_signal_then_ownerless_return_is_fatal",
        "coordinator_telemetry_failure_then_owner_panic_retains_initiating_diagnostics",
        "coordinator_owner_loss_without_prior_shutdown_records_owner_diagnostics",
        "coordinator_observed_owner_loss_precedes_unprocessed_elapsed_grace",
        "coordinator_preserves_external_signal_and_first_request",
        "coordinator_grace_expiry_keeps_initiating_diagnostics",
        "cleanup_atomically_retains_consumer_that_gains_pending_after_discovery",
    ):
        require(fragment in source, f"source contract missing: {fragment}")
    require("Stage8bP1eTerminalFailureV1" in lib, "terminal failure is not exported")
    require("Some(70)" in owner_loss, "owner loss is not exact exit 70")
    require("Some(Stage8bP1eTerminalFailureV1::OwnerLost)" in owner_loss, "owner loss outcome missing")
    require("false," in owner_loss, "owner loss drain is not disabled")
    require("bounded_exit_class" not in owner_loss, "owner loss inherits initiating exit")
    require("retained_decision" in owner_loss, "owner loss does not retain diagnostics")
    for fragment in (
        "first-wins `Stage8bP1eShutdownIntentV1`",
        "`Stage8bP1eTerminalFailureV1`",
        "always produce exit 70",
        "takes precedence over a grace deadline",
        "process completes with 72",
        "Normal ExternalSignal followed by an authenticated boundary remains exit 0",
        "Redis DB0/VPS activation",
    ):
        require(fragment in boundary, f"boundary contract missing: {fragment}")

    print("stage8b-p1e-i1-foundation-r2-check: ok fatal_owner_loss=70 diagnostics=first-wins")


if __name__ == "__main__":
    main()
