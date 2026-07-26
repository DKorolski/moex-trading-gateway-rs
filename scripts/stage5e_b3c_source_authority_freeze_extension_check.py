#!/usr/bin/env python3
"""Fail-closed gate for the reviewed Stage 5E-b3c R6 production bridge."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md"
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
BASELINE_REF = "2b2c57d7bacb8e3f1de572b7c35790be906b82a9"

EXPECTED_INVENTORY_SHA256 = "d7e7e696bbb8eebb786a16c0191406ea1bc85b9468aad7ec7e3e5c3e0aaa48c0"
EXPECTED_PLAN_SHA256 = "2d41f04901c6b72b9031d63eb3219d80100bcb758a701bf77ff495175b67b15f"
EXPECTED_SOURCE_BASELINES = {
    "crates/broker-core/src/lib.rs": "5d8758624f53a6b46d8903dd3f2339d5bd04f64c9c6490448167f08ac68ec8a2",
    "crates/broker-core/src/operational_config.rs": "492905c6e404ee67f62ad456128ff659cd4a32c8e638936b94b5ea14ff3ba2f8",
    "crates/broker-core/src/stage4_bootstrap.rs": "33455bd4447193f723aa5a749707739d89e2d2ca58b083d416c268a24613bdd7",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "41f6f9da0e0beedf4e292c852b6dafb6fd00bb2215368c7b366a78000170e399",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": "cd49213ba507f924f390e839013b13222bc78a6ab9b75bcd355bd5ba8f766d9f",
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/broker-core/src/stage4_bootstrap.rs",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md",
    "docs/stage-5/5e-b3-schedule-window-evidence-plan.md",
    "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md",
    "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json",
    "docs/stage-5/stage5e-b3-schedule-window-evidence-inventory.json",
    "docs/stage-5/stage5e-b3c-private-eligibility-seam-inventory.json",
    "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5d_additive_freeze_check.py",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
    "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
]

STAGE5C_BEGIN = "// STAGE5E-B3C-R6-SEALS-BEGIN: additive-no-io-v1"
STAGE5C_END = "// STAGE5E-B3C-R6-SEALS-END: additive-no-io-v1"
STAGE5E_BEGIN = "// STAGE5E-B3C-PRODUCTION-BRIDGE-BEGIN: trusted-no-io-v1"
STAGE5E_END = "// STAGE5E-B3C-PRODUCTION-BRIDGE-END: trusted-no-io-v1"


def fail(message: str) -> None:
    print(
        f"stage5e-b3c-source-authority-freeze-extension-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def marked_region(text: str, begin: str, end: str, name: str) -> str:
    if text.count(begin) != 1 or text.count(end) != 1:
        fail(f"{name} marker cardinality drift")
    before, remainder = text.split(begin, 1)
    region, after = remainder.split(end, 1)
    if len(before) + len(begin) >= len(before) + len(begin) + len(region) + len(end):
        fail(f"{name} marker order drift")
    if not after and not region:
        fail(f"{name} region empty")
    return region


def require_count(text: str, token: str, expected: int, name: str) -> None:
    actual = text.count(token)
    if actual != expected:
        fail(f"{name} cardinality drift for {token!r}: actual={actual} expected={expected}")


def run_predecessor(checker: str, label: str) -> None:
    result = subprocess.run(
        [sys.executable, checker],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(f"{label} predecessor contract failed")


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    if canonical_sha256(payload) != EXPECTED_INVENTORY_SHA256:
        fail("authority freeze contract drift")
    if sha256(PLAN) != EXPECTED_PLAN_SHA256:
        fail("authority freeze plan drift")
    if payload.get("schema_version") != 8:
        fail("implementation schema drift")
    if payload.get("stage") != "5E-b3c-source-authority-freeze-extension":
        fail("implementation identity drift")
    if payload.get("status") != "r6_additive_production_bridge_pending_review":
        fail("implementation status drift")
    if payload.get("baseline_ref") != BASELINE_REF:
        fail("implementation baseline drift")
    if payload.get("expected_provenance_case_count") != 200:
        fail("implementation negative-matrix count drift")
    if payload.get("production_source_baselines") != EXPECTED_SOURCE_BASELINES:
        fail("implementation source baseline drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_CHANGED_PATHS:
        fail("implementation changed-path contract drift")
    if payload.get("implementation_authorization") != {
        "authority_freeze_r6_reviewed": True,
        "production_source_changes_allowed": True,
        "trusted_combined_eligibility": True,
        "unverified_sequence_production_authoritative": False,
        "callback_intent_live_authorized": False,
    }:
        fail("implementation authorization drift")

    for rel, expected in EXPECTED_SOURCE_BASELINES.items():
        if sha256(ROOT / rel) != expected:
            fail(f"protected implementation source changed: {rel}")

    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", BASELINE_REF, "--"],
            cwd=ROOT,
            text=True,
        ).splitlines()
        if sorted(changed) != sorted(EXPECTED_ALLOWED_CHANGED_PATHS):
            fail("implementation review diff drift")
    if json.loads(ACTIVE.read_text()) != {
        "schema_version": 1,
        "stage": "5E-b3c-source-authority-freeze-extension",
    }:
        fail("active descriptor drift")

    plan = PLAN.read_text()
    for marker in (
        "R6 below is the",
        "sole operative contract",
        "reviewed R6 handoff at `2b2c57d`",
        "R6 additive implementation outcome",
        "No strategy callback is invoked",
        "Redis, FINAM I/O, transport, dispatch, runtime-live",
    ):
        if marker not in plan:
            fail(f"implementation plan marker missing: {marker}")

    stage4 = (ROOT / "crates/broker-core/src/stage4_bootstrap.rs").read_text()
    stage5c = (ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs").read_text()
    stage5e = (ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs").read_text()
    stage5c_region = marked_region(stage5c, STAGE5C_BEGIN, STAGE5C_END, "Stage 5C R6")
    stage5e_region = marked_region(stage5e, STAGE5E_BEGIN, STAGE5E_END, "Stage 5E R6")

    for marker in (
        "schedule_state: BrokerMarketSessionState",
        "pub fn schedule_state(&self) -> BrokerMarketSessionState",
        "schedule_state: validated.schedule_state",
    ):
        if marker not in stage4:
            fail(f"Stage 4 dynamic Open retention missing: {marker}")
    if "evidence.schedule_state()" not in stage5e_region:
        fail("accepted Stage 4 dynamic state is not projected")
    if "BrokerMarketSessionState::Open" not in stage5e_region:
        fail("exact Stage 4 Open check missing")

    for marker in (
        "Stage5cSequenceCandidateSeal",
        "Stage5cClassifiedSequenceSeal",
        "stage5e-b3c-market-sequence-v2",
        "stage5e-b3c-stage3-provenance-v1",
        "stage5e-b3c-semantic-bar-v1",
        "stage5e-b3c-recovery-receipt-v1",
        "SequenceAlreadyExpired",
        "sequence_expires_at = admission.expires_at().min(ttl_expiry)",
    ):
        if marker not in stage5c_region:
            fail(f"Stage 5C R6 marker missing: {marker}")
    if "sequence_identity_fingerprint" in stage5c_region.split(
        "pub(crate) struct Stage5cSequenceCandidateSeal", 1
    )[1].split("}", 1)[0]:
        fail("preclassification candidate owns final sequence identity")
    require_count(
        stage5c_region,
        "build_stage5c_sequence_candidate_seal_inside_stage5e_try_observe_live_bar_after_history_with_sequence_evidence(",
        2,
        "candidate constructor",
    )
    require_count(
        stage5c_region,
        ".classify_with_owned_projection(",
        1,
        "sealed classifier call",
    )
    require_count(stage5c_region, ".consume_for_b3b(", 0, "B3B consume call outside owner")

    for marker in (
        "Stage5eScheduleProjectionBridgeInput",
        "Stage5eScheduleCandidateClassifier",
        "classify_from_stage5c_seal_fields",
        "stage5e-b3c-non-tradable-boundary-v1",
        "stage5e-b3b-schedule-observed-sequence-binding-v2",
        "stage5e-continuation-binding-v3",
        "pub(crate) mod b3c_evidence",
        "schedule.expires_at.0.min(b3b.payload.sequence_expires_at)",
        ".max(b3b.payload.sequence_observed_at)",
    ):
        if marker not in stage5e_region:
            fail(f"Stage 5E R6 marker missing: {marker}")
    require_count(
        stage5e_region,
        "Stage5eB3bConsumeSeal(())",
        2,
        "B3B consume seal type plus sole issuer",
    )
    require_count(stage5e_region, ".consume_for_b3b(", 1, "B3B sealed consumer")
    require_count(
        stage5e_region,
        "pub(crate) fn into_stage5e_schedule_candidate_classifier(",
        1,
        "classifier constructor",
    )
    if "UnverifiedMarketSequenceSource" in stage5e_region:
        fail("unverified sequence entered production R6 region")

    for region_name, region in (("Stage 5C R6", stage5c_region), ("Stage 5E R6", stage5e_region)):
        for forbidden in (
            "on_broker_bar",
            "BrokerNeutralHybridIntent",
            "FinamRestClient",
            "reqwest",
            "redis::",
            "std::fs",
            "std::net",
        ):
            if forbidden in region:
                fail(f"forbidden {region_name} surface: {forbidden}")

    for test_name in payload.get("required_implementation_tests", []):
        semantic_token = test_name.split("::")[-1]
        if semantic_token not in stage4 + stage5c + stage5e:
            # Some requirements are enforced structurally by this checker and
            # the negative matrix rather than by one same-named Rust test.
            structural = {
                "schedule_projection_bridge_cannot_be_raw_constructed_or_reused",
                "Stage5cSequenceCandidateSeal_has_one_constructor_and_no_getters",
                "preclassification_candidate_has_no_final_sequence_identity",
                "Stage5cClassifiedSequenceSeal_is_created_only_after_concrete_classification",
                "final_sequence_identity_uses_accepted_stage3_digest_and_classification_boundary",
                "Stage5eScheduleCandidateClassifier_has_one_constructor_and_one_call_site",
                "sealed_classifier_never_exposes_raw_sessions_to_stage5c",
                "single_linear_issuer_preserves_strategy_recovery_and_bar",
                "observed_live_bar_with_sequence_has_one_constructor_and_sealed_B3B_consumer",
                "B3B_consume_seal_has_one_constructor_and_one_consumer",
                "B3B_consumes_new_output_and_revalidates_sequence_freshness",
                "sequence_created_expired_blocks_before_receipt",
                "B3C_revalidates_production_clock_expiry_observation_bar_and_sequence_time",
                "B3C_effective_expiry_is_exact_min_of_projection_and_sequence",
                "B3C_test_clock_seam_is_cfg_test_only",
                "expected_close_grid_uses_discrete_endpoint_and_candidate_classification",
                "stage4_schedule_section_identity_changes_for_every_frozen_field",
                "sequence_identity_changes_for_each_freshness_classification_and_boundary_field",
                "every_nested_identity_field_changes_its_fingerprint",
                "restart_reissues_full_receipt_chain_without_identity_mixing",
                "all_required_predecessor_and_freeze_updates_present",
                "stage4_dynamic_open_is_retained_from_accepted_input",
            }
            if test_name not in structural:
                fail(f"required implementation evidence missing: {test_name}")

    run_predecessor("scripts/stage5e_b_no_io_lifecycle_check.py", "Stage 5E-b")
    run_predecessor("scripts/stage5e_b3_schedule_window_evidence_check.py", "Stage 5E-b3")
    run_predecessor("scripts/stage5e_b3c_private_eligibility_seam_check.py", "Stage 5E-b3c")
    run_predecessor("scripts/stage5d_additive_freeze_check.py", "Stage 5D additive")
    print("stage5e-b3c-source-authority-freeze-extension-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
