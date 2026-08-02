#!/usr/bin/env python3
"""Fail-closed authority check for the Stage 5G-c replay identity gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

BASE = "470d898104b06fbf725f532554ba7a0fbde7e5c3"
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-replay-package-identity.json")
ADR = Path("docs/adr/adr-stage5g-c-replay-package-identity.md")
R3_SNAPSHOT = Path("scripts/stage5g_c_r2ca_r3_snapshot_gate.py")
IMMUTABLE = {
    Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs"): "ca357ea9e2dd39910d119e1033e00eef7698cf459255a95825591cd1c86984e7",
    Path("crates/strategy-runtime-core/src/stage5g_mock_ack.rs"): "a3aa1a64ebc763750b52530925c03b4573a30627c05211491a0ae51f64da7b67",
    Path("crates/broker-finam/src/mapper.rs"): "e1e91a075a8b73c99a6c2a76a3ec045e630de4da0943ed9d50d4756648b09b97",
    Path("fixtures/finam/stage5g_r2cb_full_snapshot_sequence.json"): "3130424a9feb667b837037286d2fce17e19630ee3dde909284f85482dd5fb57d",
    Path("fixtures/expected/stage5g_r2cb_three_poll_broker_truth.json"): "570fe747d3dc2be0f431768d547f4b7eca41456fe3b216e30575d5072189d608",
    R3_SNAPSHOT: "2f73a9882e3efa9a091079a59741bb068a8a1d5820e81d31ae30246702975315",
}
ALLOWED_CHANGED_PATHS = {
    str(ORDER), str(DESCRIPTOR), str(ADR),
    "scripts/stage5g_c_replay_identity_authority_check.py",
    "scripts/stage5g_c_replay_identity_negative_harness.py",
    "scripts/stage5g_c_replay_identity_gate.sh",
    "scripts/stage5g_c_replay_identity_handoff_safety_check.py",
    "scripts/make_stage5g_c_replay_identity_handoff_archive.py",
}
FROZEN_PREFIXES = (
    "crates/broker-core/", "crates/broker-finam/",
    "crates/strategy-runtime-core/src/stage5c_",
    "crates/strategy-runtime-core/src/stage5d_",
    "crates/strategy-runtime-core/src/stage5f_",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs", "fixtures/",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(source: str, tokens: tuple[str, ...], label: str) -> None:
    for token in tokens:
        if token not in source:
            raise ValueError(f"{label} token missing: {token}")


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def check_scope(root: Path) -> None:
    if not (root / ".git").exists():
        return
    if git(root, "rev-parse", f"{BASE}^{{commit}}") != BASE:
        raise ValueError("identity-gate base does not resolve exactly")
    head = git(root, "rev-parse", "HEAD")
    if head != BASE and git(root, "rev-parse", "HEAD^") != BASE:
        raise ValueError("identity gate is not exactly one successor to 470d898")
    changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
    unexpected = changed - ALLOWED_CHANGED_PATHS
    if unexpected:
        raise ValueError(f"identity-gate scope drift: {sorted(unexpected)}")
    frozen = sorted(path for path in changed if path.startswith(FROZEN_PREFIXES))
    if frozen:
        raise ValueError(f"frozen surface changed: {frozen}")


def check_descriptor(root: Path) -> None:
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    if descriptor.get("base_commit") != BASE or descriptor.get("required_parent_relation") != "exactly_one_successor":
        raise ValueError("descriptor predecessor binding drift")
    identity = descriptor.get("identity_contract", {})
    expected = {
        "schema": "moex.broker-truth.package.v1",
        "source_authority": "BrokerTruthSnapshot.received_ts",
        "precision": "unix_seconds_plus_nanoseconds",
        "strategy_total_sequence_is_authority": False,
        "payload_fingerprint_is_identity": False,
        "exact_replay": "idempotent",
        "changed_payload_same_identity": "conflicting_duplicate_evidence",
        "distinct_same_millisecond_packages": "accepted_when_full_precision_receipts_differ",
        "exact_receipt_collision_changed_payload": "fail_closed_ambiguous",
        "missing_receipt": "structurally_rejected_by_required_broker_truth_field",
        "reverse_package_order": "broker_truth_time_regression",
        "restart_identity": "exact_full_precision_receipt_preserved",
    }
    if identity != expected:
        raise ValueError("replay identity contract drift")
    if descriptor.get("schema_versions") != {
        "order_position_lifecycle": 4,
        "evidence_fingerprint": 3,
        "package_identity": 1,
    }:
        raise ValueError("identity schema-version drift")
    closed = descriptor.get("closed_surfaces")
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        raise ValueError("closed surface opened")


def check_source(root: Path) -> None:
    source = (root / ORDER).read_text()
    require(source, (
        "STAGE5G_ORDER_POSITION_SCHEMA_VERSION: u16 = 4",
        "STAGE5G_EVIDENCE_FINGERPRINT_SCHEMA_VERSION: u16 = 3",
        "STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION: u16 = 1",
        "STAGE5G-C-REPLAY-PACKAGE-IDENTITY-BEGIN",
        "moex.broker-truth.package.v{}:{}:{:09}",
        "received_at.timestamp()",
        "received_at.timestamp_subsec_nanos()",
        "moex.stage5g.order-position-evidence-identity.v3",
        "last_broker_truth_received_at: Option<DateTime<Utc>>",
        "last_broker_truth_received_at.is_some_and(|last| snapshot_ts < last)",
        "last_broker_truth_package_discriminator",
        "STAGE5G-C-REPLAY-PACKAGE-IDENTITY-WITNESSES-BEGIN",
        "replay_package_exact_replay_and_restart_identity_are_stable",
        "replay_package_same_source_identity_with_changed_payload_fails_closed",
        "replay_package_two_distinct_same_millisecond_packages_are_both_accepted",
        "replay_package_missing_source_receipt_is_structurally_rejected",
        "r2cb_public_runtime_three_poll_golden_converges_through_stage5c",
        "r2cb_r2_subset_refresh_preserves_committed_max_and_blocks_unseen_late_trade",
        "validate_stage5c_market_terminal_outcome_r3",
        "settle_stage5c_validated_market_terminal_outcome_r3",
    ), "order-position")
    identity_body = source.split("fn evidence_identity", 1)[1].split(
        "// STAGE5G-C-REPLAY-PACKAGE-IDENTITY-END", 1
    )[0]
    if source.count("last_broker_truth_received_at: Option<DateTime<Utc>>") != 2:
        raise ValueError("exact package watermark state/signature binding drift")
    if "timestamp_millis" in identity_body or "total_sequence" in identity_body or "fingerprint" in identity_body:
        raise ValueError("millisecond/caller/payload authority leaked into package identity")
    discriminator = source.split("fn broker_truth_received_at_discriminator", 1)[1].split(
        "fn broker_truth_package_discriminator", 1
    )[0]
    if "timestamp_subsec_nanos" not in discriminator or "timestamp_millis" in discriminator:
        raise ValueError("full-precision package discriminator weakened")
    public_apply = source.split("pub fn apply_stage5g_order_position_evidence", 1)[1].split(
        "fn classify_evidence_replay", 1
    )[0]
    if public_apply.index("classify_evidence_replay") > public_apply.index("BrokerTruthBeforeAck"):
        raise ValueError("exact replay is not classified before continuation chronology")
    if "last_broker_truth_received_at = Some(evidence.broker_truth.received_ts)" not in public_apply:
        raise ValueError("exact package continuation watermark not committed")
    witnesses = source.split("STAGE5G-C-REPLAY-PACKAGE-IDENTITY-WITNESSES-BEGIN", 1)[1].split(
        "STAGE5G-C-REPLAY-PACKAGE-IDENTITY-WITNESSES-END", 1
    )[0]
    require(witnesses, (
        "first_receipt.timestamp_millis(),",
        "second_receipt.timestamp_millis()",
        "assert_ne!(first_identity, second_identity);",
        "ConflictingDuplicateEvidence",
        "BrokerTruthTimeRegression",
        "duplicate_evidence_count, 1",
        "serde_json::from_value::<BrokerTruthSnapshot>(encoded).is_err()",
    ), "identity witnesses")
    if any(token in source for token in (
        "validate_stage5c_market_terminal_outcome_r1",
        "validate_stage5c_market_terminal_outcome_r2",
        "settle_stage5c_validated_market_terminal_outcome_r1",
        "settle_stage5c_validated_market_terminal_outcome_r2",
    )):
        raise ValueError("accepted R3 authority bypass reachable")


def check(root: Path) -> None:
    for relative, expected in IMMUTABLE.items():
        if not (root / relative).is_file() or sha256(root / relative) != expected:
            raise ValueError(f"accepted artifact drift: {relative}")
    check_scope(root)
    check_descriptor(root)
    check_source(root)
    result = subprocess.run(
        [sys.executable, str(root / R3_SNAPSHOT), "--root", str(root)],
        cwd=root, text=True, capture_output=True, check=False,
    )
    if result.returncode != 0:
        raise ValueError(f"accepted R3 snapshot rejected tree: {result.stderr.strip()}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-replay-identity-authority-check: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-replay-identity-authority-check: PASS")
    print(f"base_commit: {BASE}")
    print("full_precision/exact_replay/conflict/same_ms/restart/reverse/R3/closed: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
