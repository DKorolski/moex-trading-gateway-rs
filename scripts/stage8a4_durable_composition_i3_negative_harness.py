#!/usr/bin/env python3
"""Mutation harness proving the I3 checker fails closed."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_i3_check as checker

ROOT = Path(__file__).resolve().parents[1]


def copy_required(destination: Path) -> None:
    for relative in checker.REQUIRED:
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing mutation anchor: {relative}: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def mutate_all(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing mutation anchor: {relative}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def mutate_authority(root: Path, key: str, value: object) -> None:
    path = root / checker.AUTHORITY
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    cases = [
        ("predecessor-drift", lambda r: mutate_authority(r, "accepted_i2_r3_ref", "0" * 40)),
        ("review-hash-drift", lambda r: mutate_authority(r, "accepted_i2_r3_review_sha256", "0" * 64)),
        ("premature-acceptance", lambda r: mutate_authority(r, "status", "accepted")),
        ("ack-opened", lambda r: mutate_authority(r, "ack_readiness_enabled", True)),
        ("redis-opened", lambda r: mutate_authority(r, "redis_live_enabled", True)),
        ("finam-opened", lambda r: mutate_authority(r, "finam_post_delete_enabled", True)),
        ("runtime-live-opened", lambda r: mutate_authority(r, "runtime_live_enabled", True)),
        ("writer-owner-drift", lambda r: mutate_authority(r, "sole_writer_owner", "caller")),
        ("cas-field-removed", lambda r: mutate_authority(r, "pre_append_cas_fields", ["seal"])),
        ("idempotency-drift", lambda r: mutate_authority(r, "same_key_same_payload", "append_again")),
        ("collision-opened", lambda r: mutate_authority(r, "same_key_different_payload", "accept")),
        ("suffix-repair-weakened", lambda r: mutate_authority(r, "partial_suffix_action", "append_any")),
        ("writer-entry-removed", lambda r: mutate_text(r, checker.RUNTIME, "pub fn append_stage8a4_durable_batch_and_cover", "pub fn removed_stage8a4_writer")),
        ("s0-reread-removed", lambda r: mutate_all(r, checker.RUNTIME, "revalidate_cached_committed_seal", "removed_cached_seal_validation")),
        ("second-v2-rejection-removed", lambda r: mutate_text(r, checker.RUNTIME, "any(|record| matches!(record, Stage6JournalRecordVersioned::V2(_)))", "any(|_| false)")),
        ("covering-seal-removed", lambda r: mutate_all(r, checker.RUNTIME, "advance_recovery_seal", "removed_covering_seal_commit")),
        ("seal-reread-removed", lambda r: mutate_all(r, checker.RUNTIME, "read_committed_recovery_seal", "removed_committed_seal_read")),
        ("durable-binding-removed", lambda r: mutate_all(r, checker.CORE, "durable_request_binding_sha256", "removed_request_binding")),
        ("cancel-shape-removed", lambda r: mutate_all(r, checker.CORE, "validate_cancel_original_target_shape", "removed_original_order_guard")),
        ("stable-collision-test-removed", lambda r: mutate_text(r, checker.CORE, "same_stable_key_with_different_v2_payload_is_hard_conflict", "stable_collision_test_removed")),
        ("stale-cas-test-removed", lambda r: mutate_text(r, checker.CORE, "rejects_stale_frontier_and_request_state_before_append", "stale_cas_test_removed")),
        ("partial-crash-test-removed", lambda r: mutate_text(r, checker.RUNTIME, "restart_covers_partial_suffix_then_appends_only_missing_record", "partial_suffix_crash_test_removed")),
        ("arm-registry-check-removed", lambda r: mutate_all(r, checker.STAGE8A1, "read_arm_registration", "removed_registry_lookup")),
        ("post-effect-evidence-public", lambda r: mutate_text(r, checker.STAGE8A1, "pub(crate) struct Stage8a4PostEffectControlEvidence", "pub struct Stage8a4PostEffectControlEvidence")),
        ("private-writer-public", lambda r: mutate_text(r, checker.I3, "fn append_private_candidate_and_cover", "pub fn append_private_candidate_and_cover")),
        ("account-safety-removed", lambda r: mutate_all(r, checker.I3, "account_safety_summary", "removed_safety_projection")),
        ("transport-added", lambda r: mutate_text(r, checker.I3, "use super::super", "use reqwest;\nuse super::super")),
        ("matrix-row-removed", lambda r: mutate_text(r, checker.MATRIX, "I3-045,I4 and Stage8A5 remain separately gated,authority/docs\n", "")),
    ]
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8a4-i3-negative-") as raw:
        base = Path(raw)
        for name, mutation in cases:
            candidate = base / name
            copy_required(candidate)
            mutation(candidate)
            try:
                checker.check(candidate, git_scope=False)
            except Exception:
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-durable-composition-i3-negative: FAIL survived={name}")
    if passed != len(cases):
        raise SystemExit("stage8a4-durable-composition-i3-negative: FAIL inventory")
    print(f"stage8a4-durable-composition-i3-negative: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
