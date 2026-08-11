#!/usr/bin/env python3
"""Evaluate all 52 Stage 7A rows against concrete gate artifacts."""
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

MATRIX = Path("docs/stage-7/STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv")


def witness(file: str, *tokens: str) -> list[tuple[str, tuple[str, ...]]]:
    return [(file, tokens)]


WITNESSES: dict[str, list[tuple[str, tuple[str, ...]]]] = {
    "A-001": witness("stage7a-check.txt", "stage7a-check: PASS"),
    "A-002": witness("closed-surface.txt", "finam_post_delete=false", "runtime_live=false"),
    "A-003": witness("stage7a-check.txt", "stage6_authority=exclusive"),
    "A-004": witness("bridge-debug.txt", "accepted_ack_then_runtime_duplicate_is_stage5g_noop ... ok"),
    "A-005": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-006": witness("bridge-debug.txt", "stop_shape_and_profile_drift_cannot_reach_provider ... ok"),
    "A-007": witness("bridge-debug.txt", "envelope_policy_and_ttl_fail_before_paper_effect ... ok"),
    "A-008": witness("bridge-debug.txt", "dlq_outage_empty_polls_do_not_restore_readiness ... ok"),
    "A-009": witness("bridge-debug.txt", "envelope_policy_and_ttl_fail_before_paper_effect ... ok"),
    "A-010": witness("bridge-debug.txt", "envelope_policy_and_ttl_fail_before_paper_effect ... ok"),
    "A-011": witness("bridge-debug.txt", "dlq_is_redacted_and_cursor_is_bounded ... ok"),
    "A-012": witness("bridge-debug.txt", "dlq_outage_empty_polls_do_not_restore_readiness ... ok"),
    "A-013": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-014": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-015": witness("bridge-debug.txt", "xautoclaim_tail_eventually_reached_with_claim_count_1_max_pages_1 ... ok"),
    "A-016": witness("bridge-debug.txt", "dlq_is_redacted_and_cursor_is_bounded ... ok"),
    "A-017": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-018": witness("bridge-debug.txt", "ack_xadd_failure_redelivery_republishes_canonical_accepted ... ok"),
    "A-019": witness("bridge-debug.txt", "ack_xadd_success_before_xack_redelivery_emits_runtime_duplicate ... ok"),
    "A-020": witness("bridge-debug.txt", "place_then_cancel_use_one_profile_without_redis_identity_authority ... ok"),
    "A-021": [
        ("bridge-debug.txt", ("accepted_ack_then_runtime_duplicate_is_stage5g_noop ... ok",)),
        ("stage5g-ack-oracle.txt", ("gack07_duplicate_requires_prior_outcome_and_exact_duplicate_is_noop ... ok", "duplicate_ack_terminal_twice_and_expired_lifecycle_block ... ok")),
    ],
    "A-022": witness("bridge-debug.txt", "envelope_policy_and_ttl_fail_before_paper_effect ... ok"),
    "A-023": witness("bridge-debug.txt", "stop_shape_and_profile_drift_cannot_reach_provider ... ok"),
    "A-024": witness("bridge-debug.txt", "place_then_cancel_use_one_profile_without_redis_identity_authority ... ok"),
    "A-025": witness("core-debug.txt", "stage7a_limit_pending_blocks_second_new_place ... ok", "stage7a_market_filled_nonfinal_blocks_second_new_place ... ok", "stage7a_broker_order_found_nonfinal_blocks_second_new_place ... ok"),
    "A-026": witness("core-debug.txt", "stage7a_admission_deduplicates_exact_command_without_second_effect ... ok"),
    "A-027": witness("core-debug.txt", "stage7a_conflicting_duplicate_is_held_without_mutation ... ok"),
    "A-028": witness("bridge-debug.txt", "uncertain_provider_and_post_dispatch_crash_remain_pending ... ok"),
    "A-029": witness("stage7a-check.txt", "stage6_authority=exclusive"),
    "A-030": witness("core-debug.txt", "stage7a_resumes_only_the_dispatch_after_accepted_crash_window ... ok"),
    "A-031": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-032": witness("bridge-debug.txt", "supervisor_never_leaves_stale_ready_after_failure_or_stop ... ok"),
    "A-033": witness("bridge-debug.txt", "supervisor_never_leaves_stale_ready_after_failure_or_stop ... ok"),
    "A-034": witness("bridge-debug.txt", "ack_xadd_failure_redelivery_republishes_canonical_accepted ... ok"),
    "A-035": witness("bridge-debug.txt", "dlq_outage_empty_polls_do_not_restore_readiness ... ok"),
    "A-036": witness("bridge-debug.txt", "auto_consumer_names_are_process_unique_and_not_execution_ids ... ok"),
    "A-037": witness("bridge-debug.txt", "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler ... ok"),
    "A-038": witness("bridge-debug.txt", "auto_consumer_names_are_process_unique_and_not_execution_ids ... ok"),
    "A-039": witness("bridge-debug.txt", "unrelated_success_does_not_clear_blocked_request ... ok", "cancel_overlap_policy_is_explicit_and_fail_closed ... ok"),
    "A-040": witness("stage7a-check.txt", "stage7a-check: PASS"),
    "A-041": witness("bridge-debug.txt", "uncertain_provider_and_post_dispatch_crash_remain_pending ... ok"),
    "A-042": witness("stage7a-check.txt", "stage6_authority=exclusive"),
    "A-043": witness("negative.txt", "PASS add-rusqlite-authority"),
    "A-044": witness("stage7a-check.txt", "live=false"),
    "A-045": witness("bridge-debug.txt", "ack_xadd_success_before_xack_redelivery_emits_runtime_duplicate ... ok", "uncertain_provider_and_post_dispatch_crash_remain_pending ... ok"),
    "A-046": witness("bridge-debug.txt", "xautoclaim_tail_eventually_reached_with_claim_count_1_max_pages_1 ... ok"),
    "A-047": witness("closed-surface.txt", "broker_network=false", "real_orders=false"),
    "A-048": [
        ("bridge-debug.txt", ("test result: ok. 16 passed",)),
        ("bridge-release.txt", ("test result: ok. 16 passed",)),
        ("core-debug.txt", ("test result: ok. 7 passed",)),
        ("core-release.txt", ("test result: ok. 7 passed",)),
    ],
    "A-049": [
        ("workspace-tests.txt", ("test result: ok.",)),
        ("workspace-docs.txt", ("test result: ok. 58 passed",)),
        ("clippy.txt", ("Finished",)),
        ("fmt.txt", ("fmt: PASS",)),
    ],
    "A-050": witness("negative.txt", "stage7a-negative: PASS cases="),
    "A-051": witness("preseal.txt", "stage7a-preseal: PASS"),
    "A-052": witness("stage7a-check.txt", "stage7a-check: PASS"),
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    ids = [row["ID"] for row in rows]
    if ids != [f"A-{index:03d}" for index in range(1, 53)]:
        raise SystemExit("stage7a-r1-acceptance: FAIL: matrix identity drift")
    if set(ids) != set(WITNESSES):
        raise SystemExit("stage7a-r1-acceptance: FAIL: witness registry incomplete")

    evaluated = []
    for row in rows:
        references = []
        passed = True
        for filename, tokens in WITNESSES[row["ID"]]:
            path = args.artifact_dir / filename
            text = path.read_text(errors="replace") if path.is_file() else ""
            matched = [token for token in tokens if token in text]
            passed &= len(matched) == len(tokens)
            references.append({
                "artifact": filename,
                "required_tokens": list(tokens),
                "matched_tokens": matched,
            })
        evaluated.append({
            "id": row["ID"],
            "blocking": row["Blocking"] == "YES",
            "status": "PASS" if passed else "FAIL",
            "witnesses": references,
        })

    pass_count = sum(item["status"] == "PASS" for item in evaluated)
    report = {
        "schema_version": 1,
        "stage": "7A-R1",
        "acceptance_row_count": len(rows),
        "acceptance_evaluated_count": len(evaluated),
        "acceptance_pass_count": pass_count,
        "all_blocking_rows_passed": pass_count == len(rows),
        "rows": evaluated,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if pass_count != 52:
        failed = [item["id"] for item in evaluated if item["status"] != "PASS"]
        raise SystemExit(f"stage7a-r1-acceptance: FAIL rows={failed}")
    print("stage7a-r1-acceptance: PASS evaluated=52 passed=52")


if __name__ == "__main__":
    main()
