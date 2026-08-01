#!/usr/bin/env python3
"""Adversarial mutation matrix for Stage 5G-b R3."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CHECKER = "scripts/stage5g_b_r3_check.py"
PATHS = [
    MODULE,
    CHECKER,
    "docs/stage-5/stage5g-b-r3-contract.json",
    "docs/stage-5/5g-b-r3-duplicate-transition-identity.md",
    "docs/current-status.md",
    "scripts/stage5g_b_r2_snapshot_gate.sh",
    "scripts/stage5g_b_r3_origin_sync_gate.sh",
]


def replace(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    if before not in text:
        raise RuntimeError(f"mutation anchor missing: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


CASES = [
    (
        "successful-duplicate-watermark-update-removed",
        "state.last_ack_received_ts_utc = Some(event.ack.received_ts);\n    state.duplicate_status_count += 1;",
        "state.duplicate_status_count += 1;",
    ),
    (
        "current-lifecycle-fingerprint-projection-removed",
        "current_lifecycle_fingerprint_sha256: stage5g_state_fingerprint(state),",
        "current_lifecycle_fingerprint_sha256: String::new(),",
    ),
    (
        "current-lifecycle-fingerprint-rebound-to-precallback",
        "current_lifecycle_fingerprint_sha256: stage5g_state_fingerprint(state),",
        "current_lifecycle_fingerprint_sha256: pre_callback_lifecycle_fingerprint_sha256.to_string(),",
    ),
    (
        "duplicate-time-anticollision-test-removed",
        "fn duplicate_timestamp_changes_transition_fingerprint()",
        "fn removed_duplicate_timestamp_changes_transition_fingerprint()",
    ),
    (
        "continuation-divergence-test-removed",
        "fn duplicate_timestamp_changes_continuation_semantics()",
        "fn removed_duplicate_timestamp_changes_continuation_semantics()",
    ),
    (
        "production-public-fingerprint-witness-removed",
        "fn production_public_duplicate_time_changes_transition_fingerprint_without_callback_replay()",
        "fn removed_production_public_duplicate_time_changes_transition_fingerprint_without_callback_replay()",
    ),
]


def main() -> int:
    passed = 0
    for name, before, after in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-b-r3-negative-") as raw:
            root = Path(raw)
            for relative in PATHS:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / relative, target)
            replace(root / MODULE, before, after)
            result = subprocess.run(
                ["python3", str(root / CHECKER), "--root", str(root)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                print(f"FAIL {name}: checker accepted mutation")
                return 1
            passed += 1
            print(f"PASS {name}")
    print(f"stage5g-b-r3-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
