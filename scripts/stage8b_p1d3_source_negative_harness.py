#!/usr/bin/env python3
"""Targeted in-memory mutations for the P1-d3 source contract."""

from __future__ import annotations

import copy
import json

import stage8b_p1d3_source_check as checker


def rejected(name: str, mutate) -> None:
    content = copy.deepcopy(checker.load_content())
    mutate(content)
    try:
        checker.validate_content(content)
    except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"FAIL mutation accepted: {name}")


def replace(key: str, old: str, new: str, *, all_occurrences: bool = False):
    def mutate(content: dict[str, str]) -> None:
        if old not in content[key]:
            raise SystemExit(f"harness source token missing: {key}:{old}")
        content[key] = content[key].replace(old, new, -1 if all_occurrences else 1)

    return mutate


def mutate_first_golden_byte(content: dict[str, str]) -> None:
    golden = json.loads(content["golden"])
    encoded = golden["shapes"][0]["fresh_canonical_bytes_hex"]
    golden["shapes"][0]["fresh_canonical_bytes_hex"] = ("0" if encoded[0] != "0" else "1") + encoded[1:]
    content["golden"] = json.dumps(golden)


def mutate_first_projection_byte(content: dict[str, str]) -> None:
    golden = json.loads(content["projection_golden"])
    encoded = golden["shapes"][0]["fresh"]["ack"]["canonical_bytes_hex"]
    golden["shapes"][0]["fresh"]["ack"]["canonical_bytes_hex"] = (
        "0" if encoded[0] != "0" else "1"
    ) + encoded[1:]
    content["projection_golden"] = json.dumps(golden)


