#!/usr/bin/env python3
"""Adversarial mutation matrix for the Stage 5G-b R2 checker."""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODULE = "crates/strategy-runtime-core/src/stage5g_mock_ack.rs"
CHECKER = "scripts/stage5g_b_r2_check.py"
PATHS = [
    MODULE,
    CHECKER,
    "docs/stage-5/stage5g-b-r2-contract.json",
    "docs/stage-5/5g-b-r2-transition-history-coherence.md",
    "docs/current-status.md",
    "scripts/stage5g_b_r1_snapshot_gate.sh",
]


def replace(path: Path, before: str, after: str) -> None:
    text = path.read_text(encoding="utf-8")
    if before not in text:
        raise RuntimeError(f"mutation anchor missing: {before}")
    path.write_text(text.replace(before, after, 1), encoding="utf-8")


CASES = [
    ("no-send-prior-submitted-guard-removed", "slot.state == Stage5gMockAckSlotState::Waiting && slot.latest_ack.is_none()", "slot.state == Stage5gMockAckSlotState::Submitted && slot.latest_ack.is_none()"),
    ("no-send-prior-accepted-guard-removed", "slot.state == Stage5gMockAckSlotState::Waiting && slot.latest_ack.is_none()", "slot.state == Stage5gMockAckSlotState::Accepted && slot.latest_ack.is_none()"),
    ("no-send-unproved-expired-path-broken", "prior.status == CommandAckStatus::Expired", "prior.status == CommandAckStatus::Submitted"),
    ("observed-id-terminal-missing-id-accepted", "if stage5g_terminal_ack_loses_observed_broker_identity(&state.slots[slot_index], &event.ack) {", "if false && stage5g_terminal_ack_loses_observed_broker_identity(&state.slots[slot_index], &event.ack) {"),
    ("observed-id-conflicting-id-accepted", "if observed != incoming {", "if false && observed != incoming {"),
    ("ack-time-watermark-removed", "event.ack.received_ts < *last", "false && event.ack.received_ts < *last"),
    ("ack-time-watermark-not-fingerprinted", ".map(stage5g_ack_timestamp)", ".and(None)"),
    ("reversed-time-test-removed", "fn resolved_duplicate_rejects_reversed_ack_time()", "fn removed_resolved_duplicate_rejects_reversed_ack_time()"),
    ("production-attach-test-removed", "fn production_public_attach_apply_accepted_resolves_stage5c_once()", "fn removed_production_public_attach_apply_accepted_resolves_stage5c_once()"),
    ("production-apply-test-removed", "fn production_public_submitted_then_recovered_resolves_stage5c_once()", "fn removed_production_public_submitted_then_recovered_resolves_stage5c_once()"),
    ("stage5c-resolver-witness-removed", "resolve_stage5c_paper_intent_lifecycle(", "removed_resolve_stage5c_paper_intent_lifecycle("),
    ("stage5c-resolver-second-call-added", "match resolve_stage5c_paper_intent_lifecycle(", "if false { let _ = resolve_stage5c_paper_intent_lifecycle(\n        settled, Stage5cPaperIntentLifecycleInput { ack_records: Vec::new() }); }\n    match resolve_stage5c_paper_intent_lifecycle("),
]


def main() -> int:
    passed = 0
    for name, before, after in CASES:
        with tempfile.TemporaryDirectory(prefix="stage5g-b-r2-negative-") as raw:
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
    print(f"stage5g-b-r2-negative-harness: PASS {passed}/{len(CASES)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
