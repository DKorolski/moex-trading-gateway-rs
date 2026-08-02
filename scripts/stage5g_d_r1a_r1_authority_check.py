#!/usr/bin/env python3
"""Fail-closed checker for Stage 5G-d R1-a R1 transactional admission."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

BASE_COMMIT = "0f72478123c8ddf90c5368ce0cef7867257087c3"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5G_D = "crates/strategy-runtime-core/src/stage5g_timer.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
STAGE5D = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
BROKER_CORE = "crates/broker-core/src/lib.rs"
DESCRIPTOR = "docs/stage-5/stage5g-d-r1a-r1-transactional-admission.json"
R1A_DESCRIPTOR = "docs/stage-5/stage5g-d-r1a-deterministic-bar-authority.json"
R1A_CHECKER = "scripts/stage5g_d_r1a_authority_check.py"

BASE_STAGE5C_SHA256 = "6b38e1c145593ef3ea376b1e1ee50832fb10ba79a25f05ca9370f06344f974f5"
CURRENT_STAGE5C_SHA256 = "dc7e0743165bc9995cde5e20531747275faaf6c60a53fc4e2c80a3dbd11d116d"
STAGE5G_D_SHA256 = "a300e48d5d9d8263b73fffad4885f58366ac30c11bcf93399be92570e240ba56"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
STAGE5D_SHA256 = "f790a907d6730e26e731a78ef89c58f993b39acde6ce934602e2fe603d90f083"
BROKER_CORE_SHA256 = "5d8758624f53a6b46d8903dd3f2339d5bd04f64c9c6490448167f08ac68ec8a2"
R1A_DESCRIPTOR_SHA256 = "aefe187f3cc1580c84d3a4aaec863d23f71cb6f7719ead5200563d0258b62b63"
R1A_CHECKER_SHA256 = "2b1253ff35ad3d34e7d966c470bc5aef34e12c3edb5bea801e1fadd1a525ea5e"

TAG = "complete-precallback-transactional-admission-v1"
R1_REGIONS = {
    "STAGE5G-D-R1A-R1-AUTHORITY": "2288c35e162ce4145133c88f940be790161b786587e4eb3e18f7b105c059e91b",
    "STAGE5G-D-R1A-R1-AUTHORITY-TESTS": "ac1724cee1b12b3657c77aeaf632f64533d2b0d8e1bbdd21d7641e34c0eb4599",
}
ACCEPTED_R1A_REGIONS = {
    ("STAGE5G-D-R1A-AUTHORITY", "deterministic-bar-continuation-authority-v1"):
        "d3547534d0767ea91f3e314897d907cb389a4b09a738e9ee53edb7ffe5b22e5d",
    ("STAGE5G-D-R1A-AUTHORITY-TESTS", "deterministic-bar-continuation-authority-v1"):
        "f571e27ee3be39a8ed2b8341899604f3cad5659dd302c4de8762caff2a0b5659",
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
        raise ValueError(f"malformed authority region: {prefix}")
    stripped, count = re.subn(pattern, "", source, count=1, flags=re.S)
    if count != 1:
        raise ValueError(f"cannot strip authority region: {prefix}")
    return match.group(1), stripped


def require_tokens(body: str, tokens: tuple[str, ...], label: str) -> None:
    for token in tokens:
        if token not in body:
            raise ValueError(f"{label} contract token missing: {token}")


def verify_base(root: Path) -> None:
    if not (root / ".git").exists():
        return
    resolved = subprocess.check_output(
        ["git", "rev-parse", f"{BASE_COMMIT}^{{commit}}"], cwd=root, text=True
    ).strip()
    if resolved != BASE_COMMIT:
        raise ValueError("R1-a predecessor does not resolve exactly")
    payload = subprocess.check_output(["git", "show", f"{BASE_COMMIT}:{STAGE5C}"], cwd=root)
    if digest(payload) != BASE_STAGE5C_SHA256:
        raise ValueError("R1-a predecessor Stage 5C Git object drift")


def check(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    expected = {
        "stage": "5G-d-R1-a-R1-complete-precallback-transactional-admission",
        "status": "review_candidate",
        "base_commit": BASE_COMMIT,
        "base_stage5c_sha256": BASE_STAGE5C_SHA256,
        "stage5c_current_sha256": CURRENT_STAGE5C_SHA256,
        "exact_preservation_fields": 14,
        "focused_r1_tests": 7,
        "normalized_public_api_unchanged": True,
        "stage5g_d_wrapper_changed": False,
    }
    for key, value in expected.items():
        if descriptor.get(key) != value:
            raise ValueError(f"descriptor drift: {key}")
    required_preflight = {
        "ready_settlement", "unresolved_intent_absent", "checked_bar_checkpoint",
        "cross_event_monotonicity", "instrument_binding", "tick_binding",
        "recovery_history_settled_chronology", "explicit_event_time_valid",
        "evaluation_now_gte_bar_checkpoint", "bootstrap_not_expired",
    }
    preflight = descriptor.get("preflight")
    if set(preflight or {}) != required_preflight or any(value is not True for value in preflight.values()):
        raise ValueError("transactional preflight descriptor drift")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")

    verify_base(root)
    for relative, expected_hash in (
        (STAGE5C, CURRENT_STAGE5C_SHA256), (STAGE5G_D, STAGE5G_D_SHA256),
        (STAGE5F, STAGE5F_SHA256), (STAGE5D, STAGE5D_SHA256),
        (BROKER_CORE, BROKER_CORE_SHA256), (R1A_DESCRIPTOR, R1A_DESCRIPTOR_SHA256),
        (R1A_CHECKER, R1A_CHECKER_SHA256),
    ):
        if file_digest(root, relative) != expected_hash:
            raise ValueError(f"source/predecessor digest drift: {relative}")

    source = (root / STAGE5C).read_text()
    stripped = source
    bodies: dict[str, str] = {}
    for prefix, expected_hash in R1_REGIONS.items():
        body, stripped = extract_region(stripped, prefix, TAG)
        if digest(body.encode()) != expected_hash:
            raise ValueError(f"R1 region digest drift: {prefix}")
        bodies[prefix] = body
    if digest(stripped.encode()) != BASE_STAGE5C_SHA256:
        raise ValueError("Stage 5C changed outside R1 regions")
    for (prefix, tag), expected_hash in ACCEPTED_R1A_REGIONS.items():
        body, _ = extract_region(source, prefix, tag)
        if digest(body.encode()) != expected_hash:
            raise ValueError(f"accepted R1-a region drift: {prefix}")

    authority = bodies["STAGE5G-D-R1A-R1-AUTHORITY"]
    tests = bodies["STAGE5G-D-R1A-R1-AUTHORITY-TESTS"]
    require_tokens(authority, (
        "advance_stage5c_timer_settlement_next_bar_transactional_at_checkpoint",
        "Stage5cTimerSettlementKind::ReadyForContinuation",
        "Stage5cTimerSettlementKind::GeneratedIntentBatch",
        "settled.batch.intent_count() > 0",
        "stage5gd_accepted_bar_checkpoint_ts_utc_ms(&accepted)",
        "bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms",
        "accepted.bar.instrument != *admission.target_instrument()",
        "!same_tick_size(accepted.tick_size, admission.tick_size())",
        "accepted.bar.close_time_utc <= recovery_receipt.recovered_ts().timestamp()",
        "accepted.bar.close_time_utc <= recovery_receipt.warmup_receipt().last_history_ts()",
        "accepted.bar.close_time_utc <= settled.batch.bar_close_ts()",
        "Utc.timestamp_millis_opt(explicit_now_ts_utc_ms).single()",
        "explicit_now_ts_utc_ms < bar_checkpoint_ts_utc_ms",
        "> recovery_receipt",
        "STAGE5GD_R1A_R1_DELEGATE_COUNT.with",
        "advance_stage5c_timer_settlement_next_bar_at_checkpoint(",
    ), "authority")
    if authority.count("advance_stage5c_timer_settlement_next_bar_at_checkpoint(") != 1:
        raise ValueError("destructive delegate cardinality drift")
    if "Utc::now" in authority or re.search(r"(?m)^pub (?:struct|enum|fn)\s", authority):
        raise ValueError("wall clock or public API entered R1 authority")
    for token in ("redis", "finam", "reqwest", ".post(", ".delete(", "dispatch(", "std::fs"):
        if token.lower() in authority.lower():
            raise ValueError(f"forbidden R1 authority surface: {token}")

    required_tests = (
        "stage5gd_r1a_r1_future_bar_is_retryable_and_exactly_preserved",
        "stage5gd_r1a_r1_wrong_instrument_is_retryable_and_exactly_preserved",
        "stage5gd_r1a_r1_wrong_tick_is_retryable_and_exactly_preserved",
        "stage5gd_r1a_r1_stale_bar_is_retryable_and_exactly_preserved",
        "stage5gd_r1a_r1_history_stale_bar_preserves_recovery_identity",
        "stage5gd_r1a_r1_unresolved_batch_is_retryable_and_exactly_preserved",
        "stage5gd_r1a_r1_valid_bar_delegates_exactly_once_deterministically",
    )
    require_tokens(tests, required_tests, "tests")
    require_tokens(tests, (
        "assert_eq!(stage5gd_r1a_r1_snapshot(&preserved), before)",
        "assert_eq!(stage5gd_r1a_r1_delegate_count(), 0)",
        "assert_eq!(stage5gd_r1a_r1_delegate_count(), 1)",
    ), "preservation tests")
    if tests.count("#[test]") != len(required_tests):
        raise ValueError("R1 focused test cardinality drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-d-r1a-r1-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-d-r1a-r1-authority-check: PASS")
    print("r1_regions: 2/2")
    print("focused_r1_tests: 7/7")
    print("precallback/preservation/public_api/io/live/r1b/e/f: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
