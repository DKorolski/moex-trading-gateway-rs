#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-b."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "0c1f1ce61c11c311e5df42edd4ed8c35beb838d2"
TIMER = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-e-b-owned-candidate-application.json")
CONTRACT = Path("docs/stage-5/stage5g-e-b-owned-candidate-application.md")
STATUS = Path("docs/current-status.md")
ALLOWED_CHANGED_PATHS = {
    str(TIMER), str(ORDER), str(LIB), str(DESCRIPTOR), str(CONTRACT), str(STATUS),
    "scripts/stage5g_eb_check.py",
    "scripts/stage5g_eb_negative_harness.py",
    "scripts/stage5g_eb_predecessor_gate.py",
    "scripts/stage5g_eb_gate.sh",
    "scripts/stage5g_eb_handoff_safety_check.py",
    "scripts/make_stage5g_eb_handoff_archive.py",
}
FROZEN_PREFIXES = (
    "crates/broker-core/", "crates/broker-finam/",
    "crates/strategy-runtime-core/src/stage5c_",
    "crates/strategy-runtime-core/src/stage5d_",
    "crates/strategy-runtime-core/src/stage5f_",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "fixtures/", ".github/",
)


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def region(source: str, start: str, end: str) -> str:
    require(start in source and end in source, f"region boundary missing: {start} / {end}")
    return source.split(start, 1)[1].split(end, 1)[0]


def validate(root: Path, *, check_git: bool = True) -> None:
    for relative in (TIMER, ORDER, LIB, DESCRIPTOR, CONTRACT, STATUS):
        require((root / relative).is_file(), f"missing Stage 5G-e-b file: {relative}")
    timer = (root / TIMER).read_text()
    order = (root / ORDER).read_text()
    lib = (root / LIB).read_text()
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    contract = (root / CONTRACT).read_text()

    require(descriptor["stage"] == "5G-e-b", "descriptor stage drift")
    require(descriptor["accepted_predecessor"] == BASE, "predecessor drift")
    require(descriptor["owned_candidate_consumed_once"] is True, "linear ownership lost")
    require(descriptor["canonicalization_pass_count"] == 1, "canonicalization count drift")
    require(descriptor["single_stage5g_c_canonical_apply_core"] is True, "canonical core lost")
    require(descriptor["commit_requires_structural_replay_equality"] is True, "commit proof lost")
    require(descriptor["awaiting_package_committed_after_apply"] is True, "Awaiting commit rule lost")
    require(descriptor["blocked_candidate_checkpoint_available"] is False, "blocked candidate became persistable")
    require(descriptor["focused_test_count"] == 7, "focused test count drift")
    require(descriptor["negative_case_count"] == 12, "negative count drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()), "closed surface opened")

    for token in (
        "pub(crate) fn apply_stage5g_canonical_order_position_evidence(",
        "apply_stage5g_canonical_order_position_evidence(session, canonical_evidence)",
        "let canonical_evidence = match canonicalize_stage5g_order_position_evidence(evidence)",
    ):
        require(token in order, f"single canonical Stage 5G-c core drift: {token}")
    canonical_core = region(
        order,
        "pub(crate) fn apply_stage5g_canonical_order_position_evidence(",
        "pub(crate) fn stage5g_order_position_session_replay(",
    )
    require("canonicalize_stage5g_order_position_evidence" not in canonical_core,
            "canonical Stage 5G-c core canonicalizes a second time")

    for token in (
        "pub enum Stage5gNewPackageApplyResult",
        "Awaiting(Stage5gCommittedAwaitingOrderPosition)",
        "Converged(Stage5gCommittedConvergedOrderPosition)",
        "MarketTerminal(Stage5gCommittedMarketTerminalOrderPosition)",
        "pub fn apply_stage5g_new_package_candidate(",
        "candidate: Stage5gNewPackageCandidate,",
        "candidate.into_stage5g_e_parts()",
        "apply_stage5g_canonical_order_position_evidence(session, canonical_candidate)",
        "if applied_replay != candidate_replay",
        "checkpoint_envelope(&applied_replay, prior_continuation_checkpoint)",
        "Stage5gNewPackageApplyBlockReason::Stage5gC(blocked.reason())",
        "pre_candidate_checkpoint",
        "session: blocked.into_session()",
        "let _ = apply_stage5g_new_package_candidate(session, candidate);",
        "let _ = candidate.canonical_identity();",
        "let _ = blocked.checkpoint();",
    ):
        require(token in timer, f"owned application token missing: {token}")
    apply_region = region(
        timer,
        "pub fn apply_stage5g_new_package_candidate(",
        "pub fn attach_stage5g_timer_session(",
    )
    for forbidden in (
        "canonicalize_stage5g_order_position_evidence",
        "serde_json::from",
        "candidate_replay.clone()",
        "canonical_candidate.evidence().clone()",
    ):
        require(forbidden not in apply_region, f"candidate authority bypass: {forbidden}")
    blocked_region = region(
        timer,
        "impl Stage5gNewPackageApplyBlocked",
        "impl std::fmt::Debug for Stage5gNewPackageApplyFailure",
    )
    require("pub fn checkpoint" not in blocked_region, "blocked result exposes candidate checkpoint")
    candidate_prefix = timer.split("pub struct Stage5gNewPackageCandidate", 1)[0].rsplit("#[", 1)[-1]
    require("Serialize" not in candidate_prefix and "Clone" not in candidate_prefix,
            "candidate became Clone/Serialize")
    candidate_impl = region(
        timer,
        "impl Stage5gNewPackageCandidate",
        "impl Stage5gNewPackageApplyBlocked",
    )
    require("pub fn checkpoint" not in candidate_impl,
            "candidate exposes checkpoint before application")

    tests = (
        "stage5ge_b_awaiting_commits_only_after_owned_canonical_application",
        "stage5ge_b_raw_and_owned_canonical_routes_share_exact_apply_core",
        "stage5ge_b_normal_convergence_commits_exact_applied_replay_once",
        "stage5ge_b_transactional_block_returns_only_pre_candidate_commit",
        "stage5ge_b_drop_before_apply_keeps_old_checkpoint_reclassifiable",
        "stage5ge_b_session_checkpoint_mismatch_blocks_before_application",
        "stage5ge_b_r3_market_terminal_candidate_commits_without_callback_duplication",
    )
    require(all(f"fn {name}()" in order for name in tests), "focused witness missing")
    require("apply_stage5g_new_package_candidate" in lib, "public linear apply export missing")
    require(f"Accepted predecessor: `{BASE}`." in contract, "contract predecessor drift")

    combined = timer + order
    for forbidden in ("redis::", "reqwest", "std::thread", "Utc::now(", "Method::POST", "Method::DELETE"):
        require(forbidden not in timer, f"I/O/autonomous surface entered Stage 5G-e-b: {forbidden}")
    require("runtime_live_enabled: true" not in combined, "runtime-live opened")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "accepted base missing")
        head = git(root, "rev-parse", "HEAD")
        require(head == BASE or git(root, "rev-parse", "HEAD^") == BASE,
                "Stage 5G-e-b must be exactly one successor to 0c1f1ce")
        changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
        unexpected = changed - ALLOWED_CHANGED_PATHS
        require(not unexpected, f"Stage 5G-e-b scope drift: {sorted(unexpected)}")
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
        print(f"stage5g-eb-check: FAIL: {error}")
        return 1
    print("stage5g-eb-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
