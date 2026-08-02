#!/usr/bin/env python3
"""Fail-closed authority gate for the R3 exact receipt-clock slice."""

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
DESCRIPTOR = "docs/stage-5/stage5g-c-r2ca-r3-exact-receipt-clock-bracket-authority.json"

BASE_COMMIT = "3d995af48e88588909e11505fdefc826ff8f66ce"
BASE_STAGE5C_SHA256 = "541b3dfffc838bd939790210c0a63e988a1c1d4a66f69bba52914a494b4cc3ea"
BASE_RUNTIME_SHA256 = "fda7593117c41797d2a98e534937b53ead18451e6a3c89c5196eace0207959f3"
STAGE5C_CURRENT_SHA256 = "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
STAGE5G_B_SHA256 = "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67"
BROKER_ACK_MAPPING_SHA256 = "c154754d3be57bc5566ee8cfde5d2ec552dea31afc7e56a7277d4592f219157d"

REGIONS = {
    "exact-receipt-clock-bracket-authority-v1": (
        "STAGE5G-C-R2CA-R3-AUTHORITY",
        "2d1d530690bfc821c908ce092fec294c3b6a5243cb80cd6ad400e1c3aa57e12e",
    ),
    "exact-receipt-clock-bracket-authority-tests-v1": (
        "STAGE5G-C-R2CA-R3-AUTHORITY-TESTS",
        "66da05b4c9074b5919add656b23ebd7925cd6f882554df05dc552d5ec28f679d",
    ),
}
TAG = "exact-receipt-clock-bracket-authority-v1"


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def file_digest(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"required file missing: {relative}")
    return digest(path.read_bytes())


def extract_region(source: str, prefix: str) -> tuple[str, str]:
    begin = f"// {prefix}-BEGIN: {TAG}"
    end = f"// {prefix}-END: {TAG}"
    if source.count(begin) != 1 or source.count(end) != 1:
        raise ValueError(f"marker cardinality drift: {prefix}")
    pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
    match = re.search(pattern, source, re.S)
    if match is None:
        raise ValueError(f"malformed R3 region: {prefix}")
    stripped, count = re.subn(pattern, "", source, count=1, flags=re.S)
    if count != 1:
        raise ValueError(f"cannot strip R3 region once: {prefix}")
    return match.group(1), stripped


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
        raise ValueError("R2 base commit does not resolve exactly")
    for relative, expected in ((STAGE5C, BASE_STAGE5C_SHA256), (RUNTIME, BASE_RUNTIME_SHA256)):
        payload = subprocess.check_output(["git", "show", f"{BASE_COMMIT}:{relative}"], cwd=root)
        if digest(payload) != expected:
            raise ValueError(f"R2 base Git object drift: {relative}")


