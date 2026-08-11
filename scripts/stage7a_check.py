#!/usr/bin/env python3
"""Static architecture, lineage and governance checks for Stage 7A."""
from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path

BASE = "10e357825a701193d964975bb5769bd0745d4986"
R1_PREDECESSOR = "6e53f5428f7f79f3c9c84fbbd15d32b3c26d5d2d"
R2_PREDECESSOR = "ac8fa7f2f3ff42ae1b351c298ff0b3abd62599b5"
BRANCH = "stage7a-paper-command-consumer"
BRIDGE = Path("crates/runtime-command-bridge/src/lib.rs")
BRIDGE_CARGO = Path("crates/runtime-command-bridge/Cargo.toml")
CORE = Path("crates/strategy-runtime-core/src/stage6d_live_core.rs")
DESCRIPTOR = Path("docs/stage-7/stage7a-entry-descriptor.json")
MATRIX = Path("docs/stage-7/STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv")
TZ = Path("docs/stage-7/TZ_STAGE7A_REDIS_COMMAND_CONSUMER_PAPER_MOCK_2026-08-11.md")

TZ_SHA256 = "812da33d1917cbe3408392f81c5be50ff63a07f5173eff93258316e29c5c9d4e"
MATRIX_SHA256 = "e80ff88da6be58ec1660f0c5cc43afc5aef84a99f3c952c9a56aa7b6243677bb"

UNCHANGED_STAGE6 = (
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
)


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening:index + 1]
    raise CheckFailure(f"unterminated block: {needle}")


