#!/usr/bin/env python3
"""Fail-closed design gate for the Stage 5E-b3c authority freeze extension."""

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json"
PLAN = ROOT / "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md"
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
BASELINE_REF = "936250e675ac15b61a7a4e319b59e508cd834f30"

EXPECTED_SOURCE_BASELINES = {
    "crates/broker-core/src/lib.rs": "5d8758624f53a6b46d8903dd3f2339d5bd04f64c9c6490448167f08ac68ec8a2",
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": "14a723bd2adf98f50c2443166b7fb838edd8df6c5cf46968d13eb9e8d901b4c9",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": "76bade52f3ebb309475812b617823825a3b7e4838bf89f9eb297ca2bbffbf821",
}
EXPECTED_ALLOWED_DESIGN_PATHS = [
    "docs/stage-5/5e-b3c-private-eligibility-seam-plan.md",
    "docs/stage-5/5e-b3c-source-authority-freeze-extension-plan.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3c-source-authority-freeze-extension-inventory.json",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]


def fail(message: str) -> None:
    print(f"stage5e-b3c-source-authority-freeze-extension-check: FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    payload = json.loads(INVENTORY.read_text())
    expected_keys = {
        "schema_version", "stage", "status", "baseline_ref", "predecessor_stage",
        "production_source_baselines", "implementation_authorization",
        "proposed_owner_receipts", "fail_closed_rules", "closed_surfaces",
        "required_implementation_tests", "required_negative_mutations",
        "allowed_changed_paths",
    }
    if set(payload) != expected_keys:
        fail("inventory key set drift")
    if payload.get("schema_version") != 1 or payload.get("stage") != "5E-b3c-source-authority-freeze-extension":
        fail("inventory identity drift")
    if payload.get("status") != "design_only_pending_review" or payload.get("baseline_ref") != BASELINE_REF:
        fail("design baseline or status drift")
    if payload.get("predecessor_stage") != "5E-b3c-private-eligibility-seam":
        fail("predecessor stage drift")
    if payload.get("production_source_baselines") != EXPECTED_SOURCE_BASELINES:
        fail("production source baseline contract drift")
    authorization = payload.get("implementation_authorization")
    if authorization != {
        "stage4_stage5c_freeze_extension_reviewed": False,
        "production_source_changes_allowed": False,
        "trusted_combined_eligibility": False,
        "unverified_sequence_production_authoritative": False,
    }:
        fail("implementation authority drift")
    if payload.get("allowed_changed_paths") != EXPECTED_ALLOWED_DESIGN_PATHS:
        fail("allowed design path drift")
    if payload.get("closed_surfaces") != [
        "strategy_callback", "strategy_state_mutation", "executable_intents",
        "strategy_intent_sink", "redis", "finam_io", "transport", "dispatch",
        "runtime_live", "broker_execution", "autonomous_event_loop",
    ]:
        fail("closed surface contract drift")
    owner_receipts = payload.get("proposed_owner_receipts")
    if not isinstance(owner_receipts, dict) or set(owner_receipts) != {
        "stage4_open_session", "stage5c_market_sequence", "stage5e_continuation_binding",
    }:
        fail("owner receipt contract drift")
    if owner_receipts["stage4_open_session"].get("requires_exact_state") != "BrokerMarketSessionState::Open":
        fail("dynamic Open-state contract drift")
    if owner_receipts["stage5c_market_sequence"].get("owner") != "stage5c_canonical_history_and_semantic_bar":
        fail("Stage 5C owner contract drift")
    continuation = owner_receipts["stage5e_continuation_binding"]
    if continuation.get("constant_epoch_allowed") is not False or "deterministic_digest" not in continuation.get("model", ""):
        fail("continuation lineage contract drift")
    for rule in (
        "static_tradable_open_window_does_not_prove_dynamic_open_state",
        "unverified_market_sequence_is_test_only",
        "raw_dto_or_boolean_cannot_construct_owner_receipt",
        "missing_or_expired_owner_receipt_blocks",
        "no_trusted_combined_eligibility_before_extension_acceptance",
    ):
        if rule not in payload.get("fail_closed_rules", []):
            fail(f"missing fail-closed rule: {rule}")
    required_tests = payload.get("required_implementation_tests", [])
    required_mutations = payload.get("required_negative_mutations", [])
    if len(required_tests) != 7 or len(required_mutations) != 7:
        fail("implementation evidence matrix drift")
    for rel, expected in EXPECTED_SOURCE_BASELINES.items():
        if sha256(ROOT / rel) != expected:
            fail(f"production source changed before freeze-extension review: {rel}")
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", BASELINE_REF, "--"], cwd=ROOT, text=True
    ).splitlines()
    if any(path not in EXPECTED_ALLOWED_DESIGN_PATHS for path in changed):
        fail("out-of-scope source changed in design-only package")
    if json.loads(ACTIVE.read_text()) != {
        "schema_version": 1,
        "stage": "5E-b3c-source-authority-freeze-extension",
    }:
        fail("active descriptor drift")
    plan = PLAN.read_text()
    for marker in (
        "design only", "Stage4AcceptedOpenSessionEvidence",
        "Stage5cAcceptedMarketSequenceEvidence", "Stage5eContinuationBindingId",
        "UnverifiedMarketSequenceSource", "fail-closed", "BrokerMarketSessionState::Open",
        "no-I/O", "no-send",
    ):
        if marker not in plan:
            fail(f"plan marker missing: {marker}")
    predecessor = subprocess.run(
        [sys.executable, "scripts/stage5e_b3c_private_eligibility_seam_check.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if predecessor.returncode != 0:
        fail("B3C predecessor contract failed")
    print("stage5e-b3c-source-authority-freeze-extension-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
