#!/usr/bin/env python3
"""Fail-closed checker for Stage 5G-e-b R2 historical exact replay."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

BASE = "1621307a6012fa1f9dcbc89a59651c801f6cc26f"
ORDER = Path("crates/strategy-runtime-core/src/stage5g_order_position.rs")
TIMER = Path("crates/strategy-runtime-core/src/stage5g_timer.rs")
LIB = Path("crates/strategy-runtime-core/src/lib.rs")
DESCRIPTOR = Path("docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.json")
CONTRACT = Path("docs/stage-5/stage5g-e-b-r2-historical-exact-replay-metadata.md")
STATUS = Path("docs/current-status.md")
TIMER_SHA256 = "30167b2551f853ac2e6f61452cddb6a4fe416c63c9f5613f940b97af43de0f25"
LIB_SHA256 = "f391f4791ceb158fe93295892346a9a775b57afc4d3fa90759ad8c9d94b2b5ac"
ALLOWED_CHANGED_PATHS = {
    str(ORDER), str(DESCRIPTOR), str(CONTRACT), str(STATUS),
    "scripts/stage5g_eb_r2_check.py",
    "scripts/stage5g_eb_r2_negative_harness.py",
    "scripts/stage5g_eb_r2_predecessor_gate.py",
    "scripts/stage5g_eb_r2_gate.sh",
    "scripts/stage5g_eb_r2_handoff_safety_check.py",
    "scripts/make_stage5g_eb_r2_handoff_archive.py",
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


def region(source: str, start: str, end: str) -> str:
    require(start in source and end in source, f"region boundary missing: {start} / {end}")
    return source.split(start, 1)[1].split(end, 1)[0]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def validate(root: Path, *, check_git: bool = True) -> None:
    for relative in (ORDER, TIMER, LIB, DESCRIPTOR, CONTRACT, STATUS):
        require((root / relative).is_file(), f"missing R2 file: {relative}")
    order = (root / ORDER).read_text()
    timer = (root / TIMER).read_text()
    contract = (root / CONTRACT).read_text()
    status = (root / STATUS).read_text()
    descriptor = json.loads((root / DESCRIPTOR).read_text())

    require(sha256(root / TIMER) == TIMER_SHA256, "accepted R1 timer authority drift")
    require(sha256(root / LIB) == LIB_SHA256, "accepted R1 public API drift")
    require(descriptor["stage"] == "5G-e-b-R2", "descriptor stage drift")
    require(descriptor["accepted_predecessor"] == BASE, "R2 predecessor drift")
    for field in (
        "one_exact_replay_metadata_authority",
        "replay_classification_before_new_package_preflight",
        "historical_same_request_replay_supported",
        "historical_cross_request_replay_supported",
        "new_package_continuation_guard_retained",
        "fingerprint_conflict_guard_retained",
        "release_checkpoint_validation_fail_closed",
    ):
        require(descriptor[field] is True, f"descriptor invariant lost: {field}")
    require(
        descriptor["exact_replay_mutable_fields"]
        == ["last_total_sequence", "duplicate_evidence_count"],
        "exact replay mutation scope drift",
    )
    require(descriptor["focused_r2_test_count"] == 6, "focused R2 test count drift")
    require(descriptor["inherited_pre_r2_test_count"] == 11, "inherited test count drift")
    require(descriptor["negative_case_count"] == 13, "negative count drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()),
            "closed surface opened")
    require(f"Base commit: `{BASE}`." in contract, "contract base drift")
    require("R2 is the current review candidate" in status, "current status drift")

    raw_apply = region(
        order,
        "pub fn apply_stage5g_order_position_evidence(",
        "/// The single Stage 5G-c canonical application core.",
    )
    require("stage5g_order_position_new_package_preflight" not in raw_apply,
            "raw evidence applies NewPackage preflight before replay classification")

    canonical_core = region(
        order,
        "pub(crate) fn apply_stage5g_canonical_order_position_evidence(",
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\nenum Stage5gReplayAdmission",
    )
    exact_call = canonical_core.find("match apply_stage5g_exact_replay_metadata(")
    new_preflight = canonical_core.find("stage5g_order_position_new_package_preflight(")
    require(exact_call >= 0 and new_preflight >= 0 and exact_call < new_preflight,
            "NewPackage preflight precedes exact replay classification")
    require(
        "Ok(Stage5gReplayAdmission::ExactReplay)" in canonical_core
        and "return Ok(Stage5gOrderPositionTransition::Awaiting(session));" in canonical_core,
        "exact replay does not return before broker-state application",
    )

    exact_metadata = region(
        order,
        "fn apply_stage5g_exact_replay_metadata(",
        "fn stage5g_order_position_new_package_preflight(",
    )
    for token in (
        "last_total_sequence",
        "evidence.total_sequence <= last",
        "evidence.broker_truth.account_id != session.state.account_id",
        "classify_evidence_replay(&session.state, identity, fingerprint)?",
        "session.state.last_total_sequence = Some(evidence.total_sequence);",
        "session.state.duplicate_evidence_count += 1;",
    ):
        require(token in exact_metadata, f"exact metadata authority drift: {token}")
    for forbidden in (
        "last_continuation_checkpoint_ts_utc_ms",
        "state.slots",
        "evidence_identities.push",
        "current_evidence_identity =",
        "last_broker_truth_received_at =",
        "last_broker_truth_received_ms =",
        "converge_through_stage5c",
        "apply_to_slot",
    ):
        require(forbidden not in exact_metadata, f"exact replay mutates forbidden state: {forbidden}")

    new_only = region(
        order,
        "fn stage5g_order_position_new_package_preflight(",
        "pub(crate) fn stage5g_order_position_session_replay(",
    )
    require(
        re.search(
            r"\.is_some_and\(\|checkpoint\| "
            r"evidence\.broker_truth\.received_ts\.timestamp_millis\(\) < checkpoint\)",
            new_only,
        )
        is not None,
        "NewPackage continuation guard missing",
    )
    require(".position(|slot| slot.ack.request_id == evidence.request_id)" in new_only,
            "NewPackage current-slot lookup missing")

    replay_classifier = region(
        order,
        "fn classify_evidence_replay(",
        "fn validate_snapshot_chronology(",
    )
    require(
        re.search(r"if previous\.fingerprint != fingerprint \{", replay_classifier) is not None,
        "fingerprint conflict guard weakened",
    )

    tests = (
        "stage5ge_b_r2_historical_a_b_exact_a_then_c_is_continuous",
        "stage5ge_b_r2_raw_historical_exact_uses_the_same_metadata_authority",
        "stage5ge_b_r2_two_historical_exact_replays_then_new_package",
        "stage5ge_b_r2_inherited_older_request_exact_replay_preserves_current_slot",
        "stage5ge_b_r2_new_identity_before_continuation_still_blocks",
        "stage5ge_b_r2_historical_identity_fingerprint_conflict_still_blocks",
    )
    require(all(f"fn {name}()" in order for name in tests), "R2 witness missing")
    require(len(re.findall(r"(?m)^\s+fn stage5ge_b_.*\(\) \{", order)) >= 17,
            "inherited Stage 5G-e-b tests not retained")

    for token in (
        "if validate_stage5g_timer_checkpoint(&committed_checkpoint).is_err()",
        "Stage5gNewPackageCommitError::InvalidCommittedCheckpoint",
        "Stage5gCheckpointReplayError::InvalidCommittedCheckpoint",
    ):
        require(token in timer, f"R1 hard-validation barrier drift: {token}")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "R2 base missing")
        head = git(root, "rev-parse", "HEAD")
        require(head == BASE or git(root, "rev-parse", "HEAD^") == BASE,
                "Stage 5G-e-b R2 must be exactly one successor to 1621307")
        changed = set(filter(None, git(root, "diff", "--name-only", BASE, "--").splitlines()))
        require(not changed - ALLOWED_CHANGED_PATHS,
                f"R2 scope drift: {sorted(changed - ALLOWED_CHANGED_PATHS)}")
        frozen = sorted(path for path in changed if path.startswith(FROZEN_PREFIXES))
        require(not frozen, f"accepted/frozen surface changed: {frozen}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root, check_git=not args.skip_git)
    except (CheckFailure, OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-eb-r2-check: FAIL: {error}")
        return 1
    print("stage5g-eb-r2-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
