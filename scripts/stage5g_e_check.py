#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-a."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "54e26c886afd97cd443fd8b0728fe180ff4793b5"
DESCRIPTOR = Path("docs/stage-5/stage5g-e-restart-reconciliation-contract.json")
CONTRACT = Path("docs/stage-5/stage5g-e-restart-reconciliation-contract.md")
TIMER = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
ALLOWED_CHANGED_PATHS = {
    str(TIMER),
    str(LIB),
    "docs/current-status.md",
    str(DESCRIPTOR),
    str(CONTRACT),
    "scripts/stage5g_e_check.py",
    "scripts/stage5g_e_negative_harness.py",
    "scripts/stage5g_e_predecessor_gate.py",
    "scripts/stage5g_e_gate.sh",
    "scripts/stage5g_e_handoff_safety_check.py",
    "scripts/make_stage5g_e_handoff_archive.py",
}
FROZEN_PREFIXES = (
    "crates/broker-core/",
    "crates/broker-finam/",
    "crates/strategy-runtime-core/src/stage5c_",
    "crates/strategy-runtime-core/src/stage5d_",
    "crates/strategy-runtime-core/src/stage5f_",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "fixtures/",
    ".github/",
)


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def validate(root: Path, *, check_git: bool = True) -> None:
    for relative in (TIMER, LIB, DESCRIPTOR, CONTRACT, Path("docs/current-status.md")):
        require((root / relative).is_file(), f"missing Stage 5G-e-a file: {relative}")

    timer = (root / TIMER).read_text()
    lib = (root / LIB).read_text()
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    contract = (root / CONTRACT).read_text()

    require(descriptor["stage"] == "5G-e-a", "descriptor stage drift")
    require(descriptor["status"] == "implementation_review_candidate", "status drift")
    require(descriptor["accepted_predecessor"] == BASE, "predecessor drift")
    require(descriptor["accepted_stage5g_d_closed"] is True, "Stage 5G-d closure lost")
    require(descriptor["exact_replay_committed_checkpoint_available"] is True, "exact replay commit authority lost")
    require(descriptor["new_package_pre_candidate_checkpoint_retained"] is True, "pre-candidate checkpoint not retained")
    require(descriptor["new_package_candidate_checkpoint_persistable"] is False, "candidate checkpoint became persistable")
    require(descriptor["new_package_exact_canonical_candidate_owned"] is True, "canonical candidate ownership lost")
    require(descriptor["raw_evidence_recanonicalization_allowed"] is False, "raw re-canonicalization opened")
    require(descriptor["restart_case_count"] == 12, "restart matrix count drift")
    require(len(descriptor["implemented_restart_case_ids"]) == 0, "unimplemented crash case claimed")
    require(len(descriptor["deferred_restart_case_ids"]) == 12, "restart inventory incomplete")
    require(all(value is False for value in descriptor["closed_surfaces"].values()), "closed surface opened")

    for token in (
        "pub enum Stage5gCheckpointReplayResult",
        "ExactReplay(Stage5gExactReplayCheckpoint)",
        "NewPackage(Box<Stage5gNewPackageCandidate>)",
        "pub struct Stage5gExactReplayCheckpoint",
        "committed_checkpoint: Stage5gTimerCheckpointEnvelope",
        "pub struct Stage5gNewPackageCandidate",
        "pre_candidate_checkpoint: Stage5gTimerCheckpointEnvelope",
        "candidate_replay: Stage5gReplayCheckpoint",
        "canonical_candidate: Stage5gCanonicalOrderPositionEvidence",
        "pub fn into_exact_replay(self) -> Option<Stage5gExactReplayCheckpoint>",
        "pub fn into_new_package(self) -> Option<Stage5gNewPackageCandidate>",
        "pub fn pre_candidate_checkpoint(&self) -> &Stage5gTimerCheckpointEnvelope",
        "pub fn canonical_identity(&self) -> &str",
        "pub(crate) fn into_stage5g_e_parts",
        "Stage5gCheckpointReplayResult::ExactReplay(",
        "Stage5gCheckpointReplayResult::NewPackage(",
        "pre_candidate_checkpoint: envelope.clone()",
        "canonical_candidate: canonical_evidence",
        "fn stage5ge_a_exact_replay_alone_exposes_the_committed_checkpoint()",
        "fn stage5ge_a_new_package_retains_only_the_pre_candidate_committed_checkpoint()",
        "let _ = candidate.checkpoint();",
    ):
        require(token in timer, f"type-state token missing: {token}")

    result_region = timer.split("impl Stage5gCheckpointReplayResult", 1)[1].split(
        "impl Stage5gExactReplayCheckpoint", 1
    )[0]
    require("pub fn checkpoint" not in result_region, "common replay result exposes checkpoint")
    candidate_region = timer.split("impl Stage5gNewPackageCandidate", 1)[1].split(
        "pub fn attach_stage5g_timer_session", 1
    )[0]
    require("pub fn checkpoint" not in candidate_region, "NewPackage exposes candidate checkpoint")
    exact_region = timer.split("impl Stage5gExactReplayCheckpoint", 1)[1].split(
        "impl Stage5gNewPackageCandidate", 1
    )[0]
    require("pub fn checkpoint" in exact_region and "pub fn into_checkpoint" in exact_region,
            "ExactReplay committed checkpoint API drift")

    candidate_prefix = timer.split("pub struct Stage5gNewPackageCandidate", 1)[0].rsplit("#[", 1)[-1]
    candidate_fields = timer.split("pub struct Stage5gNewPackageCandidate {", 1)[1].split("}\n", 1)[0]
    for field in (
        "pre_candidate_checkpoint: Stage5gTimerCheckpointEnvelope",
        "candidate_replay: Stage5gReplayCheckpoint",
        "canonical_candidate: Stage5gCanonicalOrderPositionEvidence",
    ):
        require(
            re.search(rf"^\s*{re.escape(field)},?$", candidate_fields, flags=re.M) is not None,
            f"NewPackage owning field drift: {field}",
        )
    require("Serialize" not in candidate_prefix and "Deserialize" not in candidate_prefix,
            "NewPackage candidate became serializable")
    require("Stage5gExactReplayCheckpoint" in lib and "Stage5gNewPackageCandidate" in lib,
            "type-state exports missing")
    require("Accepted predecessor: `54e26c886afd97cd443fd8b0728fe180ff4793b5`." in contract,
            "contract predecessor drift")

    for forbidden in ("redis::", "reqwest", "std::thread", "Utc::now(", "Method::POST", "Method::DELETE"):
        require(forbidden not in timer, f"I/O/autonomous surface entered Stage 5G-e-a: {forbidden}")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "accepted base missing")
        head = git(root, "rev-parse", "HEAD")
        require(head == BASE or git(root, "rev-parse", "HEAD^") == BASE,
                "Stage 5G-e-a must be exactly one successor to 54e26c8")
        changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
        unexpected = changed - ALLOWED_CHANGED_PATHS
        require(not unexpected, f"Stage 5G-e-a scope drift: {sorted(unexpected)}")
        frozen = sorted(path for path in changed if path.startswith(FROZEN_PREFIXES))
        require(not frozen, f"accepted/frozen surface changed: {frozen}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), check_git=not args.skip_git)
    except (CheckFailure, ValueError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"stage5g-e-check: FAIL: {error}")
        return 1
    print("stage5g-e-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
