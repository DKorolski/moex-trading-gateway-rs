#!/usr/bin/env python3
"""Fail-closed checker for the Stage 5E-b3d private no-I/O issue boundary."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / "docs/stage-5/5e-b3d-callback-authority-design.md"
INVENTORY = (
    ROOT / "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json"
)
ACTIVE = ROOT / "docs/stage-5/stage5e-active-descriptor.json"
STAGE = "5E-b3d-callback-authority-design"
STAGE_BASELINE_REF = "ff1344f170b8457df91a6038d670087eef3cc1dc"
IMPLEMENTATION_PREDECESSOR_REF = "583241d441d94688d86e462ca9d066bf88dec2b9"

EXPECTED_INVENTORY_SHA256 = (
    "fc1c161b1a0d99104a88f03438095a2b4dc3927b2feecd0bb5c51a5b2cd92fff"
)
EXPECTED_PLAN_SHA256 = (
    "ae58b3b839782fa4f899079bb19edcac84840246f415d753314a1ad19e5476b6"
)
EXPECTED_PREDECESSOR_CHECKER_SHA256 = (
    "9f2649f22fd282df580271c189f206bb4de2e9893e8d34dd8eb93f883a8b0889"
)
EXPECTED_PRIVATE_PREDECESSOR_CHECKER_SHA256 = (
    "621e0a78aa40db732698d63b405b5eea4b8a2ff9a836de01cc66ea2c363ba955"
)
EXPECTED_ALLOWED_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3d-callback-authority-design.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-active-descriptor.json",
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/handoff_safety_check.py",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
    "scripts/stage5e_b3c_private_eligibility_seam_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5e_b3d_callback_authority_design_check.py",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
    "scripts/stage5e_descriptor.py",
    "scripts/stage5e_lifecycle_event_time_gate.sh",
]
EXPECTED_IMPLEMENTATION_CHANGED_PATHS = [
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs",
    "docs/stage-5/5e-b3d-callback-authority-design.md",
    "docs/stage-5/stage-5d-additive-freeze-manifest.json",
    "docs/stage-5/stage5e-b3d-callback-authority-design-inventory.json",
    "scripts/forbidden_surface_scan.sh",
    "scripts/handoff_provenance_negative_harness.py",
    "scripts/stage5e_b3_schedule_window_evidence_check.py",
    "scripts/stage5e_b3c_source_authority_freeze_extension_check.py",
    "scripts/stage5e_b3d_callback_authority_design_check.py",
    "scripts/stage5e_b_no_io_lifecycle_check.py",
]
EXPECTED_PROTECTED_SOURCE_SHA256 = {
    "crates/broker-core/src/stage4_bootstrap.rs": (
        "33455bd4447193f723aa5a749707739d89e2d2ca58b083d416c268a24613bdd7"
    ),
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs": (
        "7f5e3ad070c1bbc3ddca1e642d59b3f4cf75b9bb0d1651068df363323f1cd427"
    ),
    "crates/strategy-runtime-core/src/stage5c_paper_host.rs": (
        "9637a6065452b7b46581601bbee8c0270f65dc04207f15b530d3531a36872d1c"
    ),
    "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs": (
        "30d87cb4313b961f3159d2ca4e5ef214ee2009d0358a96ace945e6794b41ae6c"
    ),
    "Cargo.toml": "1c3e7dd1b83a6a8942e02cb520d49f33ed3ef77f2970854b9fdcddc7f261bc3e",
    "Cargo.lock": "ff535d0490a848e43631906ee8abd8633630d162714299f7628c0e5fe8a0b36b",
}

AUTHORITY_BEGIN = (
    "// STAGE5E-B3D-CALLBACK-AUTHORITY-BEGIN: private-no-io-issue-v1"
)
AUTHORITY_END = "// STAGE5E-B3D-CALLBACK-AUTHORITY-END: private-no-io-issue-v1"


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


def require_exact(value: object, expected: object, message: str) -> None:
    if value != expected:
        fail(message)


def marked_region(text: str, begin: str, end: str) -> str:
    if text.count(begin) != 1 or text.count(end) != 1:
        fail("B3D implementation marker cardinality drift")
    return text.split(begin, 1)[1].split(end, 1)[0]


def run_predecessor() -> None:
    private_checker = ROOT / "scripts/stage5e_b3c_private_eligibility_seam_check.py"
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


def check_inventory(inventory: dict[str, object]) -> None:
    if canonical_sha256(inventory) != EXPECTED_INVENTORY_SHA256:
        fail("R1 inventory drift")
    if sha256(PLAN) != EXPECTED_PLAN_SHA256:
        fail("R1 plan drift")
    require_exact(
        json.loads(ACTIVE.read_text()),
        {"schema_version": 1, "stage": STAGE},
        "active descriptor drift",
    )
    require_exact(inventory.get("schema_version"), 2, "implementation schema drift")
    require_exact(inventory.get("stage"), STAGE, "implementation identity drift")
    require_exact(
        inventory.get("status"),
        "private_no_io_authority_issue_implementation_pending_review",
        "implementation status drift",
    )
    require_exact(
        inventory.get("baseline_ref"),
        STAGE_BASELINE_REF,
        "implementation stage baseline drift",
    )
    require_exact(
        inventory.get("predecessor_ref"),
        IMPLEMENTATION_PREDECESSOR_REF,
        "implementation predecessor drift",
    )
    require_exact(
        inventory.get("expected_provenance_case_count"),
        239,
        "implementation negative-matrix count drift",
    )
    require_exact(
        inventory.get("allowed_changed_paths"),
        EXPECTED_ALLOWED_CHANGED_PATHS,
        "implementation cumulative changed-path contract drift",
    )
    require_exact(
        inventory.get("protected_source_sha256"),
        EXPECTED_PROTECTED_SOURCE_SHA256,
        "implementation source hash contract drift",
    )
    receipt = inventory["callback_authority_receipt_contract"]
    require_exact(
        receipt.get("implementation_status"),
        "implemented_private_no_io_issue_only",
        "authority receipt implementation status drift",
    )
    require_exact(
        receipt["authority_vector"],
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
        "authority vector drift",
    )
    issue = inventory["callback_authority_issue_transition"]
    require_exact(
        issue.get("implementation_status"),
        "implemented_private_no_io",
        "authority issue implementation status drift",
    )
    require_exact(
        issue["authority_expires_at_formula"],
        "b3c_effective_expires_at",
        "authority expiry formula drift",
    )
    require_exact(issue["grace_period_allowed"], False, "authority grace opened")
    require_exact(issue["expiry_extension_allowed"], False, "authority expiry extension opened")
    require_exact(
        inventory["callback_authority_invocation_contract"]["implementation_status"],
        "hold_future_separate_review",
        "actual callback implementation opened",
    )
    require_exact(
        inventory["callback_result_escrow_contract"]["implementation_status"],
        "hold_future_separate_review",
        "callback escrow implementation opened",
    )
    if any(value is not False for value in inventory["closed_surfaces"].values()):
        fail("a B3D closed surface was opened")


def check_source() -> None:
    for rel, expected in EXPECTED_PROTECTED_SOURCE_SHA256.items():
        if sha256(ROOT / rel) != expected:
            fail(f"B3D implementation source drift: {rel}")

    module = (
        ROOT / "crates/strategy-runtime-core/src/stage5e_no_io_lifecycle.rs"
    ).read_text()
    host = (
        ROOT / "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
    ).read_text()
    lib = (ROOT / "crates/strategy-runtime-core/src/lib.rs").read_text()
    region = marked_region(module, AUTHORITY_BEGIN, AUTHORITY_END)

    receipt_definition = region.split(
        "pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {", 1
    )[1].split("\n    }", 1)[0]
    receipt_fields = [
        line.strip().split(":", 1)[0]
        for line in receipt_definition.splitlines()
        if ":" in line
    ]
    require_exact(
        receipt_fields,
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
    if re.search(
        r"#\[derive\([^\]]*(?:Debug|Clone|Copy|Serialize|Deserialize)[^\]]*\)\]\s*"
        r"pub\(crate\) struct Stage5eCallbackAuthorityReadyPaperStrategy",
        region,
        re.S,
    ):
        fail("authority receipt forbidden trait derivation detected")
    for forbidden in (
        "impl Clone for Stage5eCallbackAuthorityReadyPaperStrategy",
        "impl Copy for Stage5eCallbackAuthorityReadyPaperStrategy",
        "impl serde::Serialize for Stage5eCallbackAuthorityReadyPaperStrategy",
        "impl serde::Deserialize",
        "impl Default for Stage5eCallbackAuthorityReadyPaperStrategy",
        "impl From<",
        "impl Into<",
        "into_parts",
        "raw_strategy",
        "raw_semantic_bar",
        "invoke_stage5e_authorized_paper_callback",
        "Stage5ePaperCallbackResultEscrow",
        "on_broker_bar",
        "BrokerNeutralHybridIntent",
        "redis",
        "finam",
        "reqwest",
        "tokio",
        "std::fs",
        "std::net",
    ):
        haystack = region.lower() if forbidden in {"redis", "finam", "reqwest", "tokio"} else region
        if forbidden in haystack:
            fail(f"forbidden B3D implementation surface: {forbidden}")

    required_counts = {
        "pub(crate) struct Stage5eCallbackAuthorityId([u8; 32]);": 1,
        "pub(crate) struct Stage5eCallbackAuthorityIssueSeal(());": 1,
        "Stage5eCallbackAuthorityIssueSeal(())": 2,
        "pub(crate) struct Stage5eCallbackAuthorityPreflight<'a> {": 1,
        "pub(crate) struct Stage5eCallbackAuthorityReadyPaperStrategy {": 1,
        "pub(crate) struct Stage5eCallbackAuthorityRetryableBlock {": 1,
        "pub(crate) struct Stage5eCallbackAuthorityTerminalBlock {": 1,
        "pub(crate) fn issue_stage5e_callback_authority(": 1,
        "fn issue_stage5e_callback_authority_with_now(": 1,
        "pub(crate) fn issue_stage5e_callback_authority_at(": 1,
        "pub(crate) fn into_retry_same_receipt(": 1,
        "Utc::now()": 1,
        "const AUTHORITY_DOMAIN: &[u8] = b\"stage5e-callback-authority-v1\";": 1,
    }
    for token, expected in required_counts.items():
        actual = region.count(token)
        if actual != expected:
            fail(
                f"B3D implementation cardinality drift for {token!r}: "
                f"actual={actual} expected={expected}"
            )
    if (
        "#[cfg(test)]\n    pub(crate) fn issue_stage5e_callback_authority_at("
        not in region
    ):
        fail("B3D deterministic clock seam is not cfg(test)-only")
    if module.count("borrow_callback_authority_preflight(") != 2:
        fail("B3C borrowed preflight bridge cardinality drift")
    if module.count("Stage5eCallbackAuthorityPreflight::from_b3c_receipt(") != 1:
        fail("B3C borrowed preflight constructor cardinality drift")
    if host.count("self.accepted_semantic_bar.semantic_bar_identity") != 2:
        fail("accepted semantic-bar identity forwarding drift")
    if "pub use stage5e_no_io_lifecycle" in lib:
        fail("private B3D authority module leaked into public API")
    for legacy in ("apply_stage5c_semantic_bar", "advance_stage5c_paper_loop_once"):
        if legacy in region:
            fail(f"legacy Stage 5C callback route reachable from B3D issuer: {legacy}")

    id_function = region.split("fn callback_authority_id(", 1)[1].split(
        "\n    fn canonical_instrument_bytes(", 1
    )[0]
    ordered_id_tokens = [
        "encoder.field(1, &canonical_instrument_bytes(instrument));",
        "encoder.field(2, &accepted_semantic_bar_identity);",
        "encoder.field(3, &event_key_fingerprint);",
        "encoder.field(4, &continuation_binding_id);",
        "encoder.field(5, &sequence_identity_fingerprint);",
        "encoder.field(6, &issued_at.timestamp_millis().to_be_bytes());",
        "encoder.field(7, &authority_expires_at.timestamp_millis().to_be_bytes());",
    ]
    positions = [id_function.find(token) for token in ordered_id_tokens]
    if any(position < 0 for position in positions) or positions != sorted(positions):
        fail("canonical authority ID field order drift")
    for required in (
        "if now < preflight.effective_observed_at {",
        "if preflight.accepted_bar_close_ts > now.timestamp() {",
        "if now > preflight.effective_expires_at {",
        "if preflight.effective_observed_at > preflight.effective_expires_at {",
        "let issued_at = now;",
        "let authority_expires_at = preflight.effective_expires_at;",
        "drop(b3c_receipt);",
        "callback_authority_id_is_sensitive_to_every_frozen_field",
        "b3d_authority_issue_is_linear_exact_and_callback_free",
        "b3d_retryable_issue_returns_the_exact_b3c_receipt",
        "b3d_future_bar_is_retryable_but_expiry_and_missing_identity_are_terminal",
    ):
        if required not in module:
            fail(f"required B3D implementation proof missing: {required}")


def main() -> int:
    try:
        inventory = json.loads(INVENTORY.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as exc:
        fail(f"missing or invalid B3D implementation contract: {exc}")
    check_inventory(inventory)
    check_source()
    if (ROOT / ".git").exists():
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", IMPLEMENTATION_PREDECESSOR_REF, "--"],
            cwd=ROOT,
            text=True,
        ).splitlines()
        if sorted(changed) != sorted(EXPECTED_IMPLEMENTATION_CHANGED_PATHS):
            fail("B3D implementation review diff drift")
    for marker in (
        "additive private no-I/O issue implementation, pending review",
        "Accepted governance predecessor",
        "583241d441d94688d86e462ca9d066bf88dec2b9",
        "Implemented boundary",
        "issue_stage5e_callback_authority",
        "authority_expires_at = effective_expires_at",
        "Actual callback invocation and escrow implementation remain HOLD",
    ):
        if marker not in PLAN.read_text():
            fail(f"required B3D implementation plan marker missing: {marker}")
    run_predecessor()
    print("stage5e-b3d-callback-authority-design-check: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
