#!/usr/bin/env python3
"""Mutation harness for committed Cancel/Day-expiry restart recovery."""

from __future__ import annotations

import copy

import stage8b_p1e_i1_committed_restart_check as check


def replace(content: dict[str, str], key: str, old: str, new: str) -> dict[str, str]:
    mutated = copy.deepcopy(content)
    if old not in mutated[key]:
        raise RuntimeError(f"mutation anchor missing: {key}: {old}")
    mutated[key] = mutated[key].replace(old, new, 1)
    return mutated


def replace_after(
    content: dict[str, str], key: str, start: str, old: str, new: str
) -> dict[str, str]:
    mutated = copy.deepcopy(content)
    begin = mutated[key].find(start)
    if begin < 0:
        raise RuntimeError(f"mutation section missing: {key}: {start}")
    target = mutated[key].find(old, begin)
    if target < 0:
        raise RuntimeError(f"mutation anchor missing after section: {key}: {old}")
    mutated[key] = mutated[key][:target] + new + mutated[key][target + len(old) :]
    return mutated


def main() -> None:
    base = check.load_content()
    check.validate_content(base)
    print("PASS positive-baseline")
    cases = [
        ("cancel-seal", "core", "pub fn cancel_publication_seal", "fn removed_cancel_publication_seal"),
        ("cancel-journal-seal", "journal", "binding.publication_seal_generation", "None::<u64>"),
        ("runtime-command-hash", "live", "evidence.canonical_command_sha256.as_deref()", "None::<&str>"),
        ("runtime-target", "live", "binding.active_broker_order_id.as_deref()", "None::<&str>"),
        ("runtime-source", "live", "evidence.m10_redis_id == candidate.predecessor_m10().redis_id", "true"),
        ("cancel-material", "recovery", "pub(crate) fn cancel_restart_material(", "fn removed_cancel_restart_material("),
        ("expiry-material", "recovery", "pub(crate) fn day_expiry_restart_material(", "fn removed_day_expiry_restart_material("),
        ("seal-successor", "recovery", "publication_seal_generation.checked_add(1)", "Some(publication_seal_generation)"),
        ("cancel-test", "process", "committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last", "removed_cancel_restart_test"),
        ("expiry-test", "process", "committed_day_expiry_restart_is_source_free_and_does_not_reread_schedule", "removed_expiry_restart_test"),
        ("pel-mid", "process", '"target truth cannot acknowledge M10"', '"removed target-first PEL check"'),
        ("cancel-no-reread", "process", '"committed Cancel restart cannot reread signed schedule"', '"removed cancel reread check"'),
        ("expiry-no-reread", "process", '"committed Day-expiry restart cannot reread signed schedule"', '"removed expiry reread check"'),
        ("historical-wiring-boundary", "document", "production owner-loop wiring was closed at this immutable source boundary", "production owner-loop wiring was never closed"),
        ("owner-cancel-terminal", "process", "CommittedCancelResolved(Stage8bP1eCommittedCancelResolvedV1)", "RemovedCancelResolved(Stage8bP1eCommittedCancelResolvedV1)"),
        ("owner-expiry-terminal", "process", "CommittedDayExpiryResolved(Stage8bP1eCommittedDayExpiryResolvedV1)", "RemovedDayExpiryResolved(Stage8bP1eCommittedDayExpiryResolvedV1)"),
        ("owner-expiry-pel", "process", "if control.pel_count().await? != 0", "if false"),
        ("owner-direct-cancel-test", "process", "owner_loop_routes_committed_cancel_ack_truth_to_xack_last", "removed_direct_cancel_owner_test"),
        ("owner-target-first-test", "process", "owner_loop_routes_committed_target_first_cancel_to_xack_last", "removed_target_first_owner_test"),
        ("owner-day-expiry-test", "process", "owner_loop_routes_committed_day_expiry_source_free_to_terminal", "removed_day_expiry_owner_test"),
        ("owner-document-no-fresh", "owner_document", "does not return to fresh schedule admission", "may return to fresh schedule admission"),
    ]
    section_cases = [
        ("cancel-reclaim", "redis", "pub async fn resume_stage8b_p1e_committed_cancel_with_redis(", ".reclaim_exact_binding(", ".removed_reclaim_exact_binding("),
        ("cancel-marker", "redis", "pub async fn resume_stage8b_p1e_committed_cancel_with_redis(", ".revalidate_exact_command_publication(", ".removed_revalidate_exact_command_publication("),
        ("cancel-successor", "redis", "pub async fn resume_stage8b_p1e_committed_cancel_with_redis(", ".exact_first_successor_m10(", ".removed_exact_first_successor_m10("),
        ("cancel-effect", "redis", "pub async fn resume_stage8b_p1e_committed_cancel_with_redis(", ".commit_stage8b_p1d3_cancel(", ".removed_commit_stage8b_p1d3_cancel("),
        ("expiry-effect", "redis", "pub fn resume_stage8b_p1e_committed_day_expiry(", ".expire_working_limit(authority, commitment_key)?", ".removed_expire_working_limit(authority, commitment_key)?"),
        ("pel-truth", "process", "async fn committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last()", '"S_truth must precede XACK-last"', '"removed truth-before-XACK check"'),
        ("pel-last", "process", "async fn committed_target_first_cancel_restart_preserves_pel_and_xacks_truth_last()", "assert_eq!(pending_after.count(), 0)", "assert_eq!(pending_after.count(), 1)"),
        ("direct-sequence", "process", "async fn owner_loop_routes_committed_cancel_ack_truth_to_xack_last()", "seq_ack.checked_add(1)", "Some(seq_ack)"),
        ("response-loss", "process", "async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()", "Stage8bP1RedisZeroIntentAckDisposition::AlreadyAcknowledged", "Stage8bP1RedisZeroIntentAckDisposition::AcknowledgedPending"),
        ("replay-sequence", "process", "async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()", "replay_audit.sequence_allocations", "audit_after.sequence_allocations"),
        ("replay-xack", "process", "async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()", "replay_effects.xack_total, 0", "replay_effects.xack_total, 1"),
        ("target-sequence", "process", "async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()", "target_truth_sequence.checked_add(1)", "target_truth_sequence.checked_add(2)"),
        ("replay-schedule-read", "process", "async fn owner_loop_routes_committed_target_first_cancel_to_xack_last()", "replay_effects.schedule_read_total, 0", "replay_effects.schedule_read_total, 1"),
        ("expiry-claim", "process", "async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal()", "effects.claim_total, 0", "effects.claim_total, 1"),
        ("expiry-schedule-read", "process", "async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal()", "effects.schedule_read_total, 0", "effects.schedule_read_total, 1"),
        ("expiry-sequence", "process", "async fn owner_loop_routes_committed_day_expiry_source_free_to_terminal()", "expiry.sequence_allocation_frontier.checked_add(1)", "expiry.sequence_allocation_frontier.checked_add(2)"),
    ]
    passed = 0
    for name, key, old, new in cases:
        mutated = replace(base, key, old, new)
        try:
            check.validate_content(mutated)
        except check.CheckFailure:
            passed += 1
            print(f"PASS {name}")
            continue
        raise SystemExit(f"FAIL mutation accepted: {name}")
    for name, key, start, old, new in section_cases:
        mutated = replace_after(base, key, start, old, new)
        try:
            check.validate_content(mutated)
        except check.CheckFailure:
            passed += 1
            print(f"PASS {name}")
            continue
        raise SystemExit(f"FAIL mutation accepted: {name}")
    total = len(cases) + len(section_cases)
    print(f"PASS stage8b-p1e-i1-committed-restart-negative-harness {passed}/{total}")


if __name__ == "__main__":
    main()
