#!/usr/bin/env python3
"""Fail-closed authority gate for the R2 deterministic terminal-fill slice."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
RUNTIME = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
STAGE5G_B = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
BROKER_ACK_MAPPING = "crates/broker-core/src/hybrid_strategy_boundary.rs"
DESCRIPTOR = "docs/stage-5/stage5g-c-r2ca-r2-deterministic-terminal-fill-boundary.json"

BASE_COMMIT = "d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654"
BASE_STAGE5C_SHA256 = "4670090bb6046d9c70310ef07dfee2eafaa87f7873627db9de240ee5ab568d40"
BASE_RUNTIME_SHA256 = "aa514c2479a2720a585ce0c386ab91674e125582e013912fba49fe529f8bdd2d"
STAGE5C_CURRENT_SHA256 = "541b3dfffc838bd939790210c0a63e988a1c1d4a66f69bba52914a494b4cc3ea"
RUNTIME_CURRENT_SHA256 = "fda7593117c41797d2a98e534937b53ead18451e6a3c89c5196eace0207959f3"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
STAGE5G_B_SHA256 = "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67"
BROKER_ACK_MAPPING_SHA256 = "c154754d3be57bc5566ee8cfde5d2ec552dea31afc7e56a7277d4592f219157d"

REGIONS = {
    "deterministic-terminal-fill-boundary-v1": (
        STAGE5C,
        "STAGE5G-C-R2CA-R2-AUTHORITY",
        "deterministic-terminal-fill-boundary-v1",
        "943f7ac92874f3ccc91f13c5dd020806aee953221219202da24af8affa6d9b72",
    ),
    "deterministic-terminal-fill-boundary-tests-v1": (
        STAGE5C,
        "STAGE5G-C-R2CA-R2-AUTHORITY-TESTS",
        "deterministic-terminal-fill-boundary-v1",
        "2f363b466a122a8c6ec3cdd95060d2133ac4eb20476a0afc890fe454d0f47d43",
    ),
    "deterministic-terminal-fill-runtime-v1": (
        RUNTIME,
        "STAGE5G-C-R2CA-R2-RUNTIME",
        "deterministic-terminal-fill-runtime-v1",
        "c242d514fba0c878da8c09e5b3dcd2ba1293c23845ea3e05667a3e65d40240cf",
    ),
    "deterministic-terminal-fill-runtime-errors-v1": (
        RUNTIME,
        "STAGE5G-C-R2CA-R2-RUNTIME-ERROR",
        "deterministic-terminal-fill-runtime-v1",
        "44e1661f094b595d3e175089efa196c548e6916329bbf407fa7c0f53d438c9a7",
    ),
    "deterministic-terminal-fill-timer-sync-v1": (
        RUNTIME,
        "STAGE5G-C-R2CA-R2-TIMER-SYNC",
        "deterministic-terminal-fill-runtime-v1",
        "68b16a1519f766f90e4178237e430ea68df5c63e0854e2d010084144579c48ac",
    ),
    "deterministic-terminal-fill-test-clock-v1": (
        STAGE5C,
        "STAGE5G-C-R2CA-R2-TEST-CLOCK",
        "deterministic-terminal-fill-test-clock-v1",
        "eb2b6e2c9d549a9b8d6b26b9877f76ec2b0fdc071ce3dc97757cedd69f5f3efa",
    ),
}


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def file_digest(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"required file missing: {relative}")
    return digest(path.read_bytes())


def extract_region(source: str, prefix: str, tag: str) -> tuple[str, str]:
    begin = f"// {prefix}-BEGIN: {tag}"
    end = f"// {prefix}-END: {tag}"
    if source.count(begin) != 1 or source.count(end) != 1:
        raise ValueError(f"marker cardinality drift: {prefix}")
    pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
    match = re.search(pattern, source, re.S)
    if match is None:
        raise ValueError(f"malformed region: {prefix}")
    stripped, count = re.subn(pattern, "", source, count=1, flags=re.S)
    if count != 1:
        raise ValueError(f"cannot strip region once: {prefix}")
    return match.group(1), stripped


def strip_regions(source: str, definitions: list[tuple[str, str]]) -> str:
    stripped = source
    for prefix, tag in definitions:
        _, stripped = extract_region(stripped, prefix, tag)
    return stripped


def require_tokens(body: str, tokens: tuple[str, ...], label: str) -> None:
    missing = [token for token in tokens if token not in body]
    if missing:
        raise ValueError(f"{label} contract token missing: {missing[0]}")


def verify_base_git_objects(root: Path) -> None:
    if not (root / ".git").exists():
        return
    resolved = subprocess.check_output(
        ["git", "rev-parse", f"{BASE_COMMIT}^{{commit}}"], cwd=root, text=True
    ).strip()
    if resolved != BASE_COMMIT:
        raise ValueError("R1 base commit does not resolve exactly")
    for relative, expected in (
        (STAGE5C, BASE_STAGE5C_SHA256),
        (RUNTIME, BASE_RUNTIME_SHA256),
    ):
        payload = subprocess.check_output(
            ["git", "show", f"{BASE_COMMIT}:{relative}"], cwd=root
        )
        if digest(payload) != expected:
            raise ValueError(f"R1 base Git object drift: {relative}")


def check_descriptor(root: Path) -> dict[str, object]:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    expected_scalars = {
        "stage": "5G-c-R2-c-a-R2-deterministic-terminal-fill-boundary",
        "status": "review_candidate",
        "base_commit": BASE_COMMIT,
        "predecessor_review_verdict": "rejected_as_submitted",
        "predecessor_stage5c_sha256": BASE_STAGE5C_SHA256,
        "predecessor_runtime_sha256": BASE_RUNTIME_SHA256,
        "stage5c_current_sha256": STAGE5C_CURRENT_SHA256,
        "runtime_current_sha256": RUNTIME_CURRENT_SHA256,
        "decision": "source_preflight_then_transaction_candidate_then_commit",
        "bracket_grace_clock": "canonical_broker_evidence_time",
        "inside_grace_result": "ready_for_timer_with_timer_preserved",
        "after_grace_result": "generated_recovery_intent_escrow",
        "focused_source_path_tests": 10,
        "stage5g_b_source_mapping_sha256": STAGE5G_B_SHA256,
        "broker_core_ack_mapping_sha256": BROKER_ACK_MAPPING_SHA256,
        "stage5f_sha256": STAGE5F_SHA256,
    }
    for key, expected in expected_scalars.items():
        if descriptor.get(key) != expected:
            raise ValueError(f"descriptor drift: {key}")
    for key in (
        "candidate_transition_transactional",
        "source_owner_cycle_preflight",
        "timer_private_state_sync_before_escrow",
        "inherited_test_clock_fixed_and_production_unchanged",
        "normalized_public_api_unchanged",
    ):
        if descriptor.get(key) is not True:
            raise ValueError(f"descriptor invariant not true: {key}")
    policy = descriptor.get("status_fill_policy")
    if policy != {
        "rejected": "filled_qty_equals_zero",
        "canceled": "zero_lte_filled_qty_lt_order_qty",
        "expired": "zero_lte_filled_qty_lt_order_qty",
        "terminal_status_full_fill": "typed_block_preserving_original_capability",
    }:
        raise ValueError("terminal status/fill policy drift")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")
    return descriptor


def check(root: Path) -> None:
    descriptor = check_descriptor(root)
    verify_base_git_objects(root)
    expected_files = (
        (STAGE5C, STAGE5C_CURRENT_SHA256),
        (RUNTIME, RUNTIME_CURRENT_SHA256),
        (STAGE5F, STAGE5F_SHA256),
        (STAGE5G_B, STAGE5G_B_SHA256),
        (BROKER_ACK_MAPPING, BROKER_ACK_MAPPING_SHA256),
    )
    for relative, expected in expected_files:
        if file_digest(root, relative) != expected:
            raise ValueError(f"source digest drift: {relative}")

    sources = {
        STAGE5C: (root / STAGE5C).read_text(),
        RUNTIME: (root / RUNTIME).read_text(),
    }
    bodies: dict[str, str] = {}
    for name, (relative, prefix, tag, expected) in REGIONS.items():
        body, _ = extract_region(sources[relative], prefix, tag)
        if digest(body.encode()) != expected:
            raise ValueError(f"R2 region digest drift: {name}")
        if descriptor.get("regions", {}).get(name) != expected:
            raise ValueError(f"descriptor region digest drift: {name}")
        bodies[name] = body

    stripped_stage5c = strip_regions(
        sources[STAGE5C],
        [
            ("STAGE5G-C-R2CA-R2-AUTHORITY", "deterministic-terminal-fill-boundary-v1"),
            ("STAGE5G-C-R2CA-R2-AUTHORITY-TESTS", "deterministic-terminal-fill-boundary-v1"),
            ("STAGE5G-C-R2CA-R2-TEST-CLOCK", "deterministic-terminal-fill-test-clock-v1"),
        ],
    )
    if digest(stripped_stage5c.encode()) != BASE_STAGE5C_SHA256:
        raise ValueError("Stage 5C changed outside R2 regions")
    stripped_runtime = strip_regions(
        sources[RUNTIME],
        [
            ("STAGE5G-C-R2CA-R2-RUNTIME", "deterministic-terminal-fill-runtime-v1"),
            ("STAGE5G-C-R2CA-R2-RUNTIME-ERROR", "deterministic-terminal-fill-runtime-v1"),
            ("STAGE5G-C-R2CA-R2-TIMER-SYNC", "deterministic-terminal-fill-runtime-v1"),
        ],
    )
    if digest(stripped_runtime.encode()) != BASE_RUNTIME_SHA256:
        raise ValueError("runtime changed outside R2 regions")

    authority = bodies["deterministic-terminal-fill-boundary-v1"]
    runtime = bodies["deterministic-terminal-fill-runtime-v1"]
    errors = bodies["deterministic-terminal-fill-runtime-errors-v1"]
    timer = bodies["deterministic-terminal-fill-timer-sync-v1"]
    test_clock = bodies["deterministic-terminal-fill-test-clock-v1"]
    tests = bodies["deterministic-terminal-fill-boundary-tests-v1"]
    require_tokens(
        authority,
        (
            "FullFillStatusContradiction",
            "broker_core::OrderStatus::Canceled | broker_core::OrderStatus::Expired",
            "facts.filled_qty == facts.order_qty",
            "stage5g_r2ca_r2_source_payload",
            "facts.lifecycle_event_ts_utc.checked_mul(1_000)",
            "bracket_grace_active",
            "stage5g_r2ca_r2_transaction_candidate",
            "Err(reason) => return Err(stage5c_r2_block(reason, resolved))",
            "CandidateIntentPolicyMismatch",
            "CandidateStateIncoherent",
            "CandidateEscrowFailed",
        ),
        "authority",
    )
    require_tokens(
        runtime,
        (
            "stage5g_r2ca_r2_transaction_candidate",
            "stage5g_r2ca_r2_source_payload",
            "self.active_cycle_id == Some(entry.cycle_id)",
            "self.current_owner == Some(exit.owner)",
            "stage5g_r2ca_r2_bracket_reconcile_active_at",
            "stage5g_r2ca_r2_apply_partial_exit_position_at",
            "stage5g_r2ca_r2_apply_partial_entry_position_at",
            "self.sync_state()",
        ),
        "runtime",
    )
    require_tokens(errors, ("Stage5gR2caR2SourcePayload", "Stage5gR2caR2PositionApplyError"), "errors")
    require_tokens(timer, ("self.sync_state()", "Stage 5C validates/escrows generated intents"), "timer")
    require_tokens(
        test_clock,
        (
            "struct Utc;",
            "chrono::DateTime::<chrono::Utc>::from_timestamp(1_767_679_800, 0)",
            "fixed B3F parity test timestamp",
        ),
        "test clock",
    )

    for label, body in (("authority", authority), ("runtime", runtime), ("errors", errors), ("timer", timer)):
        if "Utc::now" in body:
            raise ValueError(f"wall clock reintroduced in {label}")
        if re.search(r"(?m)^pub (?:struct|enum|fn)\s", body):
            raise ValueError(f"normalized public API expanded in {label}")
        for token in ("redis", "finam", "reqwest", ".post(", ".delete(", "dispatch(", "std::fs"):
            if token.lower() in body.lower():
                raise ValueError(f"forbidden I/O/live token in {label}: {token}")
    if "Serialize" in authority or "Deserialize" in authority:
        raise ValueError("linear authority became serializable")

    required_tests = (
        "r2_source_path_zero_fill_entry_rejected_and_recovered_canceled_are_timer_ready",
        "r2_source_path_zero_fill_exit_expired_preserves_owned_position",
        "r2_source_path_partial_entry_canceled_restores_owner_cycle_and_escrows_exit",
        "r2_source_path_partial_exit_outside_grace_escrows_recovery_exit",
        "r2_partial_exit_inside_grace_is_timer_ready_then_timer_escrows_residual",
        "r2_canceled_or_expired_full_fill_blocks_entry_and_exit_for_both_ack_paths",
        "r2_blocked_full_fill_and_timestamp_preserve_corrected_retry_capability",
        "r2_source_owner_cycle_preflight_blocks_request_only_authority",
        "r2_candidate_failure_rolls_back_exact_state_and_allows_corrected_retry",
        "r2_same_state_and_evidence_are_independent_of_process_wall_clock",
    )
    require_tokens(tests, required_tests, "tests")
    require_tokens(
        tests,
        (
            "stage5d_cfg_sha256:56141846cb180b8a224a1db7e1f5188c99c28f0fab88a27ebe65fbcb9d7cf626",
            "CommandAckStatus::Submitted",
            "CommandAckStatus::Recovered",
            "CommandAckReasonCode::RecoveredByBrokerTruth",
            "apply_stage5c_semantic_bar_at",
            "resolve_stage5c_paper_timer",
        ),
        "source-path tests",
    )
    if tests.count("#[test]") != len(required_tests):
        raise ValueError("focused source-path test cardinality drift")

    ack_mapping = sources.get(BROKER_ACK_MAPPING, (root / BROKER_ACK_MAPPING).read_text())
    require_tokens(
        ack_mapping,
        (
            "CommandAckStatus::Submitted | CommandAckStatus::Recovered",
            "Some(HybridRuntimeAckStatus::Confirmed)",
        ),
        "Broker Core ACK mapping",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, KeyError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2ca-r2-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r2-authority-check: PASS")
    print(f"predecessor_commit: {BASE_COMMIT}")
    print("r2_regions: 6/6")
    print("source_path_tests: 10/10")
    print("wall_clock/public_api/io/live/r2cb: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
