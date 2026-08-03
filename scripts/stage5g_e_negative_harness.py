#!/usr/bin/env python3
"""Mutation harness for the Stage 5G-e-a commit barrier."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_e_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = (
    "crates/strategy-runtime-core/src/stage5g_timer.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "docs/current-status.md",
    "docs/stage-5/stage5g-e-restart-reconciliation-contract.json",
    "docs/stage-5/stage5g-e-restart-reconciliation-contract.md",
)


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"mutation anchor missing: {relative}: {old}")
    path.write_text(source.replace(old, new, 1))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-e-negative-") as raw:
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
        raise SystemExit(f"FAIL mutation escaped Stage 5G-e checker: {label}")


def main() -> int:
    timer = "crates/strategy-runtime-core/src/stage5g_timer.rs"
    descriptor = "docs/stage-5/stage5g-e-restart-reconciliation-contract.json"
    cases = (
        ("common-result-checkpoint-bypass", lambda r: mutate(r, timer, "impl Stage5gCheckpointReplayResult {", "impl Stage5gCheckpointReplayResult {\n    pub fn checkpoint(&self) {}")),
        ("new-package-checkpoint-bypass", lambda r: mutate(r, timer, "impl Stage5gNewPackageCandidate {", "impl Stage5gNewPackageCandidate {\n    pub fn checkpoint(&self) {}")),
        ("serialize-new-package-candidate", lambda r: mutate(r, timer, "pub struct Stage5gNewPackageCandidate {", "#[derive(Serialize, Deserialize)]\npub struct Stage5gNewPackageCandidate {")),
        ("drop-pre-candidate-checkpoint", lambda r: mutate(r, timer, "pre_candidate_checkpoint: Stage5gTimerCheckpointEnvelope", "removed_pre_candidate_checkpoint: Stage5gTimerCheckpointEnvelope")),
        ("drop-owned-canonical-candidate", lambda r: mutate(r, timer, "canonical_candidate: Stage5gCanonicalOrderPositionEvidence", "removed_canonical_candidate: Stage5gCanonicalOrderPositionEvidence")),
        ("drop-exact-commit-api", lambda r: mutate(r, timer, "pub fn into_checkpoint(self)", "fn removed_into_checkpoint(self)")),
        ("remove-exact-witness", lambda r: mutate(r, timer, "fn stage5ge_a_exact_replay_alone_exposes_the_committed_checkpoint()", "fn removed_exact_replay_witness()")),
        ("remove-new-package-witness", lambda r: mutate(r, timer, "fn stage5ge_a_new_package_retains_only_the_pre_candidate_committed_checkpoint()", "fn removed_new_package_witness()")),
        ("remove-compile-fail-witness", lambda r: mutate(r, timer, "let _ = candidate.checkpoint();", "let _ = candidate.pre_candidate_checkpoint();")),
        ("claim-candidate-persistable", lambda r: mutate(r, descriptor, '"new_package_candidate_checkpoint_persistable": false', '"new_package_candidate_checkpoint_persistable": true')),
        ("open-stage5g-f", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
        ("open-redis", lambda r: mutate(r, timer, "use broker_core::StrategyRequestId;", "use broker_core::StrategyRequestId;\nuse redis::Client;")),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-e-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
