#!/usr/bin/env python3
"""Mutation matrix for the Stage 5G-e-b owned application boundary."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_eb_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = (
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/current-status.md",
    "docs/stage-5/stage5g-e-b-owned-candidate-application.json",
    "docs/stage-5/stage5g-e-b-owned-candidate-application.md",
)


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-eb-negative-") as raw:
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
        raise SystemExit(f"FAIL mutation escaped Stage 5G-e-b checker: {label}")


def main() -> int:
    timer = "crates/strategy-runtime-core/src/stage5g_timer.rs"
    order = "crates/strategy-runtime-core/src/stage5g_order_position.rs"
    descriptor = "docs/stage-5/stage5g-e-b-owned-candidate-application.json"
    cases = (
        ("raw-reconstruction", lambda r: mutate(r, timer, "let canonical_identity =", "let _raw = serde_json::from_str::<serde_json::Value>(\"{}\");\n    let canonical_identity =")),
        ("second-canonicalization", lambda r: mutate(r, timer, "let transition =", "let _ = canonicalize_stage5g_order_position_evidence;\n    let transition =")),
        ("candidate-checkpoint-before-apply", lambda r: mutate(r, timer, "impl Stage5gNewPackageCandidate {", "impl Stage5gNewPackageCandidate {\n    pub fn checkpoint(&self) {}")),
        ("commit-unapplied-candidate-replay", lambda r: mutate(r, timer, "checkpoint_envelope(&applied_replay, prior_continuation_checkpoint)", "checkpoint_envelope(&candidate_replay, prior_continuation_checkpoint)")),
        ("ignore-replay-mismatch", lambda r: mutate(r, timer, "if applied_replay != candidate_replay", "if false && applied_replay != candidate_replay")),
        ("blocked-candidate-checkpoint", lambda r: mutate(r, timer, "impl Stage5gNewPackageApplyBlocked {", "impl Stage5gNewPackageApplyBlocked {\n    pub fn checkpoint(&self) {}")),
        ("blocked-replay-append-witness-removed", lambda r: mutate(r, order, "fn stage5ge_b_transactional_block_returns_only_pre_candidate_commit()", "fn removed_transactional_block_witness()")),
        ("candidate-clone", lambda r: mutate(r, timer, "pub struct Stage5gNewPackageCandidate {", "#[derive(Clone)]\npub struct Stage5gNewPackageCandidate {")),
        ("candidate-not-consumed", lambda r: mutate(r, timer, "candidate: Stage5gNewPackageCandidate,", "candidate: &Stage5gNewPackageCandidate,")),
        ("awaiting-uncommitted", lambda r: mutate(r, descriptor, '"awaiting_package_committed_after_apply": true', '"awaiting_package_committed_after_apply": false')),
        ("accepted-r3-bypass", lambda r: mutate(r, order, "fn stage5ge_b_r3_market_terminal_candidate_commits_without_callback_duplication()", "fn removed_r3_market_terminal_witness()")),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-eb-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
