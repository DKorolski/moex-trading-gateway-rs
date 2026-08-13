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
        "d_c_negative_case_count": 25,
        "composite_readiness_implemented": True,
        "durable_pel_reconstruction": True,
        "per_boot_consumer_identity": True,
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
        "slice", "status", "accepted_stage7b_d_b_ref", "implemented_count",
        "pending_count", "stage7b_d_b_open", "stage7b_d_b_acceptance_pending",
        "stage7b_d_c_open", "stage7b_d_c_implementation_started",
        "stage7b_d_c_acceptance_pending", "b052_b053_implemented",
        "d_c_owned_rows_implemented", "composite_readiness_implemented",
        "durable_pel_reconstruction", "per_boot_consumer_identity",
        "claim_cursor_transport_only", "legacy_execution_authority_ignored",
        "redis_consumer_attached", "redis_settlement_enabled", "xack_enabled",
        "finam_post_delete", "broker_network_dispatch", "runtime_live", "real_orders",
    ):
        require(aggregate.get(key) == expected[key], f"aggregate descriptor drift: {key}")
    require(aggregate.get("negative_case_count") == 25, "aggregate negative count drift")
    require(ownership.get("accepted_stage7b_d_b_ref") == ACCEPTED_D_B, "ownership d-b ref drift")
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
    require(valid.index("admit_paper_command") < valid.index("paper_outcome"), "provider called before Stage 6 admission")
    require(valid.index("record_paper_outcome") < valid.index("settle_finalized_ack"), "ACK settled before durable outcome")
    readiness = source_block(recovery, "    pub(crate) fn validate_composite_readiness(")
    for token in ("require_lifecycle_available", "revalidate_cached_committed_seal", "validate_recovered_binding"):
        require(token in readiness, f"storage readiness missing {token}")
    require("pub fn decode_stage7a_pre_admission(" in bridge, "canonical decoder not shared")
    for test in (
        "stage7b_d_c_b052_b053_b068_b069_restart_and_old_pel",
        "stage7b_d_c_b064_storage_failure_dominates_redis_health",
        "stage7b_d_c_b065_b066_composite_readiness_requires_independent_inputs",
        "stage7b_d_c_b067_supervision_clears_normal_error_panic_and_abort",
        "stage7b_d_c_b068_each_boot_gets_new_consumer_identity",
        "stage7b_d_c_b068_new_process_boot_uuid_is_unique",
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
