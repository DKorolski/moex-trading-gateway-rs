#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3e callback invocation design."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3e-callback-invocation-design.md"
INVENTORY = (
    ROOT / "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
STAGE = "5E-b3e-callback-invocation-design"
BASELINE_REF = "93d365ae51f2f6ad94954782a27bc49857fe21ff"
EXPECTED_PLAN_SHA256 = (
    "6d9ca1ec694beec5fcfae23dd0e4cb8b17817896c05ccbbd7072b95b74bbc057"
)
EXPECTED_INVENTORY_SHA256 = (
    "2896d9218a2ca9e98c5a6c0a2582cf32c653ddf272679594b0a9d58457f69ca4"
)
EXPECTED_B3D_SOURCE_SHA256 = {
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "9637a6065452b7b46581601bbee8c0270f65dc04207f15b530d3531a36872d1c"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "30d87cb4313b961f3159d2ca4e5ef214ee2009d0358a96ace945e6794b41ae6c"
    ),
    "docs/stage-5/5e-b3d-callback-authority-design.md": (
        "ae58b3b839782fa4f899079bb19edcac84840246f415d753314a1ad19e5476b6"
    ),
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json": (
        "97800a8d0de98266cdc26ebc85bdee224c7ab4ff6705636a9136eda8a6a54037"
    ),
    "scripts/stage5e_b3d_callback_authority_design_check.py": (
        "143a719d916ca2f539fa1faf0849f69d1780a9ba86cd78120b9808cdf9747ff0"
    ),
}
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "docs/stage-5/5e-b3e-callback-invocation-design.md",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3e-callback-invocation-design-inventory.json",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3e_callback_invocation_design_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]


