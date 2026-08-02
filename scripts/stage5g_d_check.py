#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-d."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

BASE = "d7561e6f36d01aea3d0dd67892800fbb6ac0a716"


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def validate(root: Path, *, check_git: bool = True) -> None:
    timer_path = root / "crates/strategy-runtime-core/src/stage5g_timer.rs"
    order_path = root / "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    lib_path = root / "crates/strategy-runtime-core/src/lib.rs"
    inventory_path = root / "docs/stage-5/stage5g-d-timer-continuation-inventory.json"
    contract_path = root / "docs/stage-5/stage5g-d-timer-continuation-contract.md"
    for path in (timer_path, order_path, lib_path, inventory_path, contract_path):
        require(path.is_file(), f"missing required Stage 5G-d file: {path}")

    timer = timer_path.read_text()
    order = order_path.read_text()
    lib = lib_path.read_text()
    inventory = json.loads(inventory_path.read_text())

    required_tokens = (
        "pub struct Stage5gTimerSession",
        "pub struct Stage5gTimerGeneratedIntentEscrow",
        "pub struct Stage5gTimerCheckpointEnvelope",
        "pub fn apply_stage5g_timer_checkpoint",
        "pub fn continue_stage5g_timer_with_timer",
        "pub fn continue_stage5g_timer_with_bar",
        "pub fn attach_stage5g_timer_generated_mock_ack",
        "pub fn apply_stage5g_timer_mock_ack",
        "pub fn attach_stage5g_timer_order_position_session",
        "pub fn classify_stage5g_post_checkpoint_evidence",
        "advance_stage5c_paper_loop_once",
        "advance_stage5c_timer_settlement_next_bar",
        "advance_stage5c_timer_settlement_timer",
        "into_stage5g_b_settled",
        "NonMonotonicCheckpoint",
        "input.now_ts_utc_ms <= last",
        "ConflictingDuplicateEvidence",
        "pub last_broker_truth_received_at",
        "pub last_broker_truth_received_ms",
        "pub evidence_replay_ledger",
        "pub last_total_sequence",
        "timestamp_subsec_nanos()",
    )
    for token in required_tokens:
        require(token in timer, f"required timer contract token missing: {token}")

    forbidden_tokens = (
        "std::thread",
        "thread::spawn",
        "tokio::spawn",
        "tokio::time::sleep",
        "std::thread::sleep",
        "Utc::now",
        "redis::",
        "reqwest",
        "finam_client",
        "Method::POST",
        "Method::DELETE",
        ".post(",
        ".delete(",
    )
    for token in forbidden_tokens:
        require(token not in timer, f"forbidden Stage 5G-d surface: {token}")

    require("mod stage5g_timer;" in lib, "Stage 5G-d module not sealed in lib.rs")
    require(
        "attach_stage5g_market_terminal_timer_session" in lib,
        "R3 market-terminal timer attachment is not exported",
    )
    require(
        "Stage5gReplayCheckpoint" in order
        and "replay_checkpoint: Stage5gReplayCheckpoint" in order,
        "Stage 5G-c exact replay checkpoint is not carried into convergence",
    )

    identity_start = order.index("fn evidence_identity(")
    identity_end = order.index("// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-END")
    identity_body = order[identity_start:identity_end]
    require("total_sequence" not in identity_body, "local sequence entered package identity")
    require("fingerprint" not in identity_body, "payload fingerprint entered package identity")

    require(inventory["stage"] == "5G-d", "inventory stage drift")
    require(inventory["status"] == "review_candidate", "inventory status drift")
    require(len(inventory["scenario_family"]) == 8, "timer scenario inventory must remain 8/8")
    require(len(inventory["checkpoint_fields"]) == 7, "checkpoint field inventory drift")
    for surface, opened in inventory["closed_surfaces"].items():
        require(opened is False, f"closed surface opened: {surface}")

    require(
        "validate_stage5c_market_terminal_outcome_r2" not in timer
        and "validate_stage5c_market_terminal_outcome_r1" not in timer,
        "obsolete Market-terminal authority entered Stage 5G-d",
    )

    if check_git:
        result = subprocess.run(
            ["git", "diff", "--quiet", BASE, "--", "crates/strategy-runtime-core/src/stage5c_paper_host.rs"],
            cwd=root,
            check=False,
        )
        require(result.returncode == 0, "accepted Stage 5C timer authority was modified")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), check_git=not args.skip_git)
    except (CheckFailure, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-d-check: FAIL: {error}")
        return 1
    print("stage5g-d-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
