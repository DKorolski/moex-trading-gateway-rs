#!/usr/bin/env python3
"""Fail-closed Stage 5G-b R1 lifecycle-evidence checker."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_b_mock_ack_check as base


MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CONTRACT = "docs/stage-5/stage5g-b-r1-contract.json"
DESIGN = "docs/stage-5/5g-b-r1-lifecycle-evidence-hardening.md"
DESCRIPTOR = "docs/stage-5/stage5g-a-acceptance-descriptor.json"
SNAPSHOT_GATE = "scripts/stage5g_a_snapshot_gate.sh"
STATUS = "docs/current-status.md"

STAGE5G_A_REF = "011fd4b7baaa41fffdad7d3c28e463b7977f5989"
BASE_REF = "b6f4194769ce0f6c00a82361eba57dc3ed07e55c"
DESCRIPTOR_SHA256 = "5cc7a3a02ed3dc553824f5e75df54f911b484fd890a9a09f884844b5806af9b1"
GOLDEN_SHA256 = "f03a86a0f9f9e6c64b2a3c6bdabb4a3af86eac5674e75859ad8e13f4cf491308"

SLOT_FIELDS = [
    "request_id",
    "expected_client_order_id",
    "intent_class",
    "action",
    "side",
    "source_event_ts_utc",
    "state",
    "latest_status",
    "latest_reason_code",
    "latest_received_ts_utc",
    "canonical_total_sequence",
    "pending_disposition",
    "status_policy",
    "broker_order_id_domain_sha256",
]

CLOSED_SURFACES = {
    "redis_live_consumer",
    "redis_consumer_groups",
    "finam_transport",
    "http_post_delete",
    "broker_dispatch_execution",
    "order_trade_position_events",
    "runtime_live",
    "real_orders",
    "stage5g_c",
    "stage6",
}

WALL_CLOCK_PATTERNS = [
    r"\bUtc\s*::\s*now\s*\(",
    r"\bLocal\s*::\s*now\s*\(",
    r"\bSystemTime\s*::\s*now\s*\(",
    r"\bInstant\s*::\s*now\s*\(",
]


def fail(message: str) -> None:
    raise ValueError(message)


def require_file(root: Path, relative: str) -> Path:
    path = root / relative
    if not path.is_file():
        fail(f"required R1 file missing: {relative}")
    return path


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read valid JSON {path}: {error}")
    if not isinstance(value, dict):
        fail(f"JSON root must be an object: {path}")
    return value


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract_braced_item(source: str, pattern: str, label: str) -> tuple[str, str]:
    stripped = base.strip_rust_comments_and_literals(source)
    match = re.search(pattern, stripped)
    if not match:
        fail(f"Rust item missing: {label}")
    opening = stripped.find("{", match.end())
    if opening < 0:
        fail(f"Rust item body missing: {label}")
    depth = 0
    for index in range(opening, len(stripped)):
        token = stripped[index]
        if token == "{":
            depth += 1
        elif token == "}":
            depth -= 1
            if depth == 0:
                return source[match.start() : index + 1], stripped[match.start() : index + 1]
    fail(f"unterminated Rust item: {label}")


def function(source: str, name: str) -> tuple[str, str]:
    return extract_braced_item(
        source,
        rf"\bfn\s+{re.escape(name)}(?:\s*<[^{{;]*>)?\s*\(",
        f"fn {name}",
    )


def structure(source: str, name: str) -> tuple[str, str]:
    return extract_braced_item(
        source,
        rf"\bstruct\s+{re.escape(name)}\s*",
        f"struct {name}",
    )


def require_tokens(body: str, tokens: list[str], label: str) -> None:
    for token in tokens:
        if token not in body:
            fail(f"{label} token missing: {token}")


def check_contract(root: Path) -> None:
    contract = load_json(require_file(root, CONTRACT))
    if contract.get("schema_version") != 2:
        fail("R1 contract schema must be integer 2")
    if contract.get("stage") != "5G-b-r1-lifecycle-evidence-hardening":
        fail("R1 stage identity drift")
    if contract.get("status") != "implementation_review_candidate":
        fail("R1 status drift")
    if contract.get("base", {}).get("full_commit_sha") != BASE_REF:
        fail("R1 immutable base drift")
    authority = contract.get("stage5g_a_authority", {})
    if authority != {
        "descriptor": DESCRIPTOR,
        "descriptor_sha256": DESCRIPTOR_SHA256,
        "exact_ref": STAGE5G_A_REF,
        "snapshot_gate": SNAPSHOT_GATE,
        "entry_cases": 54,
        "negative_cases": 30,
    }:
        fail("Stage 5G-a authority binding drift")
    fingerprint = contract.get("fingerprint_schema", {})
    if fingerprint.get("version") != 2:
        fail("fingerprint schema version drift")
    if fingerprint.get("required_slot_fields") != SLOT_FIELDS:
        fail("fingerprint slot projection drift")
    if fingerprint.get("raw_broker_order_id_exported") is not False:
        fail("raw broker order ID export opened")
    if fingerprint.get("transition_binds_ordered_canonical_ack_projection") is not True:
        fail("ordered canonical ACK projection unbound")
    if fingerprint.get("golden_stage5f_market_evidence_sha256") != GOLDEN_SHA256:
        fail("accepted Stage 5F evidence golden drift")
    source = contract.get("source_authority", {})
    if source.get("admitted_actions") != ["market"]:
        fail("R1 action authority is not market-only")
    if source.get("limit_public_admission") != "NotYetSourceAuthenticated":
        fail("Limit admission unexpectedly opened")
    if source.get("cancel_public_admission") != "NotYetSourceAuthenticated":
        fail("Cancel admission unexpectedly opened")
    if source.get("trusted_stage5c_action_projection_added") is not False:
        fail("unreviewed Stage 5C action projection claimed")
    deterministic = contract.get("deterministic_fixture", {})
    if deterministic.get("bar_close_ts_utc") != 1767679800:
        fail("fixed fixture timestamp drift")
    if deterministic.get("wall_clock_read_in_stage5g_focused_tests") is not False:
        fail("wall-clock focused fixture enabled")
    if deterministic.get("debug_release_golden_equal") is not True:
        fail("debug/release evidence equality removed")
    surfaces = contract.get("closed_surfaces")
    if not isinstance(surfaces, dict) or set(surfaces) != CLOSED_SURFACES:
        fail("R1 closed-surface inventory drift")
    if any(value is not False for value in surfaces.values()):
        fail("R1 closed surface opened")
    transition = contract.get("next_transition", {})
    if transition != {
        "independent_review_required": True,
        "stage5g_c_open": False,
        "main_merge_authorized": False,
        "deployment_authorized": False,
    }:
        fail("R1 next-transition authority drift")


def check_stage5g_a(root: Path) -> None:
    descriptor_path = require_file(root, DESCRIPTOR)
    if sha256(descriptor_path) != DESCRIPTOR_SHA256:
        fail("Stage 5G-a acceptance descriptor content drift")
    descriptor = load_json(descriptor_path)
    if descriptor.get("status") != "accepted_design":
        fail("Stage 5G-a descriptor acceptance drift")
    if descriptor.get("source", {}).get("full_commit_sha") != STAGE5G_A_REF:
        fail("Stage 5G-a descriptor source drift")
    if descriptor.get("independent_review", {}).get("verdict") != "ACCEPTED_DESIGN":
        fail("Stage 5G-a independent verdict drift")
    snapshot = descriptor.get("detached_snapshot_gate", {})
    if snapshot.get("exact_ref") != STAGE5G_A_REF:
        fail("Stage 5G-a detached snapshot ref drift")
    if snapshot.get("entry_case_count") != 54 or snapshot.get("negative_case_count") != 30:
        fail("Stage 5G-a detached matrix count drift")
    script = require_file(root, SNAPSHOT_GATE).read_text(encoding="utf-8")
    require_tokens(
        script,
        [
            STAGE5G_A_REF,
            "git clone --quiet --shared --no-checkout",
            "checkout --quiet --detach",
            "scripts/stage5g_entry_plan_check.py",
            "scripts/stage5g_entry_plan_negative_harness.py",
            "entry_cases=54",
            "negative_cases=30/30",
        ],
        "Stage 5G-a snapshot gate",
    )


def check_source(root: Path) -> None:
    source = require_file(root, MODULE).read_text(encoding="utf-8")
    production, separator, tests = source.partition("#[cfg(test)]")
    if not separator:
        fail("Stage 5G-b focused test module missing")
    if "pub const STAGE5G_MOCK_ACK_SCHEMA_VERSION: u16 = 2;" not in production:
        fail("Stage 5G-b schema v2 constant drift")

    summary_raw, _ = structure(production, "Stage5gMockAckSlotSummary")
    for field in SLOT_FIELDS:
        if not re.search(rf"\bpub\s+{re.escape(field)}\s*:", summary_raw):
            fail(f"slot summary field missing: {field}")
    if re.search(r"\bpub\s+broker_order_id\s*:", summary_raw):
        fail("raw broker order ID was exported")

    summary_fp_raw, summary_fp_tokens = function(production, "stage5g_summary_fingerprint")
    require_tokens(
        summary_fp_raw,
        [
            "moex.stage5g.mock-ack-lifecycle.v2\\0",
            "serde_json::to_vec(value)",
        ],
        "lifecycle fingerprint",
    )
    require_tokens(summary_fp_tokens, ["Sha256::new", "hasher.update"], "lifecycle fingerprint")
    if "constant-fingerprint" in summary_fp_raw or "let _ = value" in summary_fp_tokens:
        fail("constant lifecycle fingerprint bypass detected")

    broker_fp_raw, broker_fp_tokens = function(
        production, "stage5g_broker_order_id_domain_sha256"
    )
    require_tokens(
        broker_fp_raw,
        [
            "moex.stage5g.broker-order-id.v1\\0",
            "order_id.as_str().as_bytes()",
        ],
        "broker order ID fingerprint",
    )
    require_tokens(broker_fp_tokens, ["Sha256::new", "hasher.update"], "broker order ID fingerprint")

    projection_raw, _ = function(
        production, "stage5g_canonical_ack_fingerprint_projection"
    )
    require_tokens(
        projection_raw,
        [
            "request_id: ack.request_id",
            "reason_code: ack.reason.as_ref().map(|reason| reason.code)",
            "received_ts_utc: stage5g_ack_timestamp(&ack.received_ts)",
            "canonical_sequence",
            "pending_disposition: decision.pending_disposition",
            "status_policy: decision.status_policy",
            ".map(stage5g_broker_order_id_domain_sha256)",
        ],
        "canonical ACK fingerprint projection",
    )
    transition_raw, _ = function(production, "stage5g_transition_fingerprint")
    require_tokens(
        transition_raw,
        [
            "ordered_canonical_ack_projection",
            ".map(stage5g_canonical_ack_fingerprint_projection)",
            "pre_callback_lifecycle_fingerprint_sha256",
            "post_lifecycle_state_fingerprint",
            "last_total_sequence: state",
            "duplicate_status_count",
        ],
        "transition fingerprint",
    )

    no_send_raw, no_send_tokens = function(
        production, "stage5g_no_send_proof_contradicts_broker_identity"
    )
    require_tokens(
        no_send_raw,
        [
            "ack.status == CommandAckStatus::Expired",
            "slot.observed_broker_order_id.is_some() || ack.broker_order_id.is_some()",
        ],
        "no-send contradiction predicate",
    )
    if "&& ack.broker_order_id.is_some()" in no_send_tokens:
        fail("no-send contradiction was weakened to require both IDs")
    apply_raw, _ = function(production, "stage5g_apply_mock_ack_state")
    contradiction_at = apply_raw.find("stage5g_no_send_proof_contradicts_broker_identity")
    disposition_at = apply_raw.find("stage5g_event_disposition")
    if contradiction_at < 0 or disposition_at < 0 or contradiction_at > disposition_at:
        fail("no-send contradiction must precede Broker Core disposition")
    if re.search(
        r"\b(?:false\s*&&|true\s*\|\|)\s*stage5g_no_send_proof_contradicts_broker_identity",
        apply_raw,
    ):
        fail("no-send contradiction predicate was constant-gated")
    require_tokens(
        apply_raw,
        [
            "Stage5gMockAckSlotState::ManualInterventionRequired",
            "Stage5gMockAckError::NoSendProofContradictsBrokerIdentity",
            "slot.observed_broker_order_id = Some(broker_order_id.clone())",
            "slot.latest_decision = None",
        ],
        "no-send retained state",
    )

    duplicate_raw, _ = function(production, "stage5g_duplicate_matches_prior")
    require_tokens(
        duplicate_raw,
        [
            "ack.request_id == prior.request_id",
            "ack.client_order_id == prior.client_order_id",
            "== Some(CommandAckReasonCode::DuplicateCommand)",
            "(Some(incoming), Some(expected)) => incoming == expected",
            "(None, None) => true",
            "_ => false",
        ],
        "exact duplicate predicate",
    )
    if re.search(r"\(None\s*,\s*_\)\s*=>\s*true", duplicate_raw):
        fail("missing broker ID can match an exact prior ID")

    admission_raw, _ = function(production, "stage5g_build_mock_ack_state")
    require_tokens(
        admission_raw,
        [
            "Stage5gMockPlaceKind::Market",
            "Stage5gMockAckAdmissionError::NotYetSourceAuthenticated",
            "input.lifecycle_expires_at_ts_utc < max_source_event_ts",
        ],
        "market-only source admission",
    )
    non_market_at = admission_raw.find("if !matches!")
    class_at = admission_raw.find("stage5g_action_matches_class")
    if non_market_at < 0 or class_at < 0 or non_market_at > class_at:
        fail("market-only source authority must precede caller binding checks")

    reason_raw, _ = function(production, "stage5g_ack_reason_is_coherent")
    for token in [
        "CommandAckStatus::Duplicate",
        "CommandAckReasonCode::DuplicateCommand",
        "CommandAckStatus::Expired",
        "CommandAckReasonCode::ExpiredCommand",
    ]:
        if token not in reason_raw:
            fail(f"ACK status/reason coherence drift: {token}")

    require_tokens(
        production,
        [
            "struct Stage5gMockAckState",
            "settled: Stage5cSettledPaperStrategy",
            "state: Stage5gMockAckState",
            "stage5g_apply_mock_ack_state(state, event)",
            "stage5g_resolve_complete_session",
        ],
        "deterministic state / linear production ownership",
    )
    for pattern in WALL_CLOCK_PATTERNS:
        if re.search(pattern, tests):
            fail(f"wall-clock read in Stage 5G focused tests: {pattern}")
    require_tokens(
        tests,
        [
            "const ACCEPTED_STAGE5F_BAR_CLOSE_TS: i64 = 1_767_679_800",
            "ImoexfPrimaryRiskgateHigh180Lb120",
            "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626",
            GOLDEN_SHA256,
            "lifecycle_fingerprint_v2_binds_exact_redacted_ack_identity",
            "lifecycle_fingerprint_v2_binds_reason_timestamp_and_sequence",
            "no_send_proof_cannot_follow_observed_broker_identity",
            "duplicate_requires_exact_broker_identity_and_coherent_reason",
            "cancel_binding_is_exact_and_carries_no_side_but_is_not_source_authenticated",
        ],
        "deterministic R1 test evidence",
    )


def check_docs(root: Path) -> None:
    design = require_file(root, DESIGN).read_text(encoding="utf-8")
    require_tokens(
        design,
        [
            BASE_REF,
            STAGE5G_A_REF,
            GOLDEN_SHA256,
            "Stage 5G-c remains blocked",
            "NotYetSourceAuthenticated",
            "NoSendProofContradictsBrokerIdentity",
        ],
        "R1 design",
    )
    status = require_file(root, STATUS).read_text(encoding="utf-8")
    for marker in [
        "Stage 5G-b is an implementation review candidate",
        "Stage 5G-b R1 is an implementation review candidate",
        "Stage 5G-c remains blocked",
    ]:
        if marker not in status:
            fail(f"current status marker missing: {marker}")


def check(root: Path) -> None:
    base.check(root)
    check_contract(root)
    check_stage5g_a(root)
    check_source(root)
    check_docs(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except ValueError as error:
        print(f"stage5g-b-r1-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-b-r1-check: PASS")
    print("fingerprint_schema: v2 exact ACK projection")
    print("no_send_contradiction: fail_closed")
    print("duplicate_identity: exact")
    print("source_authority: market_only")
    print("deterministic_fixture: debug_release_golden_bound")
    print("stage5g_a_snapshot: 54 + 30/30")
    print("closed_surfaces: preserved")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
