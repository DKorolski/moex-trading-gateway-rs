#!/usr/bin/env python3
"""Fail-closed authority check for historical full-snapshot trade replay."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

BASE = "049f39e8e50b32bc9d334cb09f8e6502988304c5"
ORDER_POSITION = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
STAGE5G_B = Path("crates/strategy-runtime-core/src/stage5g_mock_ack.rs")
FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
FINAM_FIXTURE = Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json")
GOLDEN_FIXTURE = Path("fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-r1-historical-trade-replay-chronology.json")
ADR = Path("docs/adr/adr-stage5g-c-r2cb-r1-historical-trade-replay-chronology.md")
R3_SNAPSHOT = Path("scripts/stage5g_c_r2ca_r3_snapshot_gate.py")

IMMUTABLE = {
    STAGE5C: "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7",
    STAGE5G_B: "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67",
    R3_SNAPSHOT: "2f73a9882e3efa9a091079a59741bb068a8a1d5820e81d31ae30246702975315",
}
ALLOWED_CHANGED_PATHS = {
    str(ORDER_POSITION),
    str(FINAM_MAPPER),
    str(FINAM_FIXTURE),
    str(GOLDEN_FIXTURE),
    str(DESCRIPTOR),
    str(ADR),
    "scripts/stage5g_c_r2cb_r1_authority_check.py",
    "scripts/stage5g_c_r2cb_r1_negative_harness.py",
    "scripts/stage5g_c_r2cb_r1_gate.sh",
    "scripts/stage5g_c_r2cb_r1_handoff_safety_check.py",
    "scripts/make_stage5g_c_r2cb_r1_handoff_archive.py",
}
FROZEN_PREFIXES = (
    "crates/broker-core/",
    "crates/strategy-runtime-core/src/stage5c_",
    "crates/strategy-runtime-core/src/stage5d_",
    "crates/strategy-runtime-core/src/stage5f_",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(source: str, tokens: tuple[str, ...], label: str) -> None:
    for token in tokens:
        if token not in source:
            raise ValueError(f"{label} token missing: {token}")


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check_git_scope(root: Path) -> None:
    if not (root / ".git").exists():
        return
    if git(root, "rev-parse", f"{BASE}^{{commit}}") != BASE:
        raise ValueError("R1 base commit does not resolve exactly")
    head = git(root, "rev-parse", "HEAD")
    if head != BASE and git(root, "rev-parse", "HEAD^") != BASE:
        raise ValueError("R1 is not exactly one successor to 049f39e")
    changed = set(
        filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines())
    )
    unexpected = changed - ALLOWED_CHANGED_PATHS
    if unexpected:
        raise ValueError(f"R1 changed-path scope drift: {sorted(unexpected)}")
    frozen = sorted(path for path in changed if path.startswith(FROZEN_PREFIXES))
    if frozen:
        raise ValueError(f"frozen Stage 5C/5D/5F/Broker Core surface changed: {frozen}")


def check(root: Path) -> None:
    for relative, expected in IMMUTABLE.items():
        path = root / relative
        if not path.is_file() or sha256(path) != expected:
            raise ValueError(f"accepted authority drift: {relative}")
    check_git_scope(root)

    r3 = subprocess.run(
        [sys.executable, str(root / R3_SNAPSHOT), "--root", str(root)],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if r3.returncode != 0:
        raise ValueError(f"accepted R3 snapshot rejected tree: {r3.stderr.strip()}")

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("base_commit") != BASE:
        raise ValueError("descriptor base commit drift")
    if descriptor.get("required_parent_relation") != "exactly_one_successor":
        raise ValueError("descriptor parent relation drift")
    if descriptor.get("accepted_r3_entry_points") != [
        "validate_stage5c_market_terminal_outcome_r3",
        "settle_stage5c_validated_market_terminal_outcome_r3",
    ]:
        raise ValueError("accepted R3 entry points drift")
    chronology = descriptor.get("chronology_contract", {})
    required_true = (
        "known_trade_classified_before_global_watermark",
        "known_trade_requires_immutable_payload_match",
        "known_trade_source_timestamp_exact",
        "known_trade_received_timestamp_monotonic",
        "known_trade_bypasses_global_source_watermark",
        "known_trade_quantity_counted_once",
        "new_trade_uses_global_source_and_receipt_watermarks",
    )
    if any(chronology.get(key) is not True for key in required_true):
        raise ValueError("historical trade chronology contract drift")
    if chronology.get("unseen_late_trade_policy") != "fail_closed_trade_time_regression":
        raise ValueError("unseen late-trade policy drift")
    replay = descriptor.get("replay_identity_carry_forward", {})
    if replay.get("live_stream_uniqueness_assumed") is not False:
        raise ValueError("unsafe live-stream receipt identity assumption")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")

    source = (root / ORDER_POSITION).read_text()
    require(
        source,
        (
            "STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-BEGIN",
            "find(|known| known.broker_trade_id == trade.broker_trade_id)",
            "if !immutable_trade_payload_matches(known, trade)",
            "return Err(Stage5gOrderPositionError::TradeIdentityConflict);",
            "if trade.received_ts < known.received_ts",
            "return Err(Stage5gOrderPositionError::TradeTimeRegression);",
            "continue;",
            "A previously unseen late trade remains fail closed",
            "trade.source_ts < last",
            "r2cb_public_runtime_three_poll_golden_converges_through_stage5c",
            "r2cb_three_poll_full_snapshot_replay_refreshes_history_without_regression",
            "r2cb_known_trade_refresh_and_unseen_late_trade_have_distinct_chronology",
            "stage5g_r2cb_three_poll_broker_truth.json",
            "validate_stage5c_market_terminal_outcome_r3",
            "settle_stage5c_validated_market_terminal_outcome_r3",
        ),
        "order-position",
    )
    chronology_body = source.split(
        "// STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-BEGIN", 1
    )[1].split("// STAGE5G-C-R2CB-R1-KNOWN-TRADE-CHRONOLOGY-END", 1)[0]
    if "known == *trade" in chronology_body or "last_trade_source_ts" in chronology_body:
        raise ValueError("known historical trade still depends on full equality/global source watermark")
    if chronology_body.count("continue;") != 1:
        raise ValueError("known historical trade can be double-counted")
    if (
        "validate_stage5c_market_terminal_outcome_r1" in source
        or "validate_stage5c_market_terminal_outcome_r2" in source
        or "settle_stage5c_validated_market_terminal_outcome_r1" in source
        or "settle_stage5c_validated_market_terminal_outcome_r2" in source
    ):
        raise ValueError("accepted R3 terminal authority bypass reachable")
    immutable_body = source.split("fn immutable_trade_payload_matches", 1)[1].split(
        "pub fn apply_stage5g_order_position_evidence", 1
    )[0]
    for token in ("left.price == right.price", "left.qty == right.qty", "left.source_ts == right.source_ts"):
        if token not in immutable_body:
            raise ValueError(f"immutable trade identity weakened: {token}")
    if "received_ts" in immutable_body:
        raise ValueError("observation receipt leaked into immutable trade identity")

    mapper = (root / FINAM_MAPPER).read_text()
    require(
        mapper,
        (
            "stage5g_r2cb_full_snapshot_sequence.json",
            "stage5g_r2cb_three_poll_broker_truth.json",
            'map_snapshot("poll1", "poll1_received_ts")',
            'map_snapshot("poll2", "poll2_received_ts")',
            'map_snapshot("poll3", "poll3_received_ts")',
        ),
        "FINAM mapper",
    )
    native = json.loads((root / FINAM_FIXTURE).read_text())
    if native.get("fixture_kind") != "synthetic_finam_stage5g_r2cb_three_poll_full_snapshot_sequence_v2":
        raise ValueError("native three-poll fixture kind drift")
    if [len(native[f"poll{i}"]["trades"]["trades"]) for i in (1, 2, 3)] != [1, 2, 3]:
        raise ValueError("native fixture is not A -> A+B -> A+B+C")
    trade_a = [native[f"poll{i}"]["trades"]["trades"][0] for i in (1, 2, 3)]
    trade_b = [native[f"poll{i}"]["trades"]["trades"][1] for i in (2, 3)]
    if trade_a[0] != trade_a[1] or trade_a[1] != trade_a[2] or trade_b[0] != trade_b[1]:
        raise ValueError("native fixture historical immutable payload drift")
    receipts = [native[f"poll{i}_received_ts"] for i in (1, 2, 3)]
    if receipts != sorted(receipts) or len(set(receipts)) != 3:
        raise ValueError("native fixture package receipts are not strictly ordered")

    golden = json.loads((root / GOLDEN_FIXTURE).read_text())
    polls = golden.get("polls", [])
    if len(polls) != 3 or [len(poll.get("trades", [])) for poll in polls] != [1, 2, 3]:
        raise ValueError("connector-neutral golden is not a three-poll projection")
    if [poll.get("order_status") for poll in polls] != [
        "partially_filled", "partially_filled", "filled"
    ]:
        raise ValueError("connector-neutral lifecycle progression drift")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2cb-r1-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2cb-r1-authority-check: PASS")
    print(f"base_commit: {BASE}")
    print("known_trade/new_trade/three_poll/public_runtime/R3/closed_surfaces: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
