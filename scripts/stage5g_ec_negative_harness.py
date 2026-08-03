#!/usr/bin/env python3
"""Fourteen fail-closed mutations for Stage 5G-e-c R1."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_ec_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = tuple(str(path) for path in checker.FILES.values())


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"missing mutation anchor: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-ec-negative-") as raw:
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
        raise SystemExit(f"FAIL mutation escaped e-c checker: {label}")


def main() -> int:
    restart = str(checker.FILES["restart"])
    stage5d = str(checker.FILES["stage5d"])
    order = str(checker.FILES["order"])
    lib = str(checker.FILES["lib"])
    descriptor = str(checker.FILES["descriptor"])
    cases = (
        ("caller-account-authority", lambda r: mutate(r, restart, "pub snapshot_id: String,", "pub account_id: BrokerAccountId,\n    pub snapshot_id: String,")),
        ("source-binding-removed", lambda r: mutate(r, restart, "let (strategy_id, account_id, instrument_id) = source_binding(source);", "let (strategy_id, account_id, instrument_id) = caller_binding();")),
        ("restore-cross-binding-removed", lambda r: mutate(r, restart, "validate_projection_binding(&projection, &decoded.envelope, &fresh_runtime)?;", "let _unchecked_binding = (&projection, &decoded.envelope, &fresh_runtime);")),
        ("validation-after-mutation", lambda r: mutate(r, restart, "validate_projection(&projection)?;\n    validate_projection_binding(&projection, &decoded.envelope, &fresh_runtime)?;", "let _deferred_projection_validation = &projection;")),
        ("timer-zero-intent-proof-removed", lambda r: mutate(r, restart, "if !projection.lifecycle_proof.zero_intent_ready", "if false && !projection.lifecycle_proof.zero_intent_ready")),
        ("callback-self-authorization", lambda r: mutate(r, restart, "&projection.checkpoint,\n                projection.lifecycle_proof.authoritative_callback_count,", "&projection.checkpoint,\n                projection.summary.stage5c_callback_count,")),
        ("canonical-lifecycle-collapse-removed", lambda r: mutate(r, descriptor, '    "order_position_awaiting_committed"', '    "exact_replay_synchronized"')),
        ("rehash-negative-removed", lambda r: mutate(r, order, "stage5ge_c_r1_rehashed_stage5d_account_cross_binding_fails_closed", "removed_rehashed_cross_binding_test")),
        ("public-export-roundtrip-removed", lambda r: mutate(r, order, "stage5ge_c_r1_public_timer_ready_clean_process_roundtrip", "removed_public_export_roundtrip")),
        ("public-restore-roundtrip-removed", lambda r: mutate(r, order, "stage5ge_c_r1_public_new_package_source_clean_process_roundtrip", "removed_public_restore_roundtrip")),
        ("next-stage-observation-removed", lambda r: mutate(r, restart, "next_reconciliation_observation", "removed_future_view")),
        ("source-move-proof-removed", lambda r: mutate(r, lib, "moved_source_cannot_be_reused", "removed_source_move_witness")),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
        ("reduce-source-lifecycle-set", lambda r: mutate(r, descriptor, '    "new_package_awaiting"', '    "timer_ready"')),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-ec-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
