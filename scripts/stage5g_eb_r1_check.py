#!/usr/bin/env python3
"""Fail-closed checker for Stage 5G-e-b R1 exact-replay session rebind."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

BASE = "cbe4044bbca8303a7852d225364ec5cf89f02386"
TIMER = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.json")
CONTRACT = Path("docs/stage-5/stage5g-e-b-r1-exact-replay-session-rebind.md")
STATUS = Path("docs/current-status.md")
ALLOWED_CHANGED_PATHS = {
    str(TIMER), str(ORDER), str(LIB), str(DESCRIPTOR), str(CONTRACT), str(STATUS),
    "scripts/stage5g_eb_r1_check.py",
    "scripts/stage5g_eb_r1_negative_harness.py",
    "scripts/stage5g_eb_r1_predecessor_gate.py",
    "scripts/stage5g_eb_r1_gate.sh",
    "scripts/stage5g_eb_r1_handoff_safety_check.py",
    "scripts/make_stage5g_eb_r1_handoff_archive.py",
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
        require((root / relative).is_file(), f"missing Stage 5G-e-b R1 file: {relative}")
    timer = (root / TIMER).read_text()
    order = (root / ORDER).read_text()
    lib = (root / LIB).read_text()
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    contract = (root / CONTRACT).read_text()

    require(descriptor["stage"] == "5G-e-b-R1", "descriptor stage drift")
    require(descriptor["accepted_predecessor"] == BASE, "predecessor drift")
    for field in (
        "exact_replay_owns_pre_checkpoint", "exact_replay_owns_committed_checkpoint",
        "exact_replay_owns_canonical_evidence", "exact_replay_session_synchronization",
        "continuous_exact_to_new_package_chain", "multiple_exact_replays_supported",
        "release_checkpoint_validation_fail_closed",
    ):
        require(descriptor[field] is True, f"descriptor invariant lost: {field}")
    require(descriptor["exact_replay_mutable_fields"] == ["last_total_sequence", "duplicate_evidence_count"],
            "exact replay mutation scope drift")
    require(descriptor["focused_r1_test_count"] == 4, "focused R1 test count drift")
    require(descriptor["inherited_new_package_test_count"] == 7, "inherited e-b coverage drift")
    require(descriptor["negative_case_count"] == 12, "negative count drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()), "closed surface opened")

    for token in (
        "pub struct Stage5gExactReplayCheckpoint",
        "pre_replay_checkpoint: Stage5gTimerCheckpointEnvelope",
        "committed_checkpoint: Stage5gTimerCheckpointEnvelope",
        "canonical_replay: Stage5gCanonicalOrderPositionEvidence",
        "prior_continuation_checkpoint_ts_utc_ms: Option<i64>",
        "pub struct Stage5gCommittedExactReplaySession",
        "pub fn apply_stage5g_exact_replay_to_session(",
        "exact_replay: Stage5gExactReplayCheckpoint,",
        "exact_replay.into_stage5g_eb_r1_parts()",
        "stage5g_order_position_session_replay(&session) != pre_replay",
        "apply_stage5g_canonical_order_position_evidence(session, canonical_replay)",
        "let committed_replay = replay_from_payload(&committed_checkpoint.payload)",
        "if applied_replay != committed_replay",
        "synchronized_checkpoint != committed_checkpoint",
        "pub fn into_parts(self) -> (Stage5gOrderPositionSession, Stage5gTimerCheckpointEnvelope)",
        "let _ = apply_stage5g_exact_replay_to_session(session, proof);",
        "let _ = proof.checkpoint();",
        "let _ = blocked.checkpoint();",
        "let _ = synchronized.into_exact_replay_proof();",
    ):
        require(token in timer, f"exact-replay rebind token missing: {token}")
    exact_struct = region(
        timer,
        "pub struct Stage5gExactReplayCheckpoint {",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]",
    )
    for field in (
        "pre_replay_checkpoint: Stage5gTimerCheckpointEnvelope",
        "committed_checkpoint: Stage5gTimerCheckpointEnvelope",
        "canonical_replay: Stage5gCanonicalOrderPositionEvidence",
        "prior_continuation_checkpoint_ts_utc_ms: Option<i64>",
    ):
        require(
            re.search(rf"(?m)^\s{{4}}{re.escape(field)},?$", exact_struct) is not None,
            f"exact proof owning field drift: {field}",
        )

    exact_apply = region(
        timer,
        "pub fn apply_stage5g_exact_replay_to_session(",
        "/// Consumes one newly classified package",
    )
    for forbidden in (
        "canonicalize_stage5g_order_position_evidence",
        "serde_json::from",
        "canonical_replay.evidence().clone()",
        "debug_assert!",
    ):
        require(forbidden not in exact_apply, f"exact-replay authority bypass: {forbidden}")
    require(
        re.search(
            r"(?m)^\s+if stage5g_order_position_session_replay\(&session\) != pre_replay \{$",
            exact_apply,
        )
        is not None,
        "stale Stage 5G-c session must fail before exact replay application",
    )
    new_apply = region(
        timer,
        "pub fn apply_stage5g_new_package_candidate(",
        "pub fn attach_stage5g_timer_session(",
    )
    require("debug_assert!" not in new_apply, "NewPackage release validation is debug-only")
    require("validate_stage5g_timer_checkpoint(&committed_checkpoint).is_err()" in new_apply,
            "NewPackage hard checkpoint validation missing")

    classifier = region(
        timer,
        "pub fn classify_stage5g_post_checkpoint_evidence(",
        "pub(crate) fn checkpoint_envelope(",
    )
    exact_branch = region(
        classifier,
        "if let Some(previous) = replay",
        "let received_at = canonical_evidence.evidence().broker_truth.received_ts;",
    )
    for token in (
        "replay.last_total_sequence = Some(total_sequence);",
        "replay.duplicate_evidence_count += 1;",
        "pre_replay_checkpoint: envelope.clone()",
        "canonical_replay: canonical_evidence",
        "Stage5gCheckpointReplayError::InvalidCommittedCheckpoint",
    ):
        require(token in exact_branch, f"inherited exact classifier drift: {token}")

    tests = (
        "stage5ge_b_r1_exact_replay_synchronizes_session_then_new_package_commits",
        "stage5ge_b_r1_two_exact_replays_then_new_package_form_one_linear_chain",
        "stage5ge_b_r1_stale_session_blocks_before_exact_replay_application",
        "stage5ge_b_r1_crash_after_exact_persist_keeps_valid_commit_without_candidate",
    )
    require(all(f"fn {name}()" in order for name in tests), "R1 witness missing")
    require(order.count("fn stage5ge_b_") >= 11, "inherited seven e-b tests not retained")
    require(
        "after_exact.stage5c_callback_count," in order,
        "exact replay callback immutability witness missing",
    )
    require("apply_stage5g_exact_replay_to_session" in lib, "exact replay API export missing")
    require(f"Base commit: `{BASE}`." in contract, "contract base drift")

    for forbidden in ("redis::", "reqwest", "std::thread", "Utc::now(", "Method::POST", "Method::DELETE"):
        require(forbidden not in timer, f"I/O/autonomous surface entered R1: {forbidden}")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "R1 base missing")
        head = git(root, "rev-parse", "HEAD")
        require(head == BASE or git(root, "rev-parse", "HEAD^") == BASE,
                "Stage 5G-e-b R1 must be exactly one successor to cbe4044")
        changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
        unexpected = changed - ALLOWED_CHANGED_PATHS
        require(not unexpected, f"R1 scope drift: {sorted(unexpected)}")
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
        print(f"stage5g-eb-r1-check: FAIL: {error}")
        return 1
    print("stage5g-eb-r1-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
