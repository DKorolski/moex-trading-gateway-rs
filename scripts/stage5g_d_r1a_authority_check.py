#!/usr/bin/env python3
"""Fail-closed authority checker for Stage 5G-d R1-a."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

BASE_COMMIT = "bc4cabfff42eafee48733296f121a8a6e2f42dd8"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5G_D = "crates/strategy-runtime-core/src/stage5g_timer.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
STAGE5D = "crates/strategy-runtime-core/src/stage5d_persistence.rs"
BROKER_CORE = "crates/broker-core/src/lib.rs"
DESCRIPTOR = "docs/stage-5/stage5g-d-r1a-deterministic-bar-authority.json"

BASE_STAGE5C_SHA256 = "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7"
CURRENT_STAGE5C_SHA256 = "6b38e1c145593ef3ea376b1e1ee50832fb10ba79a25f05ca9370f06344f974f5"
STAGE5G_D_SHA256 = "a300e48d5d9d8263b73fffad4885f58366ac30c11bcf93399be92570e240ba56"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
STAGE5D_SHA256 = "f790a907d6730e26e731a78ef89c58f993b39acde6ce934602e2fe603d90f083"
BROKER_CORE_SHA256 = "5d8758624f53a6b46d8903dd3f2339d5bd04f64c9c6490448167f08ac68ec8a2"

TAG = "deterministic-bar-continuation-authority-v1"
AUTHORITY_REGIONS = {
    "STAGE5G-D-R1A-AUTHORITY": "d3547534d0767ea91f3e314897d907cb389a4b09a738e9ee53edb7ffe5b22e5d",
    "STAGE5G-D-R1A-AUTHORITY-TESTS": "f571e27ee3be39a8ed2b8341899604f3cad5659dd302c4de8762caff2a0b5659",
}
ACCEPTED_REGIONS = {
    ("STAGE5G-C-R2CA-R1-AUTHORITY", "market-terminal-state-coherence-v1"):
        "63c09f197264f144c21fa650e53912b6fe9086a0cc7ceb115cc1cb2b754b709b",
    ("STAGE5G-C-R2CA-R2-AUTHORITY", "deterministic-terminal-fill-boundary-v1"):
        "943f7ac92874f3ccc91f13c5dd020806aee953221219202da24af8affa6d9b72",
    ("STAGE5G-C-R2CA-R3-AUTHORITY", "exact-receipt-clock-bracket-authority-v1"):
        "2d1d530690bfc821c908ce092fec294c3b6a5243cb80cd6ad400e1c3aa57e12e",
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
        raise ValueError("base commit does not resolve exactly")
    payload = subprocess.check_output(["git", "show", f"{BASE_COMMIT}:{STAGE5C}"], cwd=root)
    if digest(payload) != BASE_STAGE5C_SHA256:
        raise ValueError("base Stage 5C Git object drift")


def check(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    expected = {
        "stage": "5G-d-R1-a-deterministic-bar-continuation-authority",
        "status": "review_candidate",
        "base_commit": BASE_COMMIT,
        "base_stage5c_sha256": BASE_STAGE5C_SHA256,
        "stage5c_current_sha256": CURRENT_STAGE5C_SHA256,
        "focused_tests": 7,
        "normalized_public_api_unchanged": True,
        "stage5g_d_wrapper_changed": False,
    }
    for key, value in expected.items():
        if descriptor.get(key) != value:
            raise ValueError(f"descriptor drift: {key}")
    if descriptor.get("clock_policy") != {
        "timer_checkpoint": "explicit_stage5c_paper_timer_input_now_ms",
        "bar_checkpoint": "accepted_semantic_bar_close_seconds_checked_mul_1000",
        "bar_evaluation_now": "explicit_deterministic_event_time_ms",
        "process_wall_clock_used": False,
    }:
        raise ValueError("clock policy drift")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")

    verify_base(root)
    for relative, expected_hash in (
        (STAGE5C, CURRENT_STAGE5C_SHA256),
        (STAGE5G_D, STAGE5G_D_SHA256),
        (STAGE5F, STAGE5F_SHA256),
        (STAGE5D, STAGE5D_SHA256),
        (BROKER_CORE, BROKER_CORE_SHA256),
    ):
        if file_digest(root, relative) != expected_hash:
            raise ValueError(f"source digest drift: {relative}")

    source = (root / STAGE5C).read_text()
    stripped = source
    bodies: dict[str, str] = {}
    for prefix, expected_hash in AUTHORITY_REGIONS.items():
        body, stripped = extract_region(stripped, prefix, TAG)
        if digest(body.encode()) != expected_hash:
            raise ValueError(f"authority region digest drift: {prefix}")
        bodies[prefix] = body
    if digest(stripped.encode()) != BASE_STAGE5C_SHA256:
        raise ValueError("Stage 5C changed outside R1-a authority regions")

    for (prefix, tag), expected_hash in ACCEPTED_REGIONS.items():
        body, _ = extract_region(source, prefix, tag)
        if digest(body.encode()) != expected_hash:
            raise ValueError(f"accepted Market-terminal authority drift: {tag}")

    authority = bodies["STAGE5G-D-R1A-AUTHORITY"]
    tests = bodies["STAGE5G-D-R1A-AUTHORITY-TESTS"]
    require_tokens(authority, (
        "pub(crate) fn stage5gd_accepted_bar_checkpoint_ts_utc_ms",
        "accepted\n        .bar\n        .close_time_utc\n        .checked_mul(1_000)",
        "pub(crate) fn advance_stage5c_timer_settlement_next_bar_at_checkpoint",
        "explicit_now_ts_utc_ms",
        "bar_checkpoint_ts_utc_ms <= previous_continuation_checkpoint_ts_utc_ms",
        "return Err(stage5cm_block(reason, settlement))",
        "Utc.timestamp_millis_opt(explicit_now_ts_utc_ms).single()",
        "advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, explicit_now)",
    ), "authority")
    if authority.count("advance_stage5c_timer_settlement_next_bar_at(settlement, accepted, explicit_now)") != 1:
        raise ValueError("existing callback path delegation cardinality drift")
    if "Utc::now" in authority or re.search(r"(?m)^pub (?:struct|enum|fn)\s", authority):
        raise ValueError("wall clock or normalized public API entered authority")
    for token in ("redis", "finam", "reqwest", ".post(", ".delete(", "dispatch(", "std::fs"):
        if token.lower() in authority.lower():
            raise ValueError(f"forbidden authority surface: {token}")

    required_tests = (
        "stage5gd_r1a_reversed_bar_blocks_before_callback_and_preserves_settlement",
        "stage5gd_r1a_equal_bar_and_timer_checkpoint_blocks_before_callback",
        "stage5gd_r1a_later_bar_invokes_one_existing_stage5c_callback",
        "stage5gd_r1a_explicit_clock_is_reproducible_and_process_clock_independent",
        "stage5gd_r1a_explicit_now_after_expiry_is_retryable",
        "stage5gd_r1a_bar_checkpoint_overflow_is_retryable_before_callback",
        "stage5gd_r1a_generated_bar_intents_remain_in_stage5c_settled_batch",
    )
    require_tokens(tests, required_tests, "tests")
    if tests.count("#[test]") != len(required_tests):
        raise ValueError("focused test cardinality drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-d-r1a-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-d-r1a-authority-check: PASS")
    print("authority_regions: 2/2")
    print("focused_tests: 7/7")
    print("public_api/io/live/r1b/e/f: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