def main() -> None:
    cases = [
        ("working-book-domain-drift", replace("core", "moex.stage8b.p1d3.working-book.v1", "moex.stage8b.p1d3.working-book.v2")),
        ("registry-capacity-drift", replace("core", "P1D3_MAX_ORDER_RECORDS_PER_GENERATION: usize = 1024", "P1D3_MAX_ORDER_RECORDS_PER_GENERATION: usize = 2048")),
        ("embedded-book-removed", replace("core", "records: Vec<Stage8bP1d3OrderRecordV1>", "records_removed: ()")),
        ("canonical-sort-removed", replace("core", ".as_bytes()\n                .cmp(right.broker_order_id.as_str().as_bytes())", ".cmp(right.broker_order_id.as_str())")),
        ("wall-clock-added", lambda content: content.__setitem__("core", content["core"] + "\nfn drift() { let _ = Utc::now(); }\n")),
        ("float-added", lambda content: content.__setitem__("core", content["core"] + "\nfn drift(_: f64) {}\n")),
        ("buy-touch-drift", replace("core", "OrderSide::Buy if bar.low > limit_price", "OrderSide::Buy if bar.low >= limit_price")),
        ("sell-touch-drift", replace("core", "OrderSide::Sell if bar.high < limit_price", "OrderSide::Sell if bar.high <= limit_price")),
        ("stage6-v3-removed", replace("stage6", "Stage6JournalRecordVersioned::V3(outcome_record.clone())", "Stage6JournalRecordVersioned::V2(outcome_record.clone())", all_occurrences=True)),
        ("full-evidence-decode-removed", replace("stage6", "outcome_record.outcome_evidence_bytes()", "&[]", all_occurrences=True)),
        ("cancel-restart-phase-removed", replace("stage6", "Stage6Stage8bP1d3RestartPhase::CancelContinuationPending", "Stage6Stage8bP1d3RestartPhase::TruthCommitted", all_occurrences=True)),
        ("pre-cancel-seal-barrier-removed", replace("service", "p1d3-after-recovered-cancel-before-s-cancel-recovered", "p1d3-barrier-removed", all_occurrences=True)),
        ("post-cancel-seal-barrier-removed", replace("service", "p1d3-after-s-cancel-recovered-before-source-xack", "p1d3-barrier-removed", all_occurrences=True)),
        ("order-position-pending-semantic-removed", replace("restart", "p1_semantic_commit: Option<Box<Stage5gP1SemanticCommitProjectionV1>>", "p1_semantic_commit_removed: ()", all_occurrences=True)),
        ("pending-semantic-restart-overlay-removed", replace("restart", "and_then(crate::stage5g_p1_semantic::p1_prepublication_restart_slot)", "and_then(|_| None)", all_occurrences=True)),
        ("projected-request-checkpoint-removed", replace("stage6", "projected_checkpoint_after_append", "removed_projected_checkpoint", all_occurrences=True)),
        ("request-checkpoint-rebind-removed", replace("stage6", "source.rebind_semantic_request_checkpoint(", "source.removed_semantic_request_checkpoint(", all_occurrences=True)),
        ("cancel-recovery-sigkill-proof-removed", replace("redis", "p1d3_subprocess_sigkill_brackets_s_cancel_recovered", "removed_cancel_recovery_sigkill_proof")),
        ("cancel-request-client-id-collides", replace("redis", "0xd302_0000_0000_4000_8000_0000_0000_0001", "0xd301_0000_0000_4000_8000_0000_0000_0001")),
        ("ack-early-xack-added", replace("redis", "pub fn recovery_seal_generation(&self) -> u64 {", "pub async fn acknowledge_source(&mut self) {}\n    pub fn recovery_seal_generation(&self) -> u64 {")),
        ("cancel-early-xack-added", replace("redis", "pub fn market_or_schedule_input_allowed(&self) -> bool {", "pub async fn acknowledge_source(&mut self) {}\n    pub fn market_or_schedule_input_allowed(&self) -> bool {")),
        ("truth-repeat-added", replace("redis", "impl Stage8bP1RedisLimitTruthCommitted {", "impl Stage8bP1RedisLimitTruthCommitted {\n    pub fn commit_truth(&mut self) {}")),
        ("exact-successor-read-removed", replace("redis", "exact_first_successor_m10", "removed_successor_lookup", all_occurrences=True)),
        ("working-loop-removed", replace("redis", "process_next_working_limit", "removed_working_limit_loop", all_occurrences=True)),
        ("pel-proof-removed", replace("redis", "p1d3_read_only_successor_observation_retains_original_source_until_xack", "removed_pel_proof")),
        ("compile-fail-early-xack-removed", replace("service_lib", "ack.acknowledge_source()", "ack.m10_xack_allowed()", all_occurrences=True)),
        ("acceptance-row-removed", lambda content: content.__setitem__("matrix", "\n".join(content["matrix"].splitlines()[:-1]) + "\n")),
        ("db0-opened", replace("evidence", '"operational_redis_db0": false', '"operational_redis_db0": true')),
        ("next-stage-opened", replace("evidence", '"next_stage_authorized": false', '"next_stage_authorized": true')),
        ("golden-byte-tampered", mutate_first_golden_byte),
        (
            "shared-position-reducer-removed",
            replace(
                "core",
                "crate::stage8b_p1d2_market_feedback::resulting_position(",
                "removed_p1d2_position_reducer(",
            ),
        ),
        (
            "prior-position-consistency-removed",
            replace(
                "p1d2",
                "q0 == Decimal::ZERO && a0.is_some()",
                "q0 == Decimal::ZERO && false",
            ),
        ),
        (
            "cancel-input-dcid-tcid-check-removed",
            replace(
                "core",
                "input.target_place_client_id.as_ref() == Some(&input.durable_request_client_id)",
                "false",
            ),
        ),
        (
            "cancel-evidence-dcid-tcid-check-removed",
            replace(
                "core",
                "self.durable_request_client_id.as_ref() == self.target_place_client_id.as_ref()",
                "false",
            ),
        ),
        (
            "projection-ack-removed",
            replace("core", '"ack": ack', '"ack_removed": ack'),
        ),
        (
            "projection-order-snapshot-removed",
            replace("core", '"snapshot": row,', '"snapshot_removed": row,'),
        ),
        (
            "projection-position-decimal-removed",
            replace(
                "core",
                '"unrealized_pnl": optional_decimal_bytes(row.unrealized_pnl)',
                '"unrealized_pnl": serde_json::Value::Null',
            ),
        ),
        (
            "projection-truth-order-membership-removed",
            replace("core", '"orders": exact_order_rows(Some(truth))', '"orders": []'),
        ),
        (
            "position-average-scale-eight-removed",
            replace(
                "p1d2",
                "canonical.rescale(STAGE8B_P1D2_AVG_PRICE_SCALE);",
                "canonical.rescale(0);",
            ),
        ),
        ("projection-golden-byte-tampered", mutate_first_projection_byte),
    ]
    for name, mutate in cases:
        rejected(name, mutate)
    print(f"PASS stage8b-p1d3-source-negative-harness {len(cases)}/{len(cases)}")


if __name__ == "__main__":
    main()
