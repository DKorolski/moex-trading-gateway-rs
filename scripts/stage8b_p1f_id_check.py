#!/usr/bin/env python3
"""Fail-closed source checker for Stage 8B-P1-f Id."""

from __future__ import annotations

import csv
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "5c2656fbe8691da256b5380dd16ce6f6b6aa1fa8"
REVIEW_SHA256 = "ee69d58bc70288f447a9ab880d2a2eec01fefdfe6f86ce3be603eb7f5a1b30b3"
REDIS = "crates/runtime-durable-service/src/stage8b_p1f_fixed_redis.rs"
SEMANTIC = "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"
RUNTIME_LIB = "crates/runtime-durable-service/src/lib.rs"
PRODUCER = "crates/finam-gateway/src/stage8b_p1f_fixed_producers.rs"
PUBLISHER = "crates/finam-gateway/src/stage8b_p1e_schedule_publisher.rs"
GATEWAY_LIB = "crates/finam-gateway/src/lib.rs"
DOCUMENT = "docs/stage-8/stage8b-p1f-id-fixed-redis-composition.md"
INVENTORY = "docs/stage-8/stage8b-p1f-id-fixed-redis-composition.json"
MATRIX = "docs/stage-8/stage8b-p1f-id-acceptance-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
CHECKER = "scripts/stage8b_p1f_id_check.py"
NEGATIVE = "scripts/stage8b_p1f_id_negative_harness.py"
GATE = "scripts/stage8b_p1f_id_gate.sh"
HANDOFF = "scripts/make_stage8b_p1f_id_handoff.py"
HANDOFF_SAFETY = "scripts/stage8b_p1f_id_handoff_safety_check.py"
ALLOWED_CHANGES = {
    REDIS, SEMANTIC, RUNTIME_LIB, PRODUCER, PUBLISHER, GATEWAY_LIB,
    DOCUMENT, INVENTORY, MATRIX, STATUS, ROADMAP, CHECKER, NEGATIVE, GATE,
    HANDOFF, HANDOFF_SAFETY,
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path) -> dict[str, Any]:
    try:
        value = json.loads((root / INVENTORY).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read inventory: {error}") from error
    require(type(value) is dict, "inventory must be an object")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
        changed = set(subprocess.check_output(
            ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True
        ).splitlines())
        changed |= set(subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
        ).splitlines())
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify Id lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, f"Id changed-path drift: {sorted(changed ^ ALLOWED_CHANGES)}")


def validate_inventory(root: Path) -> None:
    value = read_json(root)
    require(set(value) == {
        "accepted_ic_commit", "accepted_ic_review_sha256", "audit", "closed_surfaces",
        "evidence", "next_after_acceptance", "publication", "redis", "resources",
        "schema_version", "stage", "status",
    }, "inventory key-set drift")
    require(value["schema_version"] == 1 and type(value["schema_version"]) is int, "schema drift")
    require(value["accepted_ic_commit"] == BASE, "accepted Ic binding drift")
    require(value["accepted_ic_review_sha256"] == REVIEW_SHA256, "Ic review digest drift")
    require(value["status"] == "REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION", "Id self-accepted")
    redis = value["redis"]
    require(redis["database"] == 15, "Redis DB drift")
    require(len(redis["roles"]) == 8 and len(set(redis["roles"])) == 8, "role inventory drift")
    require(len(redis["source_operations"]) == 10 and len(set(redis["source_operations"])) == 10, "operation inventory drift")
    expected_scripts = {
        "namespace-initialization-v1": "c7db9e1660bb6519ab7502b63cc695ce6d3a5f453be248b4aaad4ddbc614dd3e",
        "namespace-verify-v1": "de29c3ca22ec8eb16925a48ada22bf1dd4fd5b6bd7fa58a0438000a0923e7d49",
        "m10-publication-v1": "ecf60a82c8efdb92c2b4a13a86cd607dee46a216bdbf3c7207ea3208989baf54",
        "command-publication-v1": "0c5b4d4cfbbe8615ed863e00bfdd3a3325e028f9fba78ee1bf2e5bb819346ce1",
        "command-publication-revalidate-v1": "f0c37828d3057d77243506c2eee21e9c12f8e67a8489c42071580ba378f71993",
        "p1d4-command-publication-v1": "ec80534b837877db2dc992ffda1803f47ad8569c966d399edc3265fbc21b51c6",
        "p1d4-command-publication-revalidate-v1": "f15f0f0ef1ad1397cceaf1158e421f49bbdd8d0ad956a97ae3fe46c947108358",
        "atomic-stale-consumer-delete-v1": "cfabc24e0563c490e9950e250fdb146f992403394d48953a1b5eb2a2ffd09b6b",
    }
    require(redis["scripts"] == expected_scripts, "Lua identity drift")
    require(redis["production_endpoints"] == ["redis://127.0.0.1:6379/15", "redis://[::1]:6379/15"], "endpoint drift")
    require(redis["raw_connection_exposed"] is False, "raw connection opened")
    publication = value["publication"]
    require(publication["effect_order"] == [
        "persist_prepared", "publish_exact_retained_bytes", "exact_id_reread",
        "validate_receipt", "persist_published",
    ], "publication order drift")
    require(publication["fresh_candidate_rebuild_on_restart"] is False, "restart rebuild opened")
    resources = value["resources"]
    require(resources == {
        "db15_evidence_budget_bytes": 536870912,
        "minimum_root_free_bytes": 10737418240,
        "poll_interval_seconds": 5,
        "stop_action": "existing RedisLifecycleFailed terminal path; P1 only",
        "total_pel_fail_stop_threshold": 64,
    }, "resource contract drift")
    audit = value["audit"]
    require(audit["capacity"] == 4096 and audit["raw_command_material_retained"] is False and audit["secrets_retained"] is False, "audit boundary drift")
    require(all(flag is False for flag in value["closed_surfaces"].values()), "closed surface opened")
    require(value["next_after_acceptance"] == "P1F-Ie aggregate source closure; P1F-O0 remains closed", "next boundary drift")