def fail(message: str) -> None:
    print(
        f"stage5e-b3e-callback-invocation-design-check: FAIL: {message}",
        file=sys.stderr,
    )
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical_sha256(value: object) -> str:
    return hashlib.sha256(
        json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def require_exact(value: object, expected: object, message: str) -> None:
    if value != expected:
        fail(message)


def check_inventory(inventory: dict[str, object]) -> None:
    require_exact(
        canonical_sha256(inventory),
        EXPECTED_INVENTORY_SHA256,
        "design inventory drift",
    )
    require_exact(sha256(PLAN), EXPECTED_PLAN_SHA256, "design plan drift")
    require_exact(
        json.loads(ACTIVE.read_text()),
        {"schema_version": 1, "stage": STAGE},
        "active descriptor drift",
    )
    require_exact(inventory.get("schema_version"), 1, "schema drift")
    require_exact(inventory.get("stage"), STAGE, "stage identity drift")
    require_exact(
        inventory.get("status"),
        "design_only_pending_review",
        "design-only status drift",
    )
    require_exact(inventory.get("baseline_ref"), BASELINE_REF, "baseline drift")
    require_exact(
        inventory.get("accepted_b3d_implementation_ref"),
        BASELINE_REF,
        "accepted B3D implementation ref drift",
    )
    require_exact(
        inventory.get("expected_provenance_case_count"),
        249,
        "negative-matrix count drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "design changed-path contract drift",
    )
    require_exact(
        inventory.get("accepted_b3d_source_sha256"),
        EXPECTED_B3D_SOURCE_SHA256,
        "accepted B3D source hash contract drift",
    )

    transition = inventory["invocation_transition_contract"]
    require_exact(
        transition,
        {
            "function": "invoke_stage5e_authorized_paper_callback",
            "implementation_status": "design_only_not_implemented",
            "only_input": "Stage5eCallbackAuthorityReadyPaperStrategy",
            "success": "Stage5ePaperCallbackResultEscrow",
            "blocked": "Stage5eCallbackInvocationTerminalBlock",
            "production_clock": "captured_inside_transition",
            "caller_supplied_production_clock_allowed": False,
            "test_clock_seam": "cfg_test_only",
            "preflight_before_consume": True,
            "callback_after_all_checks_only": True,
            "callback_count_on_blocked_path": 0,
            "callback_count_on_success_path": 1,
            "callback_invocation_implies_in_memory_intent_construction": True,
            "legacy_stage5c_route_allowed": False,
        },
        "invocation transition contract drift",
    )
    preflight = inventory["callback_time_preflight_contract"]
    require_exact(
        preflight["checks_in_order"],
        [
            "issued_at_not_after_now",
            "now_not_after_authority_expires_at",
            "accepted_bar_close_not_future",
            "issued_at_not_after_authority_expires_at",
            "authority_expiry_equals_owned_b3c_effective_expiry",
            "full_instrument_identity_complete",
            "all_frozen_identity_fields_present_and_nonzero",
            "owned_identity_fields_match_b3c",
            "callback_authority_id_recomputed_and_equal",
        ],
        "callback-time check order drift",
    )
    for field in (
        "grace_period_allowed",
        "expiry_extension_allowed",
        "refresh_allowed",
        "ledger_lookup_allowed",
        "persisted_capability_allowed",
        "surrogate_ownership_id_allowed",
    ):
        require_exact(preflight[field], False, f"forbidden preflight surface opened: {field}")

    terminal = inventory["terminal_block_contract"]
    require_exact(
        terminal["reasons"],
        [
            "ClockBeforeAuthorityIssue",
            "AuthorityExpired",
            "AcceptedBarObservedInFuture",
            "InvalidAuthorityChronology",
            "InstrumentIdentityMissing",
            "OwnedIdentityMismatch",
            "CallbackAuthorityIdMismatch",
        ],
        "terminal reason taxonomy drift",
    )
    for field in (
        "returns_authority_receipt",
        "retry_allowed",
        "refresh_allowed",
        "reconstruction_allowed",
        "unbinding_allowed",
    ):
        require_exact(terminal[field], False, f"terminal authority reuse opened: {field}")

    callback = inventory["callback_execution_contract"]
    require_exact(
        callback["callback"],
        "BrokerNeutralHybridStrategy::on_broker_bar",
        "callback identity drift",
    )
    require_exact(callback["exactly_once_after_preflight"], True, "callback cardinality drift")
    require_exact(callback["uses_legacy_stage5c_apply"], False, "legacy apply route opened")
    require_exact(callback["uses_legacy_stage5c_loop"], False, "legacy loop route opened")
    require_exact(callback["catch_unwind_allowed"], False, "callback unwind retry surface opened")
    require_exact(callback["panic_retry_allowed"], False, "callback panic retry opened")
    require_exact(
        callback["actual_callback_status"],
        "hold_until_separate_implementation_review",
        "actual callback implementation opened",
    )

    escrow = inventory["result_escrow_contract"]
    require_exact(
        escrow["implementation_status"],
        "design_only_not_implemented",
        "escrow implementation opened",
    )
    require_exact(
        escrow["callback_error_retains_post_callback_strategy"],
        True,
        "callback-error ownership lost",
    )
    require_exact(
        escrow["callback_error_retry_allowed"],
        False,
        "callback-error retry opened",
    )
    required_forbidden = {
        "Clone",
        "Serialize",
        "Deserialize",
        "into_parts",
        "raw_strategy_getter",
        "intent_getter",
        "intent_iterator",
        "intent_sink_conversion",
        "command_conversion",
        "send_capable_consumer",
        "execution_ready_marker",
    }
    if not required_forbidden.issubset(set(escrow["forbidden_traits_and_surfaces"])):
        fail("private escrow forbidden-surface set weakened")
    if any(value is not False for value in inventory["closed_surfaces"].values()):
        fail("a design-only closed surface was opened")


def check_accepted_source_is_unchanged() -> None:
    for rel, expected in EXPECTED_B3D_SOURCE_SHA256.items():
        if sha256(ROOT / rel) != expected:
            fail(f"accepted B3D source drift: {rel}")
    runtime_source = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    for forbidden in (
        "fn invoke_stage5e_authorized_paper_callback(",
        "struct Stage5eCallbackInvocationSeal",
        "struct Stage5eCallbackInvocationPreflight",
        "struct Stage5ePaperCallbackResultEscrow",
        "struct Stage5eCallbackInvocationTerminalBlock",
    ):
        if forbidden in runtime_source:
            fail(f"design-only stage contains implementation symbol: {forbidden}")


def check_plan_markers() -> None:
    text = PLAN.read_text()
    for marker in (
        "This is a design-only stage",
        "93d365ae51f2f6ad94954782a27bc49857fe21ff",
        "invoke_stage5e_authorized_paper_callback",
        "Stage5eCallbackInvocationSeal",
        "Stage5eCallbackInvocationPreflight<'a>",
        "Stage5ePaperCallbackResultEscrow",
        "BrokerNeutralHybridStrategy::on_broker_bar exactly once",
        "A callback validation error remains inside the escrow",
        "Stage4AcceptedPaperHostEvidence → B3C → callback authority",
        "Any implementation requires a separate accepted assignment and review.",
    ):
        if marker not in text:
            fail(f"required design marker missing: {marker}")


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid B3E design contract: {exc}")
    check_inventory(inventory)
    check_accepted_source_is_unchanged()
    check_plan_markers()
    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", BASELINE_REF, "--"],
            cwd=ROOT,
            text=True,
        ).splitlines()
        if sorted(changed) != sorted(EXPECTED_ALLOWED_CHANGED_PATHS):
            fail("B3E design review diff drift")
    print("stage5e-b3e-callback-invocation-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
