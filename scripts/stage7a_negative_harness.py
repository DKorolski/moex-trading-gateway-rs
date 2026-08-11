#!/usr/bin/env python3
"""Pinned mutation harness for Stage 7A critical guards."""
from __future__ import annotations

import json
from pathlib import Path

import stage7a_check as checker


def replace_once(source: str, old: str, new: str) -> str:
    if old not in source:
        raise RuntimeError(f"mutation source token absent: {old}")
    return source.replace(old, new, 1)


def main() -> None:
    bridge = Path(checker.BRIDGE).read_text()
    cargo = Path(checker.BRIDGE_CARGO).read_text()
    core = Path(checker.CORE).read_text()
    stage5g_ack = Path(checker.STAGE5G_ACK).read_text()
    gate = Path(checker.GATE).read_text()
    bridge_cases = [
        ("add-broker-finam-dependency", bridge, cargo + '\nbroker-finam = "*"\n'),
        ("add-reqwest-dependency", bridge, cargo + '\nreqwest = "*"\n'),
        ("add-rusqlite-authority", bridge, cargo + '\nrusqlite = "*"\n'),
        ("remove-paper-namespace", bridge.replace("STAGE7A_PAPER_NAMESPACE", "REMOVED_NAMESPACE"), cargo),
        ("remove-profile", bridge.replace("pub struct Stage7aCommandProfile", "struct MissingProfile"), cargo),
        ("remove-cancel-correlation", bridge.replace("resolve_stage7a_cancel_command_context", "missing_cancel_resolver"), cargo),
        ("public-observed-at-injection", replace_once(bridge, "    fn handle_payload(\n", "    pub fn handle_payload(\n"), cargo),
        ("remove-stage6-admission", bridge.replace("admit_stage7a_paper_command", "missing_stage6_admission"), cargo),
        ("remove-stage6-outcome", bridge.replace("execute_stage6d_paper_outcome", "missing_stage6_outcome"), cargo),
        ("remove-stage7a-finalization", bridge.replace("finalize_stage7a_paper_request", "missing_stage7a_finalization"), cargo),
        ("remove-stage7a-replay-finalization", bridge.replace("finalize_stage7a_replayed_paper_request", "missing_replay_finalization"), cargo),
        ("remove-xreadgroup", bridge.replace('redis::cmd("XREADGROUP")', 'redis::cmd("REMOVED")'), cargo),
        ("remove-xautoclaim", bridge.replace('redis::cmd("XAUTOCLAIM")', 'redis::cmd("REMOVED")'), cargo),
        ("remove-xack", bridge.replace('redis::cmd("XACK")', 'redis::cmd("REMOVED")'), cargo),
        ("remove-claim-bound", bridge.replace("max_claim_pages", "removed_claim_pages"), cargo),
        ("remove-claim-cursor", bridge.replace("reply.next_stream_id", '"0-0".to_string()'), cargo),
        ("remove-supervisor", bridge.replace("pub struct Stage7aConsumerSupervisor", "struct MissingSupervisor"), cargo),
        ("remove-observability", bridge.replace("pub async fn publish_observability", "async fn removed_observability"), cargo),
        ("remove-bounded-backoff", bridge.replace("pub async fn run_bounded", "async fn removed_run_bounded"), cargo),
        ("remove-ack-fault-window", bridge.replace("Stage7aSettlementFault::AfterAckPublishBeforeXack", "Stage7aSettlementFault::None"), cargo),
        ("remove-dlq-fault-window", bridge.replace("Stage7aSettlementFault::AfterDlqPublishBeforeXack", "Stage7aSettlementFault::None"), cargo),
        ("remove-ack-publication-state", bridge.replace("ack_publications", "removed_ack_publications"), cargo),
        ("remove-real-redis-witness", bridge.replace("real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler", "removed_real_redis_witness"), cargo),
        ("remove-uncertainty-witness", bridge.replace("uncertain_provider_and_post_dispatch_crash_remain_pending", "removed_uncertainty_witness"), cargo),
        ("introduce-order-path-authority", bridge.replace("#[cfg(test)]", "fn forbidden() { let _ = OrderPathStore; }\n#[cfg(test)]", 1), cargo),
        ("introduce-http-post", bridge.replace("#[cfg(test)]", "fn forbidden() { client.post(\"/orders\"); }\n#[cfg(test)]", 1), cargo),
        ("remove-account-profile-binding", bridge.replace("account_id: BrokerAccountId", "removed_account_id: BrokerAccountId", 1), cargo),
        ("remove-publication-success-transition", bridge.replace("self.authority.mark_ack_published(&ack);", "/* removed publication state */", 1), cargo),
        ("remove-runtime-duplicate-construction", bridge.replace("duplicate_ack_envelope", "removed_duplicate_ack_envelope"), cargo),
        ("remove-persistent-claim-cursor", bridge.replace("self.claim_cursor = next.clone();", "let _ = next.clone();", 1), cargo),
        ("remove-exact-blocked-entry-map", bridge.replace("blocked_entries", "removed_blocked_entries"), cargo),
        ("source-poll-clears-blocked", bridge.replace("self.source_read_healthy = true;", "self.source_read_healthy = true; self.blocked_entries.clear();"), cargo),
        ("group-attach-forges-source-health", bridge.replace("pub fn mark_group_attached(&mut self) {\n        self.task_readiness.mark_started();", "pub fn mark_group_attached(&mut self) {\n        self.task_readiness.mark_started(); self.source_read_healthy = true;", 1), cargo),
        ("remove-independent-source-freshness", bridge.replace("last_successful_source_poll_at", "removed_source_freshness"), cargo),
        ("remove-independent-claim-freshness", bridge.replace("last_successful_claim_scan_at", "removed_claim_freshness"), cargo),
        ("remove-external-task-supervision", bridge.replace("pub fn spawn_stage7a_supervised_task", "fn removed_task_supervision"), cargo),
        ("remove-fault-matrix-authority-witness", bridge.replace("fault_matrix_authority_windows_f02_f04_f05_are_fail_closed", "removed_fault_matrix_authority"), cargo),
        ("remove-fresh-truth-closed-surface", bridge.replace("STAGE7A_CONSTRUCTS_FRESH_BROKER_TRUTH: bool = false", "STAGE7A_CONSTRUCTS_FRESH_BROKER_TRUTH: bool = true"), cargo),
        ("remove-canonical-ack-recovery", bridge.replace("canonical_ack_recoveries", "removed_ack_recoveries"), cargo),
        ("remove-finalized-before-ack-fault", bridge.replace("AfterRequestFinalizedBeforeAckMemory", "RemovedFinalizationFault"), cargo),
        ("remove-claim-outage-fault", bridge.replace("RedisClaimOutage", "RemovedClaimOutage"), cargo),
    ]
    core_cases = [
        ("restore-dispatch-forbidden-terminality", core.replace("|| request.final_disposition().is_some()", "|| request.dispatch_safety_state() == crate::Stage6DispatchSafetyStateV1::DispatchForbidden", 1)),
        ("restore-action-specific-overlap", core.replace("            same_scope\n", "            if candidate.action() == Stage6DurableActionKind::Cancel { false } else { same_scope }\n", 1)),
        ("remove-limit-pending-guard-witness", core.replace("stage7a_limit_pending_blocks_second_new_place", "removed_limit_pending_guard_witness")),
        ("remove-linked-cancel-guard-witness", core.replace("stage7a_nonfinal_place_blocks_source_correlated_cancel", "removed_linked_cancel_guard_witness")),
        ("remove-core-stage7a-finalization", core.replace("pub fn finalize_stage7a_paper_request(", "fn removed_stage7a_paper_request(")),
        ("remove-core-replay-finalization", core.replace("pub fn finalize_stage7a_replayed_paper_request(", "fn removed_replay_finalization(")),
    ]
    stage5g_cases = [
        (
            "remove-direct-stage5g-recovered-ack-oracle",
            stage5g_ack.replace(
                "stage7a_recovered_canonical_ack_resolves_and_subsequent_duplicate_is_noop",
                "removed_stage7a_recovered_ack_oracle",
            ),
        ),
    ]
    gate_cases = [
        (
            "remove-inherited-stage6e-r1-gate",
            gate.replace("stage6e_r1_gate.sh", "removed_stage6e_r1_gate.sh"),
        ),
    ]
    descriptor = json.loads(Path(checker.DESCRIPTOR).read_text())
    checker.validate_descriptor(descriptor)
    expected = descriptor["negative_case_count"]
    actual_inventory = len(bridge_cases) + len(core_cases) + len(stage5g_cases) + len(gate_cases)
    if actual_inventory != expected:
        raise SystemExit(
            "stage7a-negative: FAIL pinned inventory "
            f"descriptor={expected} actual={actual_inventory}"
        )
    mutated_descriptor = dict(descriptor)
    mutated_descriptor["negative_case_count"] = expected - 1
    try:
        checker.validate_descriptor(mutated_descriptor)
    except checker.CheckFailure:
        print(f"PIN negative-case-count={expected}")
    else:
        raise SystemExit("stage7a-negative: FAIL descriptor count mutation survived")
    passed = 0
    for name, mutated_source, mutated_cargo in bridge_cases:
        try:
            checker.validate_bridge(mutated_source, mutated_cargo)
        except (checker.CheckFailure, ValueError):
            passed += 1
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage7a-negative: FAIL mutation survived: {name}")
    for name, mutated_core in core_cases:
        try:
            checker.validate_core(mutated_core)
        except (checker.CheckFailure, ValueError):
            passed += 1
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage7a-negative: FAIL mutation survived: {name}")
    for name, mutated_stage5g in stage5g_cases:
        try:
            checker.validate_stage5g_ack_oracle(mutated_stage5g)
        except (checker.CheckFailure, ValueError):
            passed += 1
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage7a-negative: FAIL mutation survived: {name}")
    for name, mutated_gate in gate_cases:
        try:
            checker.validate_gate(mutated_gate)
        except (checker.CheckFailure, ValueError):
            passed += 1
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage7a-negative: FAIL mutation survived: {name}")
    if passed != expected:
        raise SystemExit(f"stage7a-negative: FAIL count={passed}")
    print(f"stage7a-negative: PASS cases={passed}")


if __name__ == "__main__":
    main()
