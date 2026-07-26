#!/usr/bin/env python3
"""Fail-closed checker for Stage 5E-b3d-r1 governance hardening."""

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
STAGE_BASELINE_REF = "ff1344f170b8457df91a6038d670087eef3cc1dc"
R1_REVIEW_PREDECESSOR_REF = "95096b7d28ecd3fafddbbfd3ec91b0611019e0eb"
STAGE = "5E-b3d-callback-authority-design"

EXPECTED_INVENTORY_SHA256 = (
    "e965664ce35449c982b4f7ce306e2479ecfaa728e3ac881f5596e3cc9ea13a88"
)
EXPECTED_PLAN_SHA256 = (
    "1420ef4f69e57cd0f7b5877fcef154091c6a982400ef6a0c23e004e6e7f79d01"
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
EXPECTED_R1_CHANGED_PATHS = [
    "docs/stage-5/5e-b3d-callback-authority-design.md",
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/stage5e_b3d_callback_authority_design_check.py",
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


def require_exact(value: object, expected: object, message: str) -> None:
    if value != expected:
        fail(message)


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
        active = json.loads(ACTIVE.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid R1 governance: {exc}")

    if canonical_sha256(inventory) != EXPECTED_INVENTORY_SHA256:
        fail("R1 inventory drift")
    if sha256(PLAN) != EXPECTED_PLAN_SHA256:
        fail("R1 plan drift")
    require_exact(active, {"schema_version": 1, "stage": STAGE}, "active descriptor drift")
    if inventory.get("schema_version") != 2 or inventory.get("stage") != STAGE:
        fail("R1 identity drift")
    if inventory.get("status") != "r1_governance_hardening_pending_review":
        fail("R1 status drift")
    if inventory.get("baseline_ref") != STAGE_BASELINE_REF:
        fail("R1 baseline drift")
    if inventory.get("predecessor_ref") != R1_REVIEW_PREDECESSOR_REF:
        fail("R1 predecessor drift")
    if inventory.get("expected_provenance_case_count") != 225:
        fail("R1 negative-matrix count drift")
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "R1 changed-path contract drift",
    )
    require_exact(
        inventory.get("protected_source_sha256"),
        EXPECTED_PROTECTED_SOURCE_SHA256,
        "protected source contract drift",
    )
    for rel, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
        if sha256(ROOT / rel) != expected:
            fail(f"governance-only R1 changed protected source: {rel}")

    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", R1_REVIEW_PREDECESSOR_REF, "--"],
            cwd=ROOT,
            text=True,
        ).splitlines()
        if sorted(changed) != sorted(EXPECTED_R1_CHANGED_PATHS):
            fail("R1 review diff drift")

    require_exact(
        inventory.get("route_exclusivity_contract"),
        {
            "decision": "explicit_isolated_scope",
            "sole_new_stage5e_callback_input": (
                "Stage5eCallbackAuthorityReadyPaperStrategy"
            ),
            "sole_new_stage5e_callback_transition": (
                "invoke_stage5e_authorized_paper_callback"
            ),
            "legacy_stage5c_routes": [
                "apply_stage5c_semantic_bar",
                "advance_stage5c_paper_loop_once",
            ],
            "legacy_route_scope": "paper_oracle_compatibility_only",
            "legacy_route_stage5e_runtime_attachment_allowed": False,
            "stage5c_api_freeze_extension_required_now": False,
            "future_runtime_call_graph_negative_evidence_required": True,
        },
        "callback route exclusivity drift",
    )

    receipt = inventory.get("callback_authority_receipt_contract")
    if not isinstance(receipt, dict):
        fail("authority receipt contract missing")
    require_exact(
        receipt.get("fields"),
        [
            "b3c_receipt",
            "callback_authority_id",
            "issued_at",
            "effective_observed_at",
            "authority_expires_at",
            "accepted_bar_close_ts",
            "full_instrument_id",
            "accepted_semantic_bar_identity",
            "event_key_fingerprint",
            "continuation_binding_id",
            "sequence_identity_fingerprint",
        ],
        "authority receipt field schema drift",
    )
    if (
        receipt.get("owner_module")
        != "strategy_runtime_core::stage5e_no_io_lifecycle::callback_authority"
        or receipt.get("visibility") != "crate_private_opaque"
        or receipt.get("owns_complete_b3c_receipt") is not True
        or receipt.get("successful_unbinding_allowed") is not False
        or receipt.get("persistence_allowed") is not False
        or receipt.get("restart_reconstruction_allowed") is not False
    ):
        fail("authority receipt ownership or lifetime drift")
    require_exact(
        receipt.get("authority_vector"),
        {
            "callback_ready": True,
            "callback_invoked": False,
            "execution_ready": False,
            "calls_strategy": False,
            "mutates_strategy": False,
            "creates_in_memory_intents": False,
            "creates_executable_intent": False,
            "intent_count": 0,
        },
        "authority issue vector drift",
    )

    authority_id = inventory.get("callback_authority_id_contract")
    if not isinstance(authority_id, dict):
        fail("authority identity contract missing")
    if (
        authority_id.get("domain") != "stage5e-callback-authority-v1"
        or authority_id.get("algorithm")
        != "sha256_tagged_length_prefixed_canonical_bytes"
        or authority_id.get("issuance_ledger_required") is not False
        or authority_id.get("duplicate_runtime_blocker_present") is not False
        or authority_id.get("crash_policy")
        != "capability_lost_rebuild_full_chain_from_fresh_evidence"
    ):
        fail("authority identity or exactly-once contract drift")
    if len(authority_id.get("fields_in_order", [])) != 7:
        fail("authority identity field coverage drift")

    ownership = inventory.get("ownership_contract")
    require_exact(
        ownership,
        {
            "proof": "linear_type_ownership_of_complete_b3c_receipt",
            "production_ownership_binding_id_present": False,
            "runtime_ownership_mismatch_blocker_present": False,
            "test_only_state_fingerprint_allowed": True,
            "test_fingerprint_production_authority_allowed": False,
        },
        "ownership proof contract drift",
    )

    issue = inventory.get("callback_authority_issue_transition")
    if not isinstance(issue, dict):
        fail("authority issue transition missing")
    if (
        issue.get("input")
        != "Stage5eBoundSessionCalendarSequenceForObservedLiveBar"
        or issue.get("success") != "Stage5eCallbackAuthorityReadyPaperStrategy"
        or issue.get("issue_seal") != "Stage5eCallbackAuthorityIssueSeal"
        or issue.get("preflight_view") != "Stage5eCallbackAuthorityPreflight<'a>"
        or issue.get("consume_order") != "preflight_then_single_linear_consume"
        or issue.get("production_clock") != "captured_inside_transition"
        or issue.get("caller_supplied_production_clock_allowed") is not False
        or issue.get("authority_expires_at_formula")
        != "b3c_effective_expires_at"
        or issue.get("grace_period_allowed") is not False
        or issue.get("expiry_extension_allowed") is not False
    ):
        fail("authority issue transition drift")

    block = inventory.get("issue_block_contract")
    if not isinstance(block, dict):
        fail("authority issue blocker contract missing")
    if (
        block.get("retryable_type") != "Stage5eCallbackAuthorityRetryableBlock"
        or block.get("retryable_conversion") != "into_retry_same_receipt"
        or block.get("terminal_type") != "Stage5eCallbackAuthorityTerminalBlock"
        or block.get("terminal_retry_refresh_or_unbinding_allowed") is not False
        or block.get("refresh_type_present") is not False
        or block.get("refresh_conversion_present") is not False
        or block.get("expired_evidence_policy")
        != "terminal_drop_and_rebuild_from_fresh_accepted_chain"
        or block.get("autonomous_retry_authorized") is not False
    ):
        fail("authority blocker type-state drift")
    terminal_reasons = block.get("terminal_reasons", [])
    if (
        "EvidenceExpired" not in terminal_reasons
        or "DuplicateAuthorityIssue" in terminal_reasons
        or "OwnershipBindingMismatch" in terminal_reasons
    ):
        fail("unimplementable blocker reintroduced")

    invocation = inventory.get("callback_authority_invocation_contract")
    if not isinstance(invocation, dict):
        fail("callback invocation contract missing")
    if (
        invocation.get("implementation_status") != "hold_future_separate_review"
        or invocation.get("only_input")
        != "Stage5eCallbackAuthorityReadyPaperStrategy"
        or invocation.get("production_clock") != "captured_inside_transition"
        or invocation.get("callback_invocation_after_all_checks_only") is not True
        or invocation.get("output") != "Stage5ePaperCallbackResultEscrow"
        or invocation.get("callback_invocation_implies_in_memory_intent_construction")
        is not True
        or invocation.get("intent_sink_allowed") is not False
        or invocation.get("send_allowed") is not False
        or invocation.get("execution_allowed") is not False
    ):
        fail("callback-time authority contract drift")
    callback_checks = invocation.get("callback_time_checks", [])
    for required in (
        "now_not_after_authority_expires_at",
        "callback_authority_id_recomputed_and_equal",
        "immutable_identity_fields_match_owned_b3c_receipt",
    ):
        if required not in callback_checks:
            fail(f"callback-time revalidation missing: {required}")

    closed = inventory.get("closed_surfaces")
    if not isinstance(closed, dict) or any(value is not False for value in closed.values()):
        fail("closed surface opened")
    providers = inventory.get("provider_and_calendar_contract")
    if not isinstance(providers, dict) or any(
        value is not False for value in providers.values()
    ):
        fail("provider or venue-calendar gate opened")

    stage5c = (
        ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    ).read_text()
    lib = (ROOT / "crates/strategy-runtime-core/src/lib.rs").read_text()
    for legacy in (
        "pub fn apply_stage5c_semantic_bar(",
        "pub fn advance_stage5c_paper_loop_once(",
    ):
        if legacy not in stage5c:
            fail(f"legacy paper/oracle route unexpectedly changed: {legacy}")
    if "apply_stage5c_semantic_bar," not in lib:
        fail("legacy Stage 5C API export unexpectedly changed")
    protected_stage5e = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    if "Stage5eCallbackAuthorityReadyPaperStrategy" in protected_stage5e:
        fail("governance-only authority type entered production source")

    plan = PLAN.read_text()
    for marker in (
        "governance/design-only, pending review",
        "explicit isolated scope",
        "paper/oracle compatibility APIs",
        "stage5e-callback-authority-v1",
        "authority_expires_at = effective_expires_at",
        "There is no issuance ledger",
        "There is no production `ownership_binding_id`",
        "R1 deliberately has no refresh output",
        "invoke_stage5e_authorized_paper_callback",
        "now <= authority_expires_at",
        "in-memory paper intent construction",
        "Actual callback invocation and escrow implementation remain HOLD",
    ):
        if marker not in plan:
            fail(f"required R1 plan marker missing: {marker}")

    run_predecessor()
    print("stage5e-b3d-callback-authority-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