def check(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    expected = {
        "stage": "5G-c-R2-c-a-R3-exact-receipt-clock-bracket-authority",
        "status": "review_candidate",
        "base_commit": BASE_COMMIT,
        "predecessor_review_verdict": "rejected_as_submitted",
        "predecessor_stage5c_sha256": BASE_STAGE5C_SHA256,
        "predecessor_runtime_sha256": BASE_RUNTIME_SHA256,
        "stage5c_current_sha256": STAGE5C_CURRENT_SHA256,
        "runtime_current_sha256": BASE_RUNTIME_SHA256,
        "focused_source_path_tests": 6,
    }
    for key, value in expected.items():
        if descriptor.get(key) != value:
            raise ValueError(f"descriptor drift: {key}")
    if descriptor.get("clock_domain") != {
        "bracket_timer_origin": "local_runtime_receipt_clock_milliseconds",
        "terminal_decision": "broker_truth_package_receipt_clock_milliseconds",
        "component_source_timestamps": "economic_identity_and_chronology_only",
        "exact_receipt_expression": "evidence.truth.received_ts.timestamp_millis()",
        "second_level_derivation_forbidden": True,
    }:
        raise ValueError("descriptor clock-domain drift")
    if descriptor.get("chronology") != {
        "ack_processed_lte_truth_receipt": True,
        "component_source_lte_component_receipt_lte_truth_receipt": True,
        "partial_exit_truth_receipt_gte_bracket_start": True,
        "ack_seconds_to_milliseconds_checked": True,
    }:
        raise ValueError("descriptor chronology drift")
    for key in (
        "same_second_subsecond_witness",
        "delayed_receipt_witness",
        "fresh_snapshot_retry_witness",
        "full_fill_and_rollback_inherited",
        "normalized_public_api_unchanged",
    ):
        if descriptor.get(key) is not True:
            raise ValueError(f"descriptor invariant not true: {key}")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")

    verify_base_git_objects(root)
    for relative, expected_hash in (
        (STAGE5C, STAGE5C_CURRENT_SHA256),
        (RUNTIME, BASE_RUNTIME_SHA256),
        (STAGE5F, STAGE5F_SHA256),
        (STAGE5G_B, STAGE5G_B_SHA256),
        (BROKER_ACK_MAPPING, BROKER_ACK_MAPPING_SHA256),
    ):
        if file_digest(root, relative) != expected_hash:
            raise ValueError(f"source digest drift: {relative}")

    source = (root / STAGE5C).read_text()
    bodies: dict[str, str] = {}
    stripped = source
    for name, (prefix, expected_hash) in REGIONS.items():
        body, stripped = extract_region(stripped, prefix)
        if digest(body.encode()) != expected_hash:
            raise ValueError(f"R3 region digest drift: {name}")
        if descriptor.get("regions", {}).get(name) != expected_hash:
            raise ValueError(f"descriptor region digest drift: {name}")
        bodies[name] = body
    if digest(stripped.encode()) != BASE_STAGE5C_SHA256:
        raise ValueError("Stage 5C changed outside R3 regions")

    authority = bodies["exact-receipt-clock-bracket-authority-v1"]
    tests = bodies["exact-receipt-clock-bracket-authority-tests-v1"]
    require_tokens(
        authority,
        (
            "evidence.truth.received_ts.timestamp_millis()",
            "ack.processed_ts_utc.checked_mul(1_000)",
            "evidence_received_ms < ack_processed_ms",
            "evidence_received_ms < started",
            "stage5g_r2ca_r2_bracket_reconcile_active_at(evidence_received_ms)",
            "Stage5cValidatedMarketTerminalOutcomeR2",
            "evidence_now_ms: evidence_received_ms",
            "settle_stage5c_validated_market_terminal_outcome_r2(validated.validated_r2)",
        ),
        "authority",
    )
    if "facts.lifecycle_event_ts_utc.checked_mul(1_000)" in authority:
        raise ValueError("component source seconds rebound as R3 decision clock")
    if "Utc::now" in authority or re.search(r"\.timestamp\(\)", authority):
        raise ValueError("R3 authority lost exact receipt-clock precision")
    if re.search(r"(?m)^pub (?:struct|enum|fn)\s", authority):
        raise ValueError("normalized public API expanded")
    for token in ("redis", "finam", "reqwest", ".post(", ".delete(", "dispatch(", "std::fs"):
        if token.lower() in authority.lower():
            raise ValueError(f"forbidden I/O/live token in R3 authority: {token}")

    required_tests = (
        "r3_same_second_post_start_receipt_uses_inside_grace_policy",
        "r3_pre_timer_receipt_blocks_and_preserves_capability",
        "r3_delayed_receipt_after_grace_escrows_recovery_immediately",
        "r3_fresh_snapshot_same_source_later_receipt_unblocks_retry",
        "r3_exact_state_and_evidence_are_process_clock_independent",
        "r3_inherits_full_fill_contradiction_and_transaction_rollback",
    )
    require_tokens(tests, required_tests, "tests")
    require_tokens(
        tests,
        (
            "Duration::milliseconds(900)",
            "Duration::milliseconds(950)",
            "Duration::milliseconds(850)",
            "Duration::seconds(4)",
            "source-reachable Stage 5F R3 Exit callback",
            "attach_stage5g_mock_ack_session",
            "CommandAckStatus::Accepted",
        ),
        "source-path tests",
    )
    if tests.count("#[test]") != len(required_tests):
        raise ValueError("focused R3 test cardinality drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, KeyError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2ca-r3-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r3-authority-check: PASS")
    print(f"predecessor_commit: {BASE_COMMIT}")
    print("r3_regions: 2/2")
    print("source_path_tests: 6/6")
    print("receipt_ms/public_api/io/live/r2cb: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
