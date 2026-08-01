#!/usr/bin/env python3
"""Fail-closed authority gate for Stage 5G-c R2-c-a R1."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
STAGE5F = "crates/strategy-runtime-core/src/stage5f_atomic_hybrid_semantics.rs"
STAGE5G_B = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
BROKER_ACK_MAPPING = "crates/broker-core/src/hybrid_strategy_boundary.rs"
DESCRIPTOR = "docs/stage-5/stage5g-c-r2ca-r1-market-terminal-state-coherence.json"
R2A_DESCRIPTOR = "docs/stage-5/stage5g-c-source-projection-extension.json"
R2A_CHECKER = "scripts/stage5g_c_r2a_authority_check.py"
INVENTORY = "docs/stage-5/stage5g-lifecycle-entry-inventory.json"

BASE_COMMIT = "581f4f6021dd781e7a5db9177be05feb7d94b12a"
BASE_STAGE5C_SHA256 = "2315b70ba14432da56b777057506e69e425295a9c1b221e08438cc9e16af3d77"
BASE_REGIONS = {
    "market-terminal-no-callback-v1": "1d98411788ec1e0b331a7377fc8efdc6074afcaac107c99ea30c8aba4e351202",
    "market-terminal-no-callback-tests-v1": "e7c7ad7cadc8e8f93c1e07acdab0cf2d4558a42da519190a9447afeba01a0606",
}
R2A_STAGE5C_SHA256 = "636dd27ac64b1d9dc448fc065497d7d59c09fc8735891953e5b0432879b60193"
R2A_CHECKER_SHA256 = "e0b94ef8efe98b430478210cc783673f6a0b10281619b00281dafbd5c161b0bb"
R2A_DESCRIPTOR_SHA256 = "33eaaf5b7de3cdb69698ef1ff434f22480fdb059d17861124179703a88b5a088"
STAGE5F_SHA256 = "cf8fe7900a2f1f84d3928c0d911db69415f19ee640c26dea47227759e375c508"
STAGE5G_B_SHA256 = "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67"
BROKER_ACK_MAPPING_SHA256 = "c154754d3be57bc5566ee8cfde5d2ec552dea31afc7e56a7277d4592f219157d"

REGIONS = {
    "market-terminal-state-coherence-v1": (
        "STAGE5G-C-R2CA-R1-AUTHORITY",
        "63c09f197264f144c21fa650e53912b6fe9086a0cc7ceb115cc1cb2b754b709b",
    ),
    "market-terminal-state-coherence-tests-v1": (
        "STAGE5G-C-R2CA-R1-AUTHORITY-TESTS",
        "2776814114e51cd377c94abed761db09eef1c692818fe890ea8911fafcdaaccf",
    ),
}


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_digest(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        raise ValueError(f"required file missing: {relative}")
    return digest(path.read_bytes())


def extract_current(source: str) -> tuple[dict[str, str], str]:
    bodies: dict[str, str] = {}
    stripped = source
    for name, (prefix, _) in REGIONS.items():
        begin = f"// {prefix}-BEGIN: market-terminal-state-coherence-v1"
        end = f"// {prefix}-END: market-terminal-state-coherence-v1"
        if source.count(begin) != 1 or source.count(end) != 1:
            raise ValueError(f"marker cardinality drift: {name}")
        pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
        match = re.search(pattern, source, re.S)
        if match is None:
            raise ValueError(f"malformed region: {name}")
        bodies[name] = match.group(1)
        stripped, count = re.subn(pattern, "", stripped, count=1, flags=re.S)
        if count != 1:
            raise ValueError(f"cannot strip region once: {name}")
    return bodies, stripped


def extract_predecessor(source: str, prefix: str) -> str:
    begin = f"// {prefix}-BEGIN: market-terminal-no-callback-v1"
    end = f"// {prefix}-END: market-terminal-no-callback-v1"
    pattern = rf"(?m)^\s*{re.escape(begin)}\n(.*?)^\s*{re.escape(end)}\n"
    match = re.search(pattern, source, re.S)
    if match is None:
        raise ValueError(f"predecessor region missing: {prefix}")
    return match.group(1)


def verify_predecessor_git_object(root: Path) -> None:
    if not (root / ".git").exists():
        return
    resolved = subprocess.check_output(
        ["git", "rev-parse", f"{BASE_COMMIT}^{{commit}}"], cwd=root, text=True
    ).strip()
    if resolved != BASE_COMMIT:
        raise ValueError("R2-c-a predecessor commit does not resolve exactly")
    predecessor = subprocess.check_output(
        ["git", "show", f"{BASE_COMMIT}:{STAGE5C}"], cwd=root
    )
    if digest(predecessor) != BASE_STAGE5C_SHA256:
        raise ValueError("R2-c-a predecessor Stage 5C bytes drift")
    text = predecessor.decode()
    for name, prefix in (
        ("market-terminal-no-callback-v1", "STAGE5G-C-R2CA-AUTHORITY"),
        (
            "market-terminal-no-callback-tests-v1",
            "STAGE5G-C-R2CA-AUTHORITY-TESTS",
        ),
    ):
        if digest(extract_predecessor(text, prefix).encode()) != BASE_REGIONS[name]:
            raise ValueError(f"R2-c-a predecessor region drift: {name}")


def run_detached_r2a(root: Path, stripped_stage5c: str) -> None:
    if file_digest(root, R2A_CHECKER) != R2A_CHECKER_SHA256:
        raise ValueError("accepted detached R2-a checker drift")
    if file_digest(root, R2A_DESCRIPTOR) != R2A_DESCRIPTOR_SHA256:
        raise ValueError("accepted detached R2-a descriptor drift")
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r1-r2a-") as tmp:
        detached = Path(tmp)
        for relative in (R2A_CHECKER, R2A_DESCRIPTOR, INVENTORY, STAGE5F):
            destination = detached / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(root / relative, destination)
        destination = detached / STAGE5C
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(stripped_stage5c)
        completed = subprocess.run(
            [sys.executable, str(detached / R2A_CHECKER), "--root", str(detached)],
            check=False,
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            raise ValueError(
                "detached accepted R2-a checker rejected stripped tree: "
                + completed.stderr.strip()
            )


def require_tokens(body: str, tokens: tuple[str, ...], label: str) -> None:
    missing = [token for token in tokens if token not in body]
    if missing:
        raise ValueError(f"{label} contract token missing: {missing[0]}")


def check(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("stage") != "5G-c-R2-c-a-R1-market-terminal-state-coherence":
        raise ValueError("descriptor stage drift")
    if descriptor.get("base_commit") != BASE_COMMIT:
        raise ValueError("R1 base commit drift")
    if descriptor.get("predecessor_stage5c_sha256") != BASE_STAGE5C_SHA256:
        raise ValueError("predecessor Stage 5C digest rewritten")
    if descriptor.get("predecessor_regions") != BASE_REGIONS:
        raise ValueError("predecessor region binding drift")
    if descriptor.get("stage5c_r2a_stripped_sha256") != R2A_STAGE5C_SHA256:
        raise ValueError("accepted R2-a Stage 5C binding drift")
    if any(descriptor.get("closed_surfaces", {}).values()):
        raise ValueError("closed surface opened")

    verify_predecessor_git_object(root)
    if file_digest(root, STAGE5F) != STAGE5F_SHA256:
        raise ValueError("frozen Stage 5F source drift")
    if file_digest(root, STAGE5G_B) != STAGE5G_B_SHA256:
        raise ValueError("accepted Stage 5G-b ACK source-path drift")
    if file_digest(root, BROKER_ACK_MAPPING) != BROKER_ACK_MAPPING_SHA256:
        raise ValueError("Broker Core ACK mapping drift")

    source = (root / STAGE5C).read_text()
    bodies, stripped = extract_current(source)
    if digest(stripped.encode()) != R2A_STAGE5C_SHA256:
        raise ValueError("Stage 5C outside approved R1 regions changed")
    if descriptor.get("stage5c_current_sha256") != file_digest(root, STAGE5C):
        raise ValueError("current Stage 5C digest mismatch")
    declared_regions = descriptor.get("regions", {})
    for name, (_, expected) in REGIONS.items():
        if digest(bodies[name].encode()) != expected:
            raise ValueError(f"approved R1 region digest drift: {name}")
        if declared_regions.get(name) != expected:
            raise ValueError(f"descriptor R1 region digest drift: {name}")

    production = bodies["market-terminal-state-coherence-v1"]
    tests = bodies["market-terminal-state-coherence-tests-v1"]
    require_tokens(
        production,
        (
            "pub(crate) struct Stage5cValidatedMarketTerminalOutcome",
            "pub(crate) fn validate_stage5c_market_terminal_outcome",
            "pub(crate) fn settle_stage5c_validated_market_terminal_outcome",
            "HybridRuntimeAckStatus::Accepted",
            "HybridRuntimeAckStatus::Confirmed",
            "on_broker_ack",
            "on_broker_position",
            "generated_intent_batch",
            "evidence_fingerprint",
            "source_ts > order.received_ts",
            "trade.source_ts > trade.received_ts",
            "position_source_ts > position.received_ts",
        ),
        "production",
    )
    if re.search(r"(?m)^pub (?:struct|enum|fn)\s", production):
        raise ValueError("normalized public API expanded")
    if "Serialize" in production or "Deserialize" in production:
        raise ValueError("validated authority became serializable")
    if "#[derive" in production:
        raise ValueError("linear validation capability gained derive authority")
    for token in (
        "redis",
        "finam",
        "reqwest",
        ".post(",
        ".delete(",
        "dispatch(",
        ".send(",
        "std::fs",
    ):
        if token.lower() in production.lower():
            raise ValueError(f"forbidden I/O/transport token in R1 authority: {token}")
    if "Strategy::set_state" in production:
        raise ValueError("direct strategy-state write opened")
    if production.count("on_broker_ack") != 1 or production.count("on_broker_position") != 1:
        raise ValueError("reviewed runtime callback cardinality drift")

    required_tests = (
        "stage5g_r2ca_zero_fill_entry_resolves_pending_for_accepted_and_confirmed_ack",
        "stage5g_r2ca_zero_fill_exit_keeps_position_and_clears_original_pending",
        "stage5g_r2ca_partial_entry_and_exit_update_position_and_retain_recovery_intent",
        "stage5g_r2ca_blocks_rejected_positive_fill_and_preserves_retry_capability",
        "stage5g_r2ca_blocks_wrong_side_quantity_and_attribution",
        "stage5g_r2ca_blocks_partial_without_position_and_duplicate_terminal_order",
        "stage5g_r2ca_validation_failure_preserves_exact_retry_capability",
        "stage5g_r2ca_rejects_non_monotonic_order_trade_and_position_chronology",
    )
    require_tokens(tests, required_tests, "test")
    if tests.count("#[test]") != len(required_tests):
        raise ValueError("focused authority test cardinality drift")

    ack_mapping = (root / BROKER_ACK_MAPPING).read_text()
    require_tokens(
        ack_mapping,
        (
            "CommandAckStatus::Submitted | CommandAckStatus::Recovered",
            "Some(HybridRuntimeAckStatus::Confirmed)",
        ),
        "Broker Core ACK mapping",
    )
    stage5g_b = (root / STAGE5G_B).read_text()
    require_tokens(
        stage5g_b,
        (
            "production_public_submitted_then_recovered_resolves_stage5c_once",
            "CommandAckStatus::Submitted",
            "CommandAckStatus::Recovered",
        ),
        "Stage 5G-b source path",
    )

    run_detached_r2a(root, stripped)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (
        ValueError,
        KeyError,
        json.JSONDecodeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"stage5g-c-r2ca-r1-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r1-authority-check: PASS")
    print(f"predecessor_commit: {BASE_COMMIT}")
    print("detached_stage5g_c_r2a: PASS")
    print("stage5c_r1_regions: 2/2")
    print("ack/position callbacks: 1/1")
    print("public_api/io/live/r2cb: closed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