def validate_matrix(root: Path) -> None:
    with (root / MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FID-{index:03}" for index in range(1, 31)], "matrix row drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row")


def validate_source(root: Path) -> None:
    redis = (root / REDIS).read_text()
    semantic = (root / SEMANTIC).read_text()
    producer = (root / PRODUCER).read_text()
    publisher = (root / PUBLISHER).read_text()
    runtime_lib = (root / RUNTIME_LIB).read_text()
    gateway_lib = (root / GATEWAY_LIB).read_text()
    for fragment in (
        "pub const STAGE8B_P1F_REDIS_ROLE_COUNT: usize = 8;",
        "pub const STAGE8B_P1F_REDIS_SOURCE_OPERATION_COUNT: usize = 10;",
        "pub const STAGE8B_P1F_REDIS_SCRIPT_COUNT: usize = 8;",
        "pub const STAGE8B_P1F_COMMAND_AUDIT_CAPACITY: usize = 4_096;",
        "pub const STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS: u64 = 5;",
        "pub const STAGE8B_P1F_TOTAL_PEL_FAIL_STOP_THRESHOLD: u64 = 64;",
        "pub const STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES: u64 = 536_870_912;",
        "pub const STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES: u64 = 10_737_418_240;",
        "Stage8bP1fRedisRoleV1::Provisioner",
        "Stage8bP1fRedisRoleV1::PhaseGuardian",
        "Stage8bP1fRedisRoleV1::BrokerTruthObserver",
        "Stage8bP1fRedisDatabaseScopeV1::None",
        "validate_production_db15_endpoint(redis_url)?;",
        "self.records.pop_front();",
        "command_fingerprint_sha256: sha256_hex(exact_command_material)",
        'redis::cmd("INFO")',
        "supervisor_control.pel_count().await?",
        "Stage8bP1eSupervisorEventV1::RedisLifecycleFailed",
        "real_redis_feeder_replays_exact_id_and_rereads_exact_bytes",
        "resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit",
    ):
        require(fragment in redis, f"Redis composition missing: {fragment}")
    for digest in read_json(root)["redis"]["scripts"].values():
        require(digest in redis, f"source Lua digest missing: {digest}")
    require(
        redis.count("validate_production_db15_endpoint(redis_url)?;") == 4,
        "endpoint guard coverage drift",
    )
    require(
        redis.count("command_fingerprint_sha256: sha256_hex(exact_command_material)") == 2,
        "hash-only audit coverage drift",
    )
    for forbidden in ('redis::cmd("DEL")', 'redis::cmd("XTRIM")', 'redis::cmd("CONFIG")', 'redis::cmd("FLUSHDB")'):
        require(forbidden not in redis, f"forbidden Redis effect: {forbidden}")
    require("pub async fn verify_exact_canonical_m10(" in semantic, "exact reread API missing")
    require("self.backend.exact_stream_entry(redis_id).await?.as_deref() != Some(payload)" in semantic, "exact bytes comparison missing")
    for fragment in (
        "pub trait Stage8bP1fM10PublicationPortV1",
        "persist_stage8b_p1f_m10_producer_state(path, &prepared)?;",
        ".publish_and_reread_exact_m10(",
        "|| !receipt.exact_reread",
        "mark_stage8b_p1f_m10_published(prepared, &redis_id, &exact_bytes)?",
        "id_publication_replays_retained_prepared_bytes_after_response_loss",
        "id_publication_refuses_unproven_exact_reread",
    ):
        require(fragment in producer, f"publication composition missing: {fragment}")
    require("pub struct Stage8bP1fSchedulePublisherRedisV1" in publisher, "schedule role adapter missing")
    require("stream != STAGE8B_P1E_SCHEDULE_STREAM || maxlen != MAXLEN" in publisher, "schedule fixed command guard missing")
    require("mod stage8b_p1f_fixed_redis;" in runtime_lib, "runtime module not private")
    require("mod stage8b_p1f_fixed_producers;" in gateway_lib, "producer module not private")
    require((root / HANDOFF).exists(), "Id handoff builder missing")
    require((root / HANDOFF_SAFETY).exists(), "Id handoff safety checker missing")


def validate_docs(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for text, name in ((document, "document"), (status, "status"), (roadmap, "roadmap")):
        require(BASE in text, f"{name}: accepted Ic ref missing")
        require("P1F-Id" in text and "P1F-Ie" in text and "P1F-O0" in text, f"{name}: boundary sequence missing")
        require("closed" in text.lower(), f"{name}: closed boundary missing")
    require("REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION" in document, "candidate status missing")
    require("The active source candidate is P1F-Id fixed Redis composition" in status, "status active Id boundary missing")
    require("P1F-Id is the active source\ncandidate" in roadmap, "roadmap active Id boundary missing")


def validate(root: Path, *, check_lineage: bool = True) -> None:
    if check_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_source(root)
    validate_docs(root)


def main() -> None:
    try:
        validate(ROOT)
    except (CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError) as error:
        print(f"stage8b-p1f-id-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-id-check roles=8 operations=10 scripts=8 operational=false")


if __name__ == "__main__":
    main()
