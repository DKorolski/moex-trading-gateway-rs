#!/usr/bin/env python3
"""Stage 7B-d-b atomic Redis settlement acceptance checker."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_D_A = "8418cfb63ecee6702bf8a2873592b7cad1e711ee"
DESIGN_R1 = "00cead2989493b44e0d86ead29b95d57a7fbcbe2"
BRANCH = "stage7b-production-durability"
OWNED = {f"B-{value:03d}" for value in range(57, 64)}


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
        ["git", "merge-base", "HEAD", ACCEPTED_D_A], cwd=ROOT, text=True
    ).strip()
    require(merge_base == ACCEPTED_D_A, "candidate is not based on accepted d-a-R1")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=ROOT, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-d-b branch drift")


def check_production_scope() -> None:
    changed = set(
        subprocess.check_output(
            ["git", "diff", "--name-only", ACCEPTED_D_A, "--"], cwd=ROOT, text=True
        ).splitlines()
    )
    allowed = {
        "Cargo.lock",
        "crates/runtime-durable-service/Cargo.toml",
        "crates/runtime-durable-service/src/recovery.rs",
        "crates/runtime-durable-service/src/recovery/redis_settlement.rs",
    }
    production = {
        path
        for path in changed
        if path in {"Cargo.toml", "Cargo.lock"} or path.startswith("crates/")
    }
    require(production <= allowed, f"d-b production scope expanded: {sorted(production - allowed)}")
    for prefix in (
        "crates/finam-client/",
        "crates/finam-gateway/",
        "crates/runtime-command-bridge/",
        ".github/workflows/",
    ):
        require(not any(path.startswith(prefix) for path in changed), f"closed surface changed: {prefix}")


def check_descriptors() -> None:
    descriptor = json.loads((ROOT / "docs/stage-7/stage7b-d-entry-descriptor.json").read_text())
    aggregate = json.loads((ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text())
    ownership = json.loads((ROOT / "docs/stage-7/stage7b-d-row-ownership.json").read_text())
    expected = {
        "slice": "7B-d-b",
        "status": "implementation_candidate",
        "accepted_design_r1_ref": DESIGN_R1,
        "accepted_stage7b_d_a_ref": ACCEPTED_D_A,
        "implemented_count": 61,
        "pending_count": 19,
        "stage7b_d_a_acceptance_pending": False,
        "stage7b_d_b_open": True,
        "stage7b_d_b_implementation_started": True,
        "stage7b_d_b_acceptance_pending": True,
        "stage7b_d_c_open": False,
        "d_b_owned_rows_implemented": True,
        "b052_b053_implemented": False,
        "d_b_negative_case_count": 26,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": True,
        "xack_enabled": True,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"d-b descriptor drift: {key}")
    aggregate_expected = {
        "slice": "7B-d-b",
        "status": "implementation_candidate",
        "accepted_stage7b_d_a_ref": ACCEPTED_D_A,
        "implemented_count": 61,
        "pending_count": 19,
        "stage7b_d_a_acceptance_pending": False,
        "stage7b_d_b_open": True,
        "stage7b_d_b_implementation_started": True,
        "stage7b_d_b_acceptance_pending": True,
        "stage7b_d_c_open": False,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": True,
        "xack_enabled": True,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    for key, value in aggregate_expected.items():
        require(aggregate.get(key) == value, f"aggregate descriptor drift: {key}")
    require(ownership.get("accepted_stage7b_d_a_ref") == ACCEPTED_D_A, "ownership d-a ref drift")
    require(ownership.get("implemented_rows") == 61, "ownership implemented count drift")
    require(ownership.get("pending_rows") == 19, "ownership pending count drift")
    require(ownership.get("d_b_owned_rows_implemented") is True, "d-b ownership not closed")
    require(ownership.get("b052_b053_implemented") is False, "B-052/B-053 closed early")


def check_proof_map() -> None:
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    proof_map = json.loads((ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text())
    require(proof_map["slice"] == "7B-d-b", "proof-map slice drift")
    require(proof_map["implemented_count"] == 61, "proof-map implemented count drift")
    require(proof_map["pending_count"] == 19, "proof-map pending count drift")
    rows = {row["row_id"]: row for row in proof_map["proofs"]}
    for row_id in OWNED:
        require(rows[row_id]["status"] == "implemented", f"d-b row pending: {row_id}")
        require(rows[row_id]["exact_witness"] != "pending", f"d-b witness absent: {row_id}")
    for row_id in ("B-052", "B-053"):
        require(rows[row_id]["status"] == "pending", f"{row_id} closed before d-c restart proof")
        require(rows[row_id]["proof_type"] == "real_redis_restart", f"{row_id} proof type drift")
    for value in range(64, 71):
        require(rows[f"B-{value:03d}"]["status"] == "pending", "d-c row closed early")


def check_source() -> None:
    recovery = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    settlement = (ROOT / "crates/runtime-durable-service/src/recovery/redis_settlement.rs").read_text()
    production = settlement.split("#[cfg(test)]", 1)[0]
    manifest = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text()
    require("redis.workspace = true" in manifest, "Redis dependency absent")
    for forbidden in ("reqwest", "broker-finam", "finam-gateway"):
        require(forbidden not in manifest, f"d-b forbidden dependency: {forbidden}")

    for token in (
        "const ATOMIC_SETTLEMENT_LUA",
        "redis::cmd(\"EVAL\")",
        "pub(super) struct Stage7bRedisAckSettlementPlan",
        "pub(super) struct Stage7bPoisonDlqAuthorized",
        "pub(crate) struct Stage7bPreStage6PoisonObservation",
        "pub(crate) struct Stage7bRedisSettlementBackend",
        "Stage7bRedisSettlementError::ResponseLostAfterCommit",
        "STAGE7B_SOURCE_NOT_PENDING",
        "STAGE7B_CONFLICT_REQUEST_MARKER",
        "finam_imoexf_paper:",
    ):
        require(token in settlement, f"d-b source invariant absent: {token}")

    lua = settlement.split('r#"', 1)[1].split('"#;', 1)[0]
    require(lua.count("redis.call('XADD'") == 1, "Lua must contain exactly one XADD")
    require(lua.count("redis.call('XACK'") == 1, "Lua must contain exactly one XACK")
    first_write = lua.index("redis.call('XADD'")
    for token in (
        "STAGE7B_SCHEMA",
        "STAGE7B_KIND",
        "STAGE7B_SOURCE_TYPE",
        "STAGE7B_OUTPUT_TYPE",
        "STAGE7B_ENTRY_MARKER_TYPE",
        "STAGE7B_REQUEST_MARKER_TYPE",
        "STAGE7B_ENTRY_MARKER_INVALID",
        "STAGE7B_CONFLICT_ENTRY_FINGERPRINT",
        "STAGE7B_CONFLICT_REQUEST_MARKER",
        "STAGE7B_SOURCE_NOT_PENDING",
    ):
        require(lua.index(token) < first_write, f"Lua validation moved after first write: {token}")
    require(lua.index("local existing_entry") < lua.index("local pending"), "committed retry now requires PEL")
    require(lua.index("local pending") < first_write, "new settlement no longer requires PEL")
    require(lua.index("redis.call('XACK'") > first_write, "XACK moved before publication")

    entry_key = source_block(settlement, "    fn entry_marker_key(")
    for forbidden in ("fingerprint", "payload", "canonical"):
        require(forbidden not in entry_key, f"stable entry key uses {forbidden}")
    for token in ("self.source_stream", "self.consumer_group", "self.redis_entry_id", "kind.as_str()"):
        require(token in entry_key, f"stable entry key missing {token}")
    validate = source_block(settlement, "    fn validate(&self)")
    require('format!("{{{}}}", self.hash_tag)' in validate, "single hash-slot validation absent")
    require("stream.matches('{').count() != 1" in validate, "multiple hash tags not rejected")

    for authority in ("Stage7bRedisAckSettlementPlan", "Stage7bPoisonDlqAuthorized"):
        position = settlement.index(f"struct {authority}")
        prefix = settlement[max(0, position - 100) : position]
        require("#[derive(Clone" not in prefix, f"linear authority became Clone: {authority}")
    for forbidden in (
        "Serialize for Stage7bRedisAckSettlementPlan",
        "Deserialize for Stage7bRedisAckSettlementPlan",
        "Serialize for Stage7bPoisonDlqAuthorized",
        "Deserialize for Stage7bPoisonDlqAuthorized",
    ):
        require(forbidden not in settlement, f"settlement authority escaped: {forbidden}")
    dlq_payload = source_block(settlement, "struct Stage7bDlqPayload")
    require("redacted_payload_sha256" in dlq_payload, "DLQ redacted payload digest absent")
    require("raw_payload" not in dlq_payload, "raw payload entered DLQ schema")
    poison_authorize = source_block(settlement, "pub(super) fn authorize_poison(")
    for token in (
        "poison_reason_token(poison_reason)",
        "observation.payload_len != raw_payload.len()",
        "observation.redacted_payload_sha256 != sha256_hex(raw_payload)",
        "observation.stage6_checkpoint_sha256 != current_stage6_checkpoint_sha256",
    ):
        require(token in poison_authorize, f"poison proof weakened: {token}")
    require(
        "ack_plan(\n    authority: Stage7bDurableAckAuthorized" in settlement,
        "ACK plan no longer requires d-a authority",
    )
    require(
        "dlq_plan(\n    authority: Stage7bPoisonDlqAuthorized" in settlement,
        "DLQ plan no longer requires separate poison authority",
    )

    owner_ack = source_block(recovery, "    pub(crate) async fn settle_finalized_ack(")
    ordered = (
        "self.authorize_finalized_ack(finalized, commitment_key)?",
        "redis_settlement::ack_plan(authority, context)?",
        "backend.settle_ack(plan).await?",
    )
    positions = [owner_ack.index(token) for token in ordered]
    require(positions == sorted(positions), "owner ACK authority/plan/settlement ordering drift")
    observe = source_block(recovery, "    pub(crate) fn observe_pre_stage6_poison(")
    settle_poison = source_block(recovery, "    pub(crate) async fn settle_pre_stage6_poison(")
    for block, name in ((observe, "poison observation"), (settle_poison, "poison settlement")):
        require("self.revalidate_cached_committed_seal(commitment_key)?" in block, f"{name} disk seal check absent")
        require("refresh_stage7b_durable_frontier" in block, f"{name} frontier refresh absent")
    require("authorize_poison(" in settle_poison and "settle_dlq" in settle_poison, "poison authority chain drift")
    for forbidden in ("IdentityConflict", "ConflictingDuplicate", "ReconciliationRequired", "RecoveryBlocked"):
        require(forbidden not in production, f"hold state entered settlement module: {forbidden}")
    for forbidden in ("XREADGROUP", "XAUTOCLAIM", "consumer task", "PaperReady"):
        require(forbidden not in production, f"d-c consumer/readiness surface opened: {forbidden}")

    for test_name in (
        "stage7b_d_b_b057_atomic_ack_xadd_marker_and_xack",
        "stage7b_d_b_b057_b062_owner_mediates_only_finalized_ack_settlement",
        "stage7b_d_b_b058_stable_transport_identity_never_uses_payload_fingerprint",
        "stage7b_d_b_b059_response_loss_exact_retry_is_idempotent",
        "stage7b_d_b_b060_precommit_failure_keeps_pel_and_degrades_backend",
        "stage7b_d_b_b061_poison_dlq_is_redacted_atomic_and_checkpoint_bound",
        "stage7b_d_b_later_exact_entry_is_duplicate_and_conflict_stays_pending",
        "stage7b_d_b_new_settlement_requires_expected_pel_before_mutation",
    ):
        require(test_name in recovery or test_name in settlement, f"d-b test absent: {test_name}")


def check_docs() -> None:
    implementation = (ROOT / "docs/stage-7/stage7b-d-b-implementation.md").read_text()
    status = (ROOT / "docs/current-status.md").read_text()
    roadmap = (ROOT / "docs/roadmap.md").read_text()
    for token in ("B-057..B-063", "Stage 7B-d-c remains", "FINAM POST/DELETE"):
        require(token in implementation, f"d-b implementation document drift: {token}")
    require("Stage 7B-d-b is the active implementation candidate" in status, "current status not updated")
    require("Stage 7B-d-b — active implementation candidate" in roadmap, "roadmap not updated")


def main() -> None:
    check_lineage()
    check_production_scope()
    check_descriptors()
    check_proof_map()
    check_source()
    check_docs()
    print("stage7b-d-b-check: PASS rows=7 implemented=61 pending=19 consumer=false d_c=false")


if __name__ == "__main__":
    main()
