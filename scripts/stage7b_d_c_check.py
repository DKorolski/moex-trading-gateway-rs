#!/usr/bin/env python3
"""Stage 7B-d-c supervised restart/readiness acceptance checker."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_D_B = "e0bf9b7d9eb209e19b875f199511a493ddcd0da9"
BRANCH = "stage7b-production-durability"
OWNED = {"B-052", "B-053", *(f"B-{value:03d}" for value in range(64, 71))}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def source_block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise CheckFailure(f"unterminated source block: {needle}")


def check_lineage() -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", ACCEPTED_D_B], cwd=ROOT, text=True
    ).strip()
    require(merge_base == ACCEPTED_D_B, "candidate is not based on accepted d-b-R1")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=ROOT, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-d-c branch drift")


def check_production_scope() -> None:
    changed = set(
        subprocess.check_output(
            ["git", "diff", "--name-only", ACCEPTED_D_B, "--"], cwd=ROOT, text=True
        ).splitlines()
    )
    allowed = {
        "Cargo.lock",
        "crates/runtime-command-bridge/src/lib.rs",
        "crates/runtime-durable-service/Cargo.toml",
        "crates/runtime-durable-service/src/lib.rs",
        "crates/runtime-durable-service/src/recovery.rs",
        "crates/runtime-durable-service/src/recovery/redis_service.rs",
        "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
        "crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs",
    }
    production = {
        path
        for path in changed
        if path in {"Cargo.toml", "Cargo.lock"} or path.startswith("crates/")
    }
    require(production <= allowed, f"d-c production scope expanded: {sorted(production - allowed)}")
    for prefix in ("crates/finam-client/", "crates/finam-gateway/", ".github/workflows/"):
        require(not any(path.startswith(prefix) for path in changed), f"closed surface changed: {prefix}")


def check_descriptors() -> None:
    descriptor = json.loads((ROOT / "docs/stage-7/stage7b-d-entry-descriptor.json").read_text())
    aggregate = json.loads((ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text())
    ownership = json.loads((ROOT / "docs/stage-7/stage7b-d-row-ownership.json").read_text())
    expected = {
        "slice": "7B-d-c",
        "status": "implementation_candidate",
        "candidate_revision": "r1",
        "rejected_stage7b_d_c_ref": "c427ad1c83a27e6a80f45c7e09311ffcae26c913",
        "accepted_stage7b_d_b_ref": ACCEPTED_D_B,
        "implemented_count": 70,
        "pending_count": 10,
        "stage7b_d_b_open": False,
        "stage7b_d_b_acceptance_pending": False,
        "stage7b_d_c_open": True,
        "stage7b_d_c_implementation_started": True,
        "stage7b_d_c_acceptance_pending": True,
        "b052_b053_implemented": True,
        "d_c_owned_rows_implemented": True,
        "d_c_negative_case_count": 33,
        "composite_readiness_implemented": True,
        "real_service_paper_ready_integration": True,
        "durable_pel_reconstruction": True,
        "per_boot_consumer_identity": True,
        "subprocess_redis_reclaim_integration": True,
        "deterministic_pre_stage6_rejection_ack": True,
        "deterministic_rejection_zero_stage6_mutation": True,
        "established_profile_mismatch_stays_pending": True,
        "claim_cursor_transport_only": True,
        "legacy_execution_authority_ignored": True,
        "redis_consumer_attached": True,
        "redis_settlement_enabled": True,
        "xack_enabled": True,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"d-c descriptor drift: {key}")
    for key in (
        "slice", "status", "candidate_revision", "rejected_stage7b_d_c_ref",
        "accepted_stage7b_d_b_ref", "implemented_count",
        "pending_count", "stage7b_d_b_open", "stage7b_d_b_acceptance_pending",
        "stage7b_d_c_open", "stage7b_d_c_implementation_started",
        "stage7b_d_c_acceptance_pending", "b052_b053_implemented",
        "d_c_owned_rows_implemented", "composite_readiness_implemented",
        "real_service_paper_ready_integration", "durable_pel_reconstruction",
        "per_boot_consumer_identity", "subprocess_redis_reclaim_integration",
        "deterministic_pre_stage6_rejection_ack",
        "deterministic_rejection_zero_stage6_mutation",
        "established_profile_mismatch_stays_pending",
        "claim_cursor_transport_only", "legacy_execution_authority_ignored",
        "redis_consumer_attached", "redis_settlement_enabled", "xack_enabled",
        "finam_post_delete", "broker_network_dispatch", "runtime_live", "real_orders",
    ):
        require(aggregate.get(key) == expected[key], f"aggregate descriptor drift: {key}")
    require(aggregate.get("negative_case_count") == 33, "aggregate negative count drift")
    require(ownership.get("accepted_stage7b_d_b_ref") == ACCEPTED_D_B, "ownership d-b ref drift")
    require(ownership.get("candidate_revision") == "r1", "ownership revision drift")
    require(
        ownership.get("rejected_stage7b_d_c_ref")
        == "c427ad1c83a27e6a80f45c7e09311ffcae26c913",
        "ownership rejected d-c ref drift",
    )
    require(ownership.get("implemented_rows") == 70, "ownership implemented count drift")
    require(ownership.get("pending_rows") == 10, "ownership pending count drift")
    require(ownership.get("b052_b053_implemented") is True, "restart rows not closed")
    require(ownership.get("d_c_owned_rows_implemented") is True, "d-c rows not closed")


def check_proof_map() -> None:
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    proof_map = json.loads((ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text())
    require(proof_map["slice"] == "7B-d-c", "proof-map slice drift")
    require(proof_map["implemented_count"] == 70, "proof-map implemented count drift")
    require(proof_map["pending_count"] == 10, "proof-map pending count drift")
    rows = {row["row_id"]: row for row in proof_map["proofs"]}
    for row_id in OWNED:
        require(rows[row_id]["status"] == "implemented", f"d-c row pending: {row_id}")
        require(not rows[row_id]["exact_witness"].startswith("pending"), f"d-c witness absent: {row_id}")


def check_source() -> None:
    service = (ROOT / "crates/runtime-durable-service/src/recovery/redis_service.rs").read_text()
    recovery = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    subprocess_test = (
        ROOT / "crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs"
    ).read_text()
    bridge = (ROOT / "crates/runtime-command-bridge/src/lib.rs").read_text()
    settlement = (
        ROOT / "crates/runtime-durable-service/src/recovery/redis_settlement.rs"
    ).read_text()
    manifest = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text().lower()
    for forbidden in ("reqwest", "broker-finam", "finam-gateway", "rusqlite", "sqlx"):
        require(forbidden not in manifest, f"d-c forbidden dependency: {forbidden}")
    for token in (
        "pub struct Stage7bRedisService<P>",
        "owner: Stage7bRecoveryReadyOwner",
        "settlement: Stage7bRedisSettlementBackend",
        "commitment_key: Stage5gLifecycleCommitmentKey",
        "profile: Stage7aCommandProfile",
        "provider: P",
        'claim_cursor: "0-0".to_string()',
        'redis::cmd("XREADGROUP")',
        'redis::cmd("XAUTOCLAIM")',
        'redis::cmd("XPENDING")',
        "Uuid::new_v4()",
        "Stage7bPaperReadinessReason::StorageUnavailable",
        "Stage7bPaperReadinessReason::SourcePollStale",
        "Stage7bPaperReadinessReason::ClaimScanStale",
        "Stage7bPaperReadinessReason::SettlementUnavailable",
        "Stage7bPaperReadinessReason::DurablePendingEntries",
        "Stage7bPaperReadinessReason::CommandLifecycleBlocked",
        "validate_composite_readiness(&self.commitment_key)",
    ):
        require(token in service, f"d-c source invariant absent: {token}")
    readiness_logic = source_block(service, "    pub fn snapshots(")
    for token in (
        "if !state.durable_storage_ready",
        "if !source_poll_fresh",
        "if !claim_scan_fresh",
        "if !state.settlement_healthy",
        "if state.durable_pending_count != 0",
        "if !state.blocked_entries.is_empty()",
    ):
        require(token in readiness_logic, f"composite readiness input absent: {token}")
    require("Stage7aCommandAuthority" not in service, "process-memory Stage7A authority reused")
    task_spawn = source_block(service, "pub fn spawn_stage7b_supervised_task")
    require("let stop_guard = Stage7bTaskStopGuard(readiness);" in task_spawn, "abort-safe guard absent")
    require(task_spawn.index("let stop_guard") < task_spawn.index("tokio::spawn"), "guard created after spawn")
    poison = source_block(service, "    async fn settle_poison(")
    require("classify_stage7a_permanent_pre_admission_poison(&entry_id, payload)" in poison, "exact poison classifier absent")
    require("observe_pre_stage6_poison" in poison, "poison not passed immediately to owner")
    require(poison.index("classify_stage7a") < poison.index("observe_pre_stage6_poison"), "poison evidence pairing order drift")
    valid = source_block(service, "    async fn process_valid_command(")
    for token in (
        "observe_pre_stage6_command",
        "classify_for_recovered",
        "Stage7aRecoveredProfileClassification::DeterministicRejection",
        "Stage7aRecoveredProfileClassification::IdentityConflict",
        "classify_stage7a_deterministic_policy_rejection",
        "settle_deterministic_rejection",
    ):
        require(token in valid, f"deterministic rejection path absent: {token}")
    require(
        valid.index("observe_pre_stage6_command")
        < valid.index("classify_for_recovered")
        < valid.index("admit_paper_command"),
        "pre-Stage6 observation/classification ordering drift",
    )
    require(valid.index("admit_paper_command") < valid.index("paper_outcome"), "provider called before Stage 6 admission")
    require(valid.index("record_paper_outcome") < valid.index("settle_finalized_ack"), "ACK settled before durable outcome")
    rejection_helper = source_block(service, "    async fn settle_deterministic_rejection(")
    require(
        "settle_pre_stage6_rejection" in rejection_helper,
        "deterministic rejection bypasses owner settlement authority",
    )
    readiness = source_block(recovery, "    pub(crate) fn validate_composite_readiness(")
    for token in ("require_lifecycle_available", "revalidate_cached_committed_seal", "validate_recovered_binding"):
        require(token in readiness, f"storage readiness missing {token}")
    observation = source_block(recovery, "    pub(crate) fn observe_pre_stage6_command(")
    for token in (
        "refresh_stage7b_durable_frontier",
        "advance_recovery_seal",
        "authenticated_checkpoint",
        ".replay().request(request_id).is_some()",
    ):
        require(token in observation, f"pre-Stage6 observation proof absent: {token}")
    rejection = source_block(recovery, "    pub(crate) async fn settle_pre_stage6_rejection(")
    for token in (
        "refresh_stage7b_durable_frontier",
        "authorize_pre_stage6_rejection",
        "pre_stage6_rejection_ack_plan",
        "backend.settle_ack(plan)",
    ):
        require(token in rejection, f"pre-Stage6 rejection settlement absent: {token}")
    authority = source_block(settlement, "pub(super) fn authorize_pre_stage6_rejection(")
    require("stage6_mutation: true" not in settlement, "Stage 6 mutation claim opened")
    for token in (
        "observation.request_identity_was_established",
        "current_request_identity_exists",
        "observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256",
        "stage6_mutation: false",
    ):
        require(token in authority, f"zero-Stage6 rejection authority absent: {token}")
    for token in (
        "pub struct Stage7aDeterministicRejectionEvidence",
        "Stage7aDeterministicRejectionClass::Expired",
        "Stage7aDeterministicRejectionClass::UnsupportedCommandShape",
        "Stage7aDeterministicRejectionClass::CommandProfileMismatch",
        "classify_stage7a_deterministic_policy_rejection",
        "Stage7aRecoveredProfileClassification::IdentityConflict",
    ):
        require(token in bridge, f"accepted Stage 7A rejection classifier absent: {token}")
    require("pub fn decode_stage7a_pre_admission(" in bridge, "canonical decoder not shared")
    for test in (
        "stage7b_d_c_b052_b053_b068_b069_restart_and_old_pel",
        "stage7b_d_c_b064_storage_failure_dominates_redis_health",
        "stage7b_d_c_b065_b066_composite_readiness_requires_independent_inputs",
        "stage7b_d_c_b067_supervision_clears_normal_error_panic_and_abort",
        "stage7b_d_c_b068_each_boot_gets_new_consumer_identity",
        "stage7b_d_c_b068_new_process_boot_uuid_is_unique",
        "stage7b_d_c_r1_deterministic_rejections_ack_without_stage6_mutation",
        "stage7b_d_c_r1_rejection_restart_is_idempotent_and_established_conflict_stays_pending",
        "stage7b_d_c_r1_b066_real_service_reports_ready_only_while_supervised_task_lives",
        "stage7b_d_c_r1_b068_fresh_process_reclaims_old_pel_with_real_redis",
        "async fn stage7b_d_c_r1_b068_subprocess_redis_reclaim_child",
        "stage7b_d_c_b070_has_no_legacy_execution_authority_dependency",
    ):
        require(
            test in service or test in recovery or test in subprocess_test,
            f"d-c witness absent: {test}",
        )
    for method in ("finam_transport_attached", "runtime_live_enabled", "real_orders_enabled"):
        block = source_block(service, f"    pub fn {method}(")
        require("false" in block and "true" not in block, f"closed surface opened: {method}")


def check_docs() -> None:
    docs = "\n".join(
        (ROOT / path).read_text()
        for path in (
            "docs/stage-7/stage7b-d-c-implementation.md",
            "docs/current-status.md",
            "docs/roadmap.md",
        )
    )
    for token in (
        "e0bf9b7d9eb209e19b875f199511a493ddcd0da9",
        "B-052/B-053",
        "B-064..B-070",
        "FINAM",
        "runtime-live",
        "real orders",
    ):
        require(token in docs, f"d-c documentation invariant absent: {token}")


def main() -> None:
    check_lineage()
    check_production_scope()
    check_descriptors()
    check_proof_map()
    check_source()
    check_docs()
    print("stage7b-d-c-check: PASS rows=9 implemented=70 pending=10")


if __name__ == "__main__":
    main()
