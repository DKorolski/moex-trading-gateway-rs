#!/usr/bin/env python3
"""Semantic mutations for the Stage 5G-c replay package identity gate."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-c-replay-package-identity.json")
CHECKER = Path("scripts/stage5g_c_replay_identity_authority_check.py")
CASES = (
    ("receipt-ms-only-identity", ORDER, "received_at.timestamp_subsec_nanos()", "received_at.timestamp_subsec_millis()"),
    ("package-identity-schema-downgraded", ORDER, "STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION: u16 = 1", "STAGE5G_BROKER_TRUTH_PACKAGE_IDENTITY_SCHEMA_VERSION: u16 = 0"),
    ("strategy-sequence-becomes-identity-authority", ORDER, "evidence.broker_truth.account_id,\n        broker_truth_package_discriminator", "evidence.total_sequence,\n        broker_truth_package_discriminator"),
    ("exact-continuation-watermark-removed", ORDER, "last_broker_truth_received_at: Option<DateTime<Utc>>", "last_broker_truth_received_at_removed: Option<DateTime<Utc>>"),
    ("chronology-reverts-to-millisecond", ORDER, "last_broker_truth_received_at.is_some_and(|last| snapshot_ts < last)", "last_broker_truth_received_at.is_some_and(|last| snapshot_ts.timestamp_millis() < last.timestamp_millis())"),
    ("same-millisecond-witness-removed", ORDER, "replay_package_two_distinct_same_millisecond_packages_are_both_accepted", "replay_package_same_millisecond_witness_removed"),
    ("restart-replay-witness-removed", ORDER, "replay_package_exact_replay_and_restart_identity_are_stable", "replay_package_restart_witness_removed"),
    ("changed-payload-conflict-witness-removed", ORDER, "replay_package_same_source_identity_with_changed_payload_fails_closed", "replay_package_changed_payload_witness_removed"),
    ("missing-identity-structural-witness-removed", ORDER, "replay_package_missing_source_receipt_is_structurally_rejected", "replay_package_missing_receipt_witness_removed"),
    ("reverse-order-no-longer-blocks", ORDER, "Stage5gOrderPositionError::BrokerTruthTimeRegression\n        );\n        assert_eq!(\n            blocked.session()", "Stage5gOrderPositionError::PositionIncomplete\n        );\n        assert_eq!(\n            blocked.session()"),
    ("r3-entry-point-bypassed", ORDER, "validate_stage5c_market_terminal_outcome_r3", "validate_stage5c_market_terminal_outcome_r2"),
    ("stage5g-d-opened", DESCRIPTOR, '"stage5g_d": false', '"stage5g_d": true'),
)


def copy_root(destination: Path) -> None:
    shutil.copytree(
        ROOT, destination,
        ignore=shutil.ignore_patterns(".git", "target", "reports", "tmp", "*.log", "*.zip"),
    )


def main() -> int:
    passed = 0
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-replay-identity-negative-") as raw:
            mutant = Path(raw) / "repo"
            copy_root(mutant)
            path = mutant / relative
            source = path.read_text()
            if old not in source:
                raise RuntimeError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(old, new, 1))
            result = subprocess.run(
                [sys.executable, str(mutant / CHECKER), "--root", str(mutant)],
                cwd=mutant, text=True, capture_output=True, check=False,
            )
            if result.returncode == 0:
                print(f"FAIL mutation survived: {name}")
                return 1
            print(f"PASS {name}")
            passed += 1
    print(f"stage5g-c-replay-identity-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
