#!/usr/bin/env python3
"""Pinned mutation harness for Stage 7A critical guards."""
from __future__ import annotations

from pathlib import Path

import stage7a_check as checker


def replace_once(source: str, old: str, new: str) -> str:
    if old not in source:
        raise RuntimeError(f"mutation source token absent: {old}")
    return source.replace(old, new, 1)


def main() -> None:
    bridge = Path(checker.BRIDGE).read_text()
    cargo = Path(checker.BRIDGE_CARGO).read_text()
    cases = [
        ("add-broker-finam-dependency", bridge, cargo + '\nbroker-finam = "*"\n'),
        ("add-reqwest-dependency", bridge, cargo + '\nreqwest = "*"\n'),
        ("add-rusqlite-authority", bridge, cargo + '\nrusqlite = "*"\n'),
        ("remove-paper-namespace", bridge.replace("STAGE7A_PAPER_NAMESPACE", "REMOVED_NAMESPACE"), cargo),
        ("remove-profile", bridge.replace("pub struct Stage7aCommandProfile", "struct MissingProfile"), cargo),
        ("remove-cancel-correlation", bridge.replace("resolve_stage7a_cancel_command_context", "missing_cancel_resolver"), cargo),
        ("public-observed-at-injection", replace_once(bridge, "    fn handle_payload(\n", "    pub fn handle_payload(\n"), cargo),
        ("remove-stage6-admission", bridge.replace("admit_stage7a_paper_command", "missing_stage6_admission"), cargo),
        ("remove-stage6-outcome", bridge.replace("execute_stage6d_paper_outcome", "missing_stage6_outcome"), cargo),
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
        ("remove-identical-ack-cache", bridge.replace("settled_acks", "removed_ack_cache"), cargo),
        ("remove-real-redis-witness", bridge.replace("real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler", "removed_real_redis_witness"), cargo),
        ("remove-uncertainty-witness", bridge.replace("uncertain_provider_and_post_dispatch_crash_remain_pending", "removed_uncertainty_witness"), cargo),
        ("introduce-order-path-authority", bridge.replace("#[cfg(test)]", "fn forbidden() { let _ = OrderPathStore; }\n#[cfg(test)]", 1), cargo),
        ("introduce-http-post", bridge.replace("#[cfg(test)]", "fn forbidden() { client.post(\"/orders\"); }\n#[cfg(test)]", 1), cargo),
    ]
    passed = 0
    for name, mutated_source, mutated_cargo in cases:
        try:
            checker.validate_bridge(mutated_source, mutated_cargo)
        except (checker.CheckFailure, ValueError):
            passed += 1
            print(f"PASS {name}")
        else:
            raise SystemExit(f"stage7a-negative: FAIL mutation survived: {name}")
    if passed != 24:
        raise SystemExit(f"stage7a-negative: FAIL count={passed}")
    print(f"stage7a-negative: PASS cases={passed}")


if __name__ == "__main__":
    main()
