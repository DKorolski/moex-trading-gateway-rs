#!/usr/bin/env python3
"""Targeted in-memory mutations for the P1-d2 source contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1d2_source_check as checker


def rejected(name: str, mutate) -> None:
    content = copy.deepcopy(checker.load_content())
    mutate(content)
    try:
        checker.validate_content(content)
    except (checker.CheckFailure, json.JSONDecodeError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"FAIL mutation accepted: {name}")


def replace(key: str, old: str, new: str, *, all_occurrences: bool = False):
    def mutate(content: dict[str, str]) -> None:
        if old not in content[key]:
            raise SystemExit(f"harness source token missing: {key}:{old}")
        content[key] = content[key].replace(old, new, -1 if all_occurrences else 1)

    return mutate


def main() -> None:
    cases = [
        ("feedback-domain-drift", replace("feedback", "moex.stage8b.p1d2.market-feedback.v1", "moex.stage8b.p1d2.market-feedback.v2")),
        ("finalized-mint-removed", replace("feedback", "mint_stage8b_p1d2_finalized_market_feedback", "removed_finalized_feedback_mint", all_occurrences=True)),
        ("terminal-disposition-check-removed", replace("feedback", "facts.final_disposition() != Stage6RequestFinalDispositionV1::Completed", "false")),
        ("trade-cardinality-check-removed", replace("feedback", "facts.broker_trade_ids().len() == 1", "true")),
        ("wall-clock-added", lambda content: content.__setitem__("feedback", content["feedback"] + "\nfn drift() { let _ = Utc::now(); }\n")),
        ("float-added", lambda content: content.__setitem__("feedback", content["feedback"] + "\nfn drift(_: f64) {}\n")),
        ("average-scale-drift", replace("feedback", "STAGE8B_P1D2_AVG_PRICE_SCALE: u32 = 8", "STAGE8B_P1D2_AVG_PRICE_SCALE: u32 = 7")),
        ("average-rounding-drift", replace("feedback", "RoundingStrategy::MidpointNearestEven", "RoundingStrategy::MidpointAwayFromZero", all_occurrences=True)),
        ("ack-restart-source-removed", replace("restart", "P1d2Ack(crate::stage8b_p1d2_market_feedback::Stage8bP1d2AckRestartSource)", "P1d2AckRemoved")),
        ("truth-restart-source-removed", replace("restart", "P1d2Truth(crate::stage8b_p1d2_market_feedback::Stage8bP1d2TruthRestartSource)", "P1d2TruthRemoved")),
        ("lifecycle-kind-drift", replace("restart", "Stage5gCleanRestartLifecycleKind::OrderPositionAwaitingCommitted", "Stage5gCleanRestartLifecycleKind::TimerReady", all_occurrences=True)),
        ("resolved-ack-slot-helper-removed", replace("order_position", "stage8b_p1d2_truth_sequence_from_ack_state", "removed_truth_sequence_from_ack", all_occurrences=True)),
        ("checked-sequence-add-removed", replace("order_position", ".checked_add(1)", ".wrapping_add(1)", all_occurrences=True)),
        ("generic-sequence-helper-added", lambda content: content.__setitem__("feedback", content["feedback"] + "\n// stage5g_restart_candidate_context\n")),
        ("schedule-authority-removed", replace("provider", "schedule_authority: Stage8bP1d1ExecutionScheduleAuthority", "schedule_authority_removed: ()", all_occurrences=True)),
        ("synthetic-schedule-reintroduced", lambda content: content.__setitem__("provider", content["provider"] + "\n// stage8b_p1d1_contiguous_schedule_projection\n")),
        ("replacement-seal-removed", replace("service", "let ready = commit_stage8b_p1_replacement_seal(", "let ready = removed_replacement_seal(", all_occurrences=True)),
        ("early-xack-added", replace("redis", "pub fn recovery_seal_generation(&self) -> u64 {", "pub async fn acknowledge_source(&mut self) {}\n    pub fn recovery_seal_generation(&self) -> u64 {")),
        ("truth-repeat-added", replace("redis", "impl Stage8bP1RedisFeedbackTruthCommitted {", "impl Stage8bP1RedisFeedbackTruthCommitted {\n    pub fn commit_truth(&mut self) {}")),
        ("exact-xack-call-removed", replace("redis", ".acknowledge_exact(&self.pending_m10)", ".resolve_source(&self.pending_m10)")),
        ("exact-successor-check-removed", replace("redis", "exact_first_successor_m10", "removed_successor_lookup", all_occurrences=True)),
        ("successor-interval-drift", replace("redis", ".checked_add(600_000)", ".checked_add(1)")),
        ("crash-matrix-removed", replace("redis", "p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers", "removed_crash_matrix")),
        ("pre-kill-pair-comparison-removed", replace("redis", "restart must recover the exact pre-kill sequence pair", "pair comparison removed")),
        ("matrix-row-removed", lambda content: content.__setitem__("matrix", "\n".join(content["matrix"].splitlines()[:-1]) + "\n")),
        ("db0-opened", replace("evidence", '"operational_redis_db0": false', '"operational_redis_db0": true')),
        ("next-stage-opened", replace("evidence", '"next_stage_authorized": false', '"next_stage_authorized": true')),
    ]
    for name, mutate in cases:
        rejected(name, mutate)
    print(f"PASS stage8b-p1d2-source-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
