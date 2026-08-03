#!/usr/bin/env python3
"""Mutation matrix for Stage 5G-e-b R1."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_eb_r1_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = (
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/current-status.md",
    "docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.json",
    "docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.md",
)


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-r1-negative-") as raw:
        root = Path(raw)
        for relative in PATHS:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        mutation(root)
        try:
            checker.validate(root, check_git=False)
        except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
            print(f"PASS {label}")
            return
        raise SystemExit(f"FAIL mutation escaped Stage 5G-e-b R1 checker: {label}")


def main() -> int:
    timer = "crates/strategy-runtime-core/src/stage5g_timer.rs"
    order = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    descriptor = "docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.json"
    cases = (
        ("drop-pre-replay-checkpoint", lambda r: mutate(r, timer, "pre_replay_checkpoint: Stage5gTimerCheckpointEnvelope", "removed_pre_replay_checkpoint: Stage5gTimerCheckpointEnvelope")),
        ("drop-owned-canonical-replay", lambda r: mutate(r, timer, "canonical_replay: Stage5gCanonicalOrderPositionEvidence", "removed_canonical_replay: Stage5gCanonicalOrderPositionEvidence")),
        ("session-not-updated", lambda r: mutate(r, timer, "apply_stage5g_canonical_order_position_evidence(session, canonical_replay)", "apply_stage5g_canonical_order_position_evidence(session, canonicalize_stage5g_order_position_evidence)")),
        ("duplicate-counter-not-updated", lambda r: mutate(r, timer, "replay.duplicate_evidence_count += 1;", "replay.duplicate_evidence_count += 0;")),
        ("sequence-not-updated", lambda r: mutate(r, timer, "replay.last_total_sequence = Some(total_sequence);", "replay.last_total_sequence = replay.last_total_sequence;")),
        ("broker-slot-mutation-witness-removed", lambda r: mutate(r, order, "fn stage5ge_b_r1_exact_replay_synchronizes_session_then_new_package_commits()", "fn removed_exact_session_state_witness()")),
        ("callback-witness-removed", lambda r: mutate(r, order, "after_exact.stage5c_callback_count,", "after_exact.removed_callback_count,")),
        ("raw-recanonicalization", lambda r: mutate(r, timer, "    let transition =\n        match apply_stage5g_canonical_order_position_evidence", "    let _ = canonicalize_stage5g_order_position_evidence;\n    let transition =\n        match apply_stage5g_canonical_order_position_evidence")),
        ("next-package-chain-witness-removed", lambda r: mutate(r, order, "fn stage5ge_b_r1_two_exact_replays_then_new_package_form_one_linear_chain()", "fn removed_multiple_exact_chain_witness()")),
        ("stale-session-accepted", lambda r: mutate(r, timer, "if stage5g_order_position_session_replay(&session) != pre_replay", "if false && stage5g_order_position_session_replay(&session) != pre_replay")),
        ("release-validation-debug-only", lambda r: mutate(r, timer, "if validate_stage5g_timer_checkpoint(&committed_checkpoint).is_err()", "if { debug_assert!(validate_stage5g_timer_checkpoint(&committed_checkpoint).is_ok()); false }")),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-eb-r1-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