def validate_bridge(source: str, cargo: str) -> None:
    production = source.split("#[cfg(test)]", 1)[0]
    for dependency in ("broker-finam", "finam-gateway", "reqwest", "rusqlite"):
        require(dependency not in cargo, f"forbidden dependency: {dependency}")
    for token in (
        "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "OrderPathStore", "M3eCommandLifecycleStore", "M3hRuntimeDryCommandEmitter",
    ):
        require(token not in production, f"forbidden execution authority: {token}")
    required = (
        "STAGE7A_PAPER_NAMESPACE",
        "finam_imoexf_paper:runtime:commands",
        "pub struct Stage7aCommandProfile",
        "resolve_stage7a_cancel_command_context",
        "pub fn handle_payload_now(",
        "fn handle_payload(",
        "admit_stage7a_paper_command",
        "execute_stage6d_paper_outcome",
        "finalize_stage7a_paper_request",
        "finalize_stage7a_replayed_paper_request",
        "pub async fn ensure_group",
        'redis::cmd("XREADGROUP")',
        'redis::cmd("XAUTOCLAIM")',
        'redis::cmd("XACK")',
        "fn xautoclaim_cursor_done",
        "pub struct Stage7aConsumerSupervisor",
        "pub async fn publish_observability",
        "pub async fn run_bounded",
        "Stage7aSettlementFault::AfterAckPublishBeforeXack",
        "Stage7aSettlementFault::AfterDlqPublishBeforeXack",
        "payload_sha256",
        "allow_controlled_beginning",
        "BeginningNotAuthorized",
        "self.ack_publications",
        "mark_ack_published",
        "\n                duplicate_ack_envelope(&self.source",
        "BrokerAccountId",
        "source_read_healthy",
        "claim_scan_healthy",
        "ack_settlement_healthy",
        "dlq_settlement_healthy",
        "stage6_authority_healthy",
        "\n    blocked_entries: BTreeMap<String, Stage7aBlockedEntry>",
        "claim_cursor",
        "STAGE7A_CONSTRUCTS_FRESH_BROKER_TRUTH: bool = false",
        'STAGE7A_FRESH_TRUTH_TEMPORAL_POLICY: &str = "not_applicable_closed_surface"',
        "last_successful_source_poll_at",
        "last_successful_claim_scan_at",
        "pub fn spawn_stage7a_supervised_task",
    )
    for token in required:
        require(token in production, f"required Stage 7A token absent: {token}")
    require("pub fn handle_payload(" not in production, "caller can inject local observed_at")

    handler = block(source, "fn handle_payload(")
    require(
        handler.index("self.profile.context_for")
        < handler.index("admit_stage7a_paper_command"),
        "profile/admission ordering drift",
    )
    dispatch_branch = block(handler, "Stage7aPaperAdmission::DispatchReady")
    handler_order = ("self.provider.paper_outcome", "execute_stage6d_paper_outcome", "ack_envelope")
    positions = [dispatch_branch.index(token) for token in handler_order]
    require(positions == sorted(positions), "canonical authority ordering drift")

    settle = block(source, "async fn settle_entry(")
    ack_start = settle.index("Stage7aHandleOutcome::Ack")
    dlq_start = settle.index("Stage7aHandleOutcome::Dlq", ack_start)
    pending_start = settle.index("Stage7aHandleOutcome::Pending", dlq_start)
    ack_branch = settle[ack_start:dlq_start]
    dlq_branch = settle[dlq_start:pending_start]
    require(ack_branch.index(".publish(") < ack_branch.index("self.xack"), "ACK XACK ordering drift")
    require(dlq_branch.index(".publish(") < dlq_branch.index("self.xack"), "DLQ XACK ordering drift")
    pending = settle[pending_start:]
    require("self.xack" not in pending, "pending path XACKs uncertainty")
    require(
        ack_branch.index(".publish(")
        < ack_branch.index("self.authority.mark_ack_published")
        < ack_branch.index("self.xack"),
        "ACK publication state ordering drift",
    )
    source_success = block(source, "pub fn mark_source_poll_success(")
    for forbidden in ("blocked_entries.clear", "ack_settlement_healthy", "dlq_settlement_healthy"):
        require(forbidden not in source_success, f"source poll heals settlement state: {forbidden}")
    group_attach = block(source, "pub fn mark_group_attached(")
    for forbidden in ("source_read_healthy = true", "claim_scan_healthy = true"):
        require(forbidden not in group_attach, f"group attach forges operation health: {forbidden}")
    profile = block(source, "pub struct Stage7aCommandProfile")
    require("\n    account_id: BrokerAccountId," in profile, "trusted profile is not account-bound")

    read = block(source, "async fn poll_new_once_inner(")
    claim = block(source, "async fn reclaim_stale_once_inner(")
    require("self.settle_entry" in read, "XREADGROUP bypasses canonical handler")
    require("self.settle_entry" in claim, "XAUTOCLAIM bypasses canonical handler")
    for token in ("max_claim_pages", "reply.next_stream_id", "self.claim_cursor = next.clone()", "xautoclaim_cursor_done"):
        require(token in claim, f"claim cursor/bound absent: {token}")

    test_names = [line for line in source.splitlines() if line.startswith("    fn ") or line.startswith("    async fn ")]
    witnesses = (
        "accepted_ack_then_runtime_duplicate_is_stage5g_noop",
        "real_redis_xreadgroup_ack_and_xautoclaim_share_canonical_handler",
        "ack_xadd_success_before_xack_redelivery_emits_runtime_duplicate",
        "ack_xadd_failure_redelivery_republishes_canonical_accepted",
        "dlq_outage_empty_polls_do_not_restore_readiness",
        "xautoclaim_tail_eventually_reached_with_claim_count_1_max_pages_1",
        "unrelated_success_does_not_clear_blocked_request",
        "strict_max_one_lifecycle_finalizes_place_before_cancel",
        "source_and_claim_freshness_are_independent_readiness_authorities",
        "external_supervisor_observes_normal_error_and_panic_task_death",
        "fault_matrix_authority_windows_f02_f04_f05_f06_are_fail_closed",
        "fault_matrix_f01_source_read_before_decode_retains_pending",
        "fault_matrix_f07_f09_f14_ack_windows_recover_without_second_effect",
        "fault_matrix_f10_f11_f15_dlq_windows_recover_without_stage6_effect",
        "fault_matrix_f13_source_outage_is_bounded_and_never_stale_ready",
        "uncertain_provider_and_post_dispatch_crash_remain_pending",
        "market_limit_and_cancel_use_one_profile_without_redis_identity_authority",
        "envelope_policy_and_ttl_fail_before_paper_effect",
        "stop_shape_and_profile_drift_cannot_reach_provider",
        "supervisor_never_leaves_stale_ready_after_failure_or_stop",
    )
    for witness in witnesses:
        require(any(f"fn {witness}(" in line for line in test_names), f"test witness absent: {witness}")


