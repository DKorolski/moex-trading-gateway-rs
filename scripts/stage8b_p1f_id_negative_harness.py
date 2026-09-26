#!/usr/bin/env python3
"""Mutation harness for the Stage 8B-P1-f Id checker."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1f_id_check as check


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"mutation anchor count for {relative}: {old!r}")
    path.write_text(text.replace(old, new, 1))


CASES = (
    ("self-accept", check.INVENTORY, "REVIEW_CANDIDATE_SOURCE_ONLY_NO_ACTIVATION", "ACCEPTED"),
    ("lineage", check.INVENTORY, check.BASE, "0" * 40),
    ("surface-open", check.INVENTORY, '"operational_db0_or_db15": false', '"operational_db0_or_db15": true'),
    ("role-count", check.REDIS, "STAGE8B_P1F_REDIS_ROLE_COUNT: usize = 8", "STAGE8B_P1F_REDIS_ROLE_COUNT: usize = 9"),
    ("operation-count", check.REDIS, "STAGE8B_P1F_REDIS_SOURCE_OPERATION_COUNT: usize = 10", "STAGE8B_P1F_REDIS_SOURCE_OPERATION_COUNT: usize = 11"),
    ("script-count", check.REDIS, "STAGE8B_P1F_REDIS_SCRIPT_COUNT: usize = 8", "STAGE8B_P1F_REDIS_SCRIPT_COUNT: usize = 9"),
    ("audit-capacity", check.REDIS, "STAGE8B_P1F_COMMAND_AUDIT_CAPACITY: usize = 4_096", "STAGE8B_P1F_COMMAND_AUDIT_CAPACITY: usize = usize::MAX"),
    ("poll-interval", check.REDIS, "STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS: u64 = 5", "STAGE8B_P1F_RESOURCE_POLL_INTERVAL_SECONDS: u64 = 50"),
    ("pel-limit", check.REDIS, "STAGE8B_P1F_TOTAL_PEL_FAIL_STOP_THRESHOLD: u64 = 64", "STAGE8B_P1F_TOTAL_PEL_FAIL_STOP_THRESHOLD: u64 = 640"),
    ("memory-limit", check.REDIS, "STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES: u64 = 536_870_912", "STAGE8B_P1F_DB15_EVIDENCE_BUDGET_BYTES: u64 = u64::MAX"),
    ("disk-limit", check.REDIS, "STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES: u64 = 10_737_418_240", "STAGE8B_P1F_MINIMUM_ROOT_FREE_BYTES: u64 = 0"),
    ("endpoint-guard", check.REDIS, "validate_production_db15_endpoint(redis_url)?;\n        Self::connect_at(redis_url, config, Stage8bP1fRedisRoleV1::SyntheticM10Feeder).await", "Self::connect_at(redis_url, config, Stage8bP1fRedisRoleV1::SyntheticM10Feeder).await"),
    ("audit-raw", check.REDIS, "operation,\n            database: STAGE8B_P1F_REDIS_DATABASE,\n            script_sha256: script.map(|value| value.sha256().to_string()),\n            command_fingerprint_sha256: sha256_hex(exact_command_material)", "operation,\n            database: STAGE8B_P1F_REDIS_DATABASE,\n            script_sha256: script.map(|value| value.sha256().to_string()),\n            command_fingerprint_sha256: String::from_utf8_lossy(exact_command_material).into_owned()"),
    ("resource-route", check.REDIS, "Stage8bP1eSupervisorEventV1::RedisLifecycleFailed", "Stage8bP1eSupervisorEventV1::ExternalSignal"),
    ("resource-production-caller", check.PROCESS, "let resources = run_stage8b_p1f_resource_monitor_v1(", "let resources = removed_resource_monitor("),
    ("resource-owner-select", check.PROCESS, "tokio::pin!(resources);", "// resource future not pinned"),
    ("command-pel-group", check.REDIS, ".arg(&namespace.canonical_command_stream)", ".arg(&namespace.canonical_m10_stream)"),
    ("verify-attach-audit", check.SUPERVISOR, "Stage8bP1fRedisSourceOperationV1::VerifyOnlyAttach", "Stage8bP1fRedisSourceOperationV1::RetentionAdmission"),
    ("schedule-read-audit", check.SCHEDULE_SOURCE, "Stage8bP1fRedisSourceOperationV1::ScheduleRead", "Stage8bP1fRedisSourceOperationV1::RetentionAdmission"),
    ("exact-reread", check.SEMANTIC, "pub async fn verify_exact_canonical_m10(", "async fn verify_exact_canonical_m10("),
    ("exact-compare", check.SEMANTIC, "self.backend.exact_stream_entry(redis_id).await?.as_deref() != Some(payload)", "false"),
    ("prepared-before-effect", check.PRODUCER, "persist_stage8b_p1f_m10_producer_state(path, &prepared)?;", "// persist removed"),
    ("receipt-reread", check.PRODUCER, "|| !receipt.exact_reread", "|| false"),
    ("mark-published", check.PRODUCER, "mark_stage8b_p1f_m10_published(prepared, &redis_id, &exact_bytes)?", "prepared"),
    ("linked-response-loss", check.PRODUCER, "id_linked_real_redis_response_loss_restarts_prepared_without_duplicate", "removed_linked_response_loss_control"),
    ("schedule-stream", check.PUBLISHER, "stream != STAGE8B_P1E_SCHEDULE_STREAM || maxlen != MAXLEN", "false"),
    ("matrix-row", check.MATRIX, "P1FID-030,boundary,Install VPS operational Redis provider FINAM writes runtime-live and real orders remain closed,REQUIRED\n", ""),
    ("status", check.STATUS, "The active source candidate is P1F-Id fixed Redis composition", "The active source candidate is P1F-O0"),
    ("roadmap", check.ROADMAP, "P1F-Id is the active source\ncandidate", "P1F-O0 is the active source\ncandidate"),
)


def copy_contract(root: Path) -> None:
    for relative in check.ALLOWED_CHANGES:
        source = check.ROOT / relative
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="stage8b-p1f-id-negative-") as raw:
        root = Path(raw)
        copy_contract(root)
        check.validate(root, check_lineage=False)
        print("PASS positive-control")
        document = root / check.DOCUMENT
        document.write_text(document.read_text() + "\n")
        check.validate(root, check_lineage=False)
        print("PASS nonsemantic-control")
    for name, relative, old, new in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1f-id-{name}-") as raw:
            root = Path(raw)
            copy_contract(root)
            replace(root, relative, old, new)
            try:
                check.validate(root, check_lineage=False)
            except (check.CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError):
                print(f"PASS {name}")
            else:
                raise SystemExit(f"FAIL mutation survived: {name}")
    print(f"PASS stage8b-p1f-id-negative-harness {len(CASES)}/{len(CASES)}")


if __name__ == "__main__":
    main()
