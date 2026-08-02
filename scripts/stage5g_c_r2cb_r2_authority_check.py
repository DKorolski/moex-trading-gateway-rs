#!/usr/bin/env python3
"""Fail-closed authority check for R2 trade-ledger/watermark coherence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

BASE = "8e3a794ea47ff24c9daf3d88b2ef7c3f588e0f01"
ORDER_POSITION = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
STAGE5C = Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs")
STAGE5G_B = Path("crates/strategy-runtime-core/src/stage5g_mock_ack.rs")
FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
FINAM_FIXTURE = Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json")
GOLDEN_FIXTURE = Path("fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json")
R3_SNAPSHOT = Path("scripts/stage5g_c_r2ca_r3_snapshot_gate.py")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-r2cb-r2-trade-ledger-watermark-coherence.json")
ADR = Path("docs/adr/adr-stage5g-c-r2cb-r2-trade-ledger-watermark-coherence.md")

IMMUTABLE = {
    STAGE5C: "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7",
    STAGE5G_B: "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67",
    FINAM_MAPPER: "e1e91a075a8b73c99a6c2a76a3ec045e630de4da0943ed9d50d4756648b09b97",
    FINAM_FIXTURE: "3130424a9feb667b837037286d2fce17e19630ee3dde909284f85482dd5fb57d",
    GOLDEN_FIXTURE: "570fe747d3dc2be0f431768d547f4b7eca41456fe3b216e30575d5072189d608",
    R3_SNAPSHOT: "2f73a9882e3efa9a091079a59741bb068a8a1d5820e81d31ae30246702975315",
}
ALLOWED_CHANGED_PATHS = {
    str(ORDER_POSITION),
    str(DESCRIPTOR),
    str(ADR),
    "scripts/stage5g_c_r2cb_r2_authority_check.py",
    "scripts/stage5g_c_r2cb_r2_negative_harness.py",
    "scripts/stage5g_c_r2cb_r2_gate.sh",
    "scripts/stage5g_c_r2cb_r2_handoff_safety_check.py",
    "scripts/make_stage5g_c_r2cb_r2_handoff_archive.py",
}
FROZEN_PREFIXES = (
    "crates/broker-core/",
    "crates/broker-finam/",
    "crates/strategy-runtime-core/src/stage5c_",
    "crates/strategy-runtime-core/src/stage5d_",
    "crates/strategy-runtime-core/src/stage5f_",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "fixtures/",
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
        raise ValueError("R2 base commit does not resolve exactly")
    head = git(root, "rev-parse", "HEAD")
    if head != BASE and git(root, "rev-parse", "HEAD^") != BASE:
        raise ValueError("R2 is not exactly one successor to 8e3a794")
    changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
    unexpected = changed - ALLOWED_CHANGED_PATHS
    if unexpected:
        raise ValueError(f"R2 changed-path scope drift: {sorted(unexpected)}")
    frozen = sorted(path for path in changed if path.startswith(FROZEN_PREFIXES))
    if frozen:
        raise ValueError(f"frozen Stage 5C/5D/5F/Broker/fixture surface changed: {frozen}")


def check_descriptor(root: Path) -> None:
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
    watermark = descriptor.get("trade_watermark_contract", {})
    expected_watermark = {
        "source_of_truth": "post_validation_committed_slot_trade_ledger",
        "source_watermark": "max(previous_last_trade_source_ts, committed_trade_source_max)",
        "receipt_watermark": "max(previous_last_trade_received_ts, committed_trade_received_max)",
        "accepted_component_watermarks_monotonic": True,
        "incoming_snapshot_or_previous_update_forbidden": True,
        "unseen_late_trade_policy": "fail_closed_trade_time_regression",
    }
    if watermark != expected_watermark:
        raise ValueError("trade watermark contract drift")
    position = descriptor.get("position_only_market_contract", {})
    if position.get("without_target_order_with_target_trade") != "typed_retryable_target_trade_without_order":
        raise ValueError("position-only target-trade policy drift")
    for key in (
        "block_before_chronology_or_candidate_mutation",
        "blocked_capability_and_fingerprint_preserved",
    ):
        if position.get(key) is not True:
            raise ValueError(f"position-only contract drift: {key}")
    identity = descriptor.get("replay_identity_gate", {})
    if identity.get("closed_in_this_stage") is not False or identity.get("required_before_stage5g_d_or_stream_reuse") is not True:
        raise ValueError("replay identity gate was incorrectly closed or bypassed")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")


def check_source(root: Path) -> None:
    source = (root / ORDER_POSITION).read_text()
    require(
        source,
        (
            "TargetTradeWithoutOrder",
            "STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-BEGIN",
            "STAGE5G-C-R2CB-R2-COMMITTED-TRADE-WATERMARK-BEGIN",
            "refresh_trade_watermarks_from_committed_ledger(&mut next_slot);",
            "component_watermarks_are_monotonic(&session.state.slots[slot_index], &next_slot)",
            "fn has_target_correlated_order(",
            "fn has_target_correlated_trade(",
            "if current_slot.terminal && has_target_trade",
            "if has_target_order || has_target_trade || contradictory_target_position",
            "STAGE5G-C-R2CB-R2-LEDGER-WATERMARK-WITNESSES-BEGIN",
            "r2cb_r2_subset_refresh_preserves_committed_max_and_blocks_unseen_late_trade",
            "r2cb_r2_known_receipt_between_trade_and_global_max_preserves_global_max",
            "r2cb_r2_position_only_trades_block_transactionally_then_order_snapshot_converges",
            "r2cb_r2_position_only_contradictory_target_trades_are_never_ignored",
            "r2cb_r2_terminal_slot_rejects_target_trade_without_order",
            "r2cb_public_runtime_three_poll_golden_converges_through_stage5c",
            "validate_stage5c_market_terminal_outcome_r3",
            "settle_stage5c_validated_market_terminal_outcome_r3",
        ),
        "order-position",
    )
    public_apply = source.split("pub fn apply_stage5g_order_position_evidence", 1)[1].split(
        "fn classify_evidence_replay", 1
    )[0]
    block_at = public_apply.index("STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-BEGIN")
    chronology_at = public_apply.index("validate_snapshot_chronology(")
    apply_at = public_apply.index("let result = apply_to_slot(")
    refresh_at = public_apply.index("refresh_trade_watermarks_from_committed_ledger(&mut next_slot);")
    commit_at = public_apply.index("session.state.slots[slot_index] = next_slot;")
    if not block_at < chronology_at < apply_at < refresh_at < commit_at:
        raise ValueError("position block / validation / ledger / watermark / commit ordering drift")

    chronology = source.split("fn validate_snapshot_chronology", 1)[1].split(
        "fn refresh_trade_watermarks_from_committed_ledger", 1
    )[0]
    if "slot.last_trade_source_ts =" in chronology or "slot.last_trade_received_ts =" in chronology:
        raise ValueError("trade watermark can advance before committed ledger transition")
    refresh = source.split("fn refresh_trade_watermarks_from_committed_ledger", 1)[1].split(
        "fn component_watermarks_are_monotonic", 1
    )[0]
    require(
        refresh,
        (
            "slot.trades.iter().map(|trade| trade.source_ts).max()",
            "slot.trades.iter().map(|trade| trade.received_ts).max()",
            "[slot.last_trade_source_ts, committed_source_max]",
            "[slot.last_trade_received_ts, committed_received_max]",
            ".flatten()",
            ".max()",
        ),
        "committed-ledger watermark projection",
    )
    if ".or(" in refresh or "incoming" in refresh:
        raise ValueError("non-monotonic incoming/.or watermark update reachable")

    subset_witness = source.split(
        "fn r2cb_r2_subset_refresh_preserves_committed_max_and_blocks_unseen_late_trade", 1
    )[1].split(
        "fn r2cb_r2_known_receipt_between_trade_and_global_max_preserves_global_max", 1
    )[0]
    require(
        subset_witness,
        (
            "assert_eq!(after_subset.last_trade_source_ts, Some(source_b));",
            "assert_eq!(\n            blocked.reason(),\n            Stage5gOrderPositionError::TradeTimeRegression\n        );",
            "retained_fingerprint",
        ),
        "subset/late-trade witness",
    )

    block_body = public_apply.split(
        "STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-BEGIN", 1
    )[1].split("STAGE5G-C-R2CB-R2-POSITION-ONLY-TRADE-BLOCK-END", 1)[0]
    require(
        block_body,
        ("TargetTradeWithoutOrder", "return Err(block(", "session"),
        "position-only block",
    )
    if (
        "validate_stage5c_market_terminal_outcome_r1" in source
        or "validate_stage5c_market_terminal_outcome_r2" in source
        or "settle_stage5c_validated_market_terminal_outcome_r1" in source
        or "settle_stage5c_validated_market_terminal_outcome_r2" in source
    ):
        raise ValueError("accepted R3 terminal authority bypass reachable")


def check(root: Path) -> None:
    for relative, expected in IMMUTABLE.items():
        path = root / relative
        if not path.is_file() or sha256(path) != expected:
            raise ValueError(f"accepted authority drift: {relative}")
    check_git_scope(root)
    check_descriptor(root)
    check_source(root)
    r3 = subprocess.run(
        [sys.executable, str(root / R3_SNAPSHOT), "--root", str(root)],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if r3.returncode != 0:
        raise ValueError(f"accepted R3 snapshot rejected tree: {r3.stderr.strip()}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2cb-r2-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2cb-r2-authority-check: PASS")
    print(f"base_commit: {BASE}")
    print("committed_ledger/monotonic_watermarks/position_only/terminal/R3/closed_surfaces: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