def validate_core(source: str) -> None:
    production = source.split("#[cfg(test)]", 1)[0]
    for token in (
        "pub fn admit_stage7a_paper_command(",
        "pub fn resolve_stage7a_cancel_command_context(",
        "Stage7aPaperAdmission::DispatchReady",
        "Stage7aPaperHoldReason::ConflictingDuplicate",
        "Stage7aPaperHoldReason::AnotherLifecycleUnresolved",
        "Stage7aPaperHoldReason::ReconciliationRequired",
        "prepare_stage6d_existing_accepted_paper_dispatch",
        "pub fn finalize_stage7a_paper_request(",
        "pub fn finalize_stage7a_replayed_paper_request(",
        "Stage6LifecycleSequence::new(1)",
    ):
        require(token in production, f"Stage 6 admission token absent: {token}")
    for token in ("redis::", "XREADGROUP", "XAUTOCLAIM", "reqwest"):
        require(token not in production, f"transport leaked into Stage 6 authority: {token}")
    tests = source.split("#[cfg(test)]", 1)[1]
    test_count = sum(line.startswith("    fn stage7a_") for line in tests.splitlines())
    require(test_count == 8, f"Stage 6 admission test count drift: {test_count}")
    lifecycle_guard = block(source, "fn stage7a_has_other_unresolved_lifecycle(")
    for token in (
        "request.final_disposition().is_some()",
        "same_scope",
    ):
        require(token in lifecycle_guard, f"Stage 7A-R2 lifecycle guard absent: {token}")
    for token in ("is_source_correlated_cancel", "permits exactly one reviewed overlap"):
        require(token not in lifecycle_guard, f"Stage 7A-R2 overlap exception remains: {token}")
    require("candidate.action()" not in lifecycle_guard, "action-specific lifecycle overlap restored")
    for token in (
        "stage7a_limit_pending_blocks_second_new_place",
        "stage7a_market_filled_nonfinal_blocks_second_new_place",
        "stage7a_broker_order_found_nonfinal_blocks_second_new_place",
        "stage7a_nonfinal_place_blocks_source_correlated_cancel",
    ):
        require(token in tests, f"Stage 7A-R1 lifecycle witness absent: {token}")


def check(root: Path) -> None:
    require(subprocess.check_output(["git", "merge-base", "HEAD", BASE], cwd=root, text=True).strip() == BASE,
            "branch is not based on accepted Stage 6 closure")
    require(subprocess.check_output(["git", "branch", "--show-current"], cwd=root, text=True).strip() == BRANCH,
            "wrong Stage 7A branch")
    for path in UNCHANGED_STAGE6:
        current = (root / path).read_bytes()
        accepted = subprocess.check_output(["git", "show", f"{BASE}:{path}"], cwd=root)
        require(current == accepted, f"accepted Stage 6 authority changed: {path}")

    validate_bridge((root / BRIDGE).read_text(), (root / BRIDGE_CARGO).read_text())
    validate_core((root / CORE).read_text())

    require(hashlib.sha256((root / TZ).read_bytes()).hexdigest() == TZ_SHA256, "Stage 7A TZ digest drift")
    require(hashlib.sha256((root / MATRIX).read_bytes()).hexdigest() == MATRIX_SHA256, "Stage 7A matrix digest drift")
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 52, f"acceptance row count drift: {len(rows)}")
    require([row["ID"] for row in rows] == [f"A-{index:03d}" for index in range(1, 53)], "acceptance IDs drift")
    require(all(row["Blocking"] == "YES" for row in rows), "nonblocking acceptance row introduced")

    descriptor = json.loads((root / DESCRIPTOR).read_text())
    expected = {
        "stage": "7A",
        "status": "r2_implementation_candidate",
        "accepted_predecessor": BASE,
        "r1_predecessor": R1_PREDECESSOR,
        "r2_predecessor": R2_PREDECESSOR,
        "blocking_acceptance_rows": 52,
        "stage6_execution_authority": "exclusive",
        "max_unresolved_lifecycles_per_strategy_instance": 1,
        "runtime_command_bridge_crate": True,
        "canonical_handler_for_read_and_claim": True,
        "real_redis_integration": True,
        "focused_runtime_bridge_test_count": 23,
        "focused_stage6_admission_test_count": 8,
        "fault_matrix_count": 15,
        "semantic_proof_map_count": 52,
        "cross_process_exactly_once_claimed": False,
        "stage7b_open": False,
        "finam_post_delete_open": False,
        "broker_network_dispatch_open": False,
        "runtime_live_open": False,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"descriptor drift: {key}")

    docs = "\n".join((root / path).read_text() for path in (
        Path("docs/current-status.md"), Path("docs/roadmap.md"),
        Path("docs/reviewer-onboarding-and-roadmap.md"),
        Path("docs/stage-7/stage7a-implementation.md"),
    ))
    for token in ("Stage 6 is CLOSED", "Stage 7A", "Stage 7B", "runtime-live", "CLOSED"):
        require(token in docs, f"governance token absent: {token}")


def main() -> None:
    try:
        check(Path.cwd().resolve())
    except (CheckFailure, subprocess.CalledProcessError, ValueError, KeyError) as error:
        raise SystemExit(f"stage7a-check: FAIL: {error}") from error
    print("stage7a-check: PASS rows=52 stage6_authority=exclusive real_redis=true live=false")
    print("fresh_truth_provider_surface_absent=true temporal_policy=not_applicable_closed_surface")


if __name__ == "__main__":
    main()
