#!/usr/bin/env python3
"""Additive-successor guard for accepted Stage8A1 R3 authority semantics."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def require(document: str, markers: tuple[str, ...], label: str) -> None:
    for marker in markers:
        if marker not in document:
            raise SystemExit(f"stage8a1-successor-check: FAIL {label}: {marker}")


def main() -> None:
    capability = (ROOT / "crates/finam-gateway/src/stage8a1_execution_capability.rs").read_text()
    runtime = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    boundary = (ROOT / "crates/finam-gateway/tests/stage8a1_r3_authority_boundary.rs").read_text()
    require(capability, (
        "pub fn from_stage7b_owner",
        "authorize_stage8a1_durable_request",
        "Stage7bCompositeReadinessSnapshot",
        "revalidate_place_capability",
        "revalidate_cancel_capability",
        "read_arm_registration",
        "issue_stage8a4_post_effect_control_evidence",
    ), "capability")
    require(runtime, (
        "revalidate_cached_committed_seal",
        "commitment_key.stage7b_verify_recovery_seal_hmac_sha256",
        "refresh_stage7b_durable_frontier",
        "Stage7bPaperReadinessPhase::PaperReady",
    ), "current seal/readiness")
    require(boundary, (
        "owner_mediated_constructor_boundary",
        "trusted_issuer_is_the_public_no_send_authority_boundary",
    ), "external boundary")
    for forbidden in (
        "from_current_stage6_authority",
        "Stage8a1CompositeReadinessSnapshot",
    ):
        if forbidden in capability:
            raise SystemExit(f"stage8a1-successor-check: FAIL forbidden={forbidden}")
    print("stage8a1-successor-check: PASS")


if __name__ == "__main__":
    main()
