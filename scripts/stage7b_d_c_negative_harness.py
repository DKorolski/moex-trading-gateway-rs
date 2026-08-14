#!/usr/bin/env python3
"""Mutation harness for the Stage 7B-d-c service boundary."""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = Path("scripts/stage7b_d_c_check.py")
DESCRIPTOR = Path("docs/stage-7/stage7b-d-entry-descriptor.json")
AGGREGATE = Path("docs/stage-7/stage7b-entry-descriptor.json")
OWNERSHIP = Path("docs/stage-7/stage7b-d-row-ownership.json")
SERVICE = Path("crates/runtime-durable-service/src/recovery/redis_service.rs")
SUBPROCESS_TEST = Path("crates/runtime-durable-service/tests/stage7b_redis_service_subprocess.rs")
RECOVERY = Path("crates/runtime-durable-service/src/recovery.rs")
SETTLEMENT = Path("crates/runtime-durable-service/src/recovery/redis_settlement.rs")
BRIDGE = Path("crates/runtime-command-bridge/src/lib.rs")
MANIFEST = Path("crates/runtime-durable-service/Cargo.toml")
PROOF = Path("scripts/stage7b_proof_map.py")

COPY_PATHS = (
    Path("Cargo.lock"), MANIFEST, SERVICE, RECOVERY, SETTLEMENT, BRIDGE,
    Path("crates/runtime-durable-service/src/lib.rs"),
    SUBPROCESS_TEST,
    Path("docs/current-status.md"), Path("docs/roadmap.md"),
    Path("docs/stage-7/stage7b-d-c-implementation.md"),
    Path("docs/stage-7/stage7b-d-c-r2-review-closure.md"),
    DESCRIPTOR, AGGREGATE, OWNERSHIP,
    Path("docs/stage-7/stage7b-acceptance-proof-map.json"), CHECK, PROOF,
)


def mutate_json(path: Path, key: str, value: object) -> None:
    document = json.loads(path.read_text())
    document[key] = value
    path.write_text(json.dumps(document, indent=2) + "\n")


def replace(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    if old not in source:
        raise SystemExit(f"stage7b-d-c-negative: fixture token absent: {path}: {old}")
    path.write_text(source.replace(old, new, 1))


CASES = (
    ("accept-d-c-without-review", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_c_acceptance_pending", False)),
    ("close-d-c", lambda root: mutate_json(root / DESCRIPTOR, "stage7b_d_c_open", False)),
    ("implemented-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "implemented_count", 71)),
    ("pending-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "pending_count", 9)),
    ("reopen-b052", lambda root: mutate_json(root / DESCRIPTOR, "b052_b053_implemented", False)),
    ("detach-consumer", lambda root: mutate_json(root / DESCRIPTOR, "redis_consumer_attached", False)),
    ("open-runtime-live", lambda root: mutate_json(root / DESCRIPTOR, "runtime_live", True)),
    ("open-finam", lambda root: mutate_json(root / DESCRIPTOR, "finam_post_delete", True)),
    ("open-broker-dispatch", lambda root: mutate_json(root / DESCRIPTOR, "broker_network_dispatch", True)),
    ("open-real-orders", lambda root: mutate_json(root / DESCRIPTOR, "real_orders", True)),
    ("overclaim-exactly-once", lambda root: mutate_json(root / DESCRIPTOR, "cross_process_exactly_once_claimed", True)),
    ("negative-count-drift", lambda root: mutate_json(root / DESCRIPTOR, "d_c_negative_case_count", 23)),
    ("drop-storage-readiness", lambda root: replace(root / SERVICE, "if !state.durable_storage_ready {", "if false {")),
    ("drop-source-freshness", lambda root: replace(root / SERVICE, "if !source_poll_fresh {", "if false {")),
    ("drop-claim-freshness", lambda root: replace(root / SERVICE, "if !claim_scan_fresh {", "if false {")),
    ("drop-settlement-health", lambda root: replace(root / SERVICE, "if !state.settlement_healthy {", "if false {")),
    ("drop-durable-pending", lambda root: replace(root / SERVICE, "if state.durable_pending_count != 0 {", "if false {")),
    ("drop-blocked-lifecycle", lambda root: replace(root / SERVICE, "if !state.blocked_entries.is_empty() {", "if false {")),
    ("guard-created-inside-task", lambda root: replace(root / SERVICE, "let stop_guard = Stage7bTaskStopGuard(readiness);\n    tokio::spawn(async move {", "tokio::spawn(async move {\n        let stop_guard = Stage7bTaskStopGuard(readiness);")),
    ("reuse-consumer-name", lambda root: replace(root / SERVICE, "Uuid::new_v4()", "Uuid::nil()")),
    ("remove-subprocess-boot-proof", lambda root: replace(root / SUBPROCESS_TEST, "stage7b_d_c_b068_new_process_boot_uuid_is_unique", "stage7b_d_c_b068_same_process_only")),
    ("persist-claim-cursor", lambda root: replace(root / SERVICE, 'claim_cursor: "0-0".to_string()', 'claim_cursor: "persisted".to_string()')),
    ("remove-xautoclaim", lambda root: replace(root / SERVICE, 'redis::cmd("XAUTOCLAIM")', 'redis::cmd("XREAD")')),
    ("remove-pel-reconstruction", lambda root: replace(root / SERVICE, 'redis::cmd("XPENDING")', 'redis::cmd("PING")')),
    ("add-legacy-authority", lambda root: replace(root / MANIFEST, "redis.workspace = true", 'redis.workspace = true\nrusqlite = "0.32"')),
    ("block-deterministic-profile-rejection", lambda root: replace(root / SERVICE, "Stage7aRecoveredProfileClassification::DeterministicRejection(evidence)", "Stage7aRecoveredProfileClassification::Matched(evidence)")),
    ("drop-policy-rejection-classifier", lambda root: replace(root / SERVICE, "classify_stage7a_deterministic_policy_rejection(&envelope.payload, rejected)", "classify_stage7a_permanent_pre_admission_poison(&envelope.payload, rejected)")),
    ("bypass-owner-rejection-settlement", lambda root: replace(root / SERVICE, "settle_pre_stage6_rejection", "settle_finalized_ack")),
    ("forge-stage6-mutation-claim", lambda root: replace(root / SETTLEMENT, "stage6_mutation: false", "stage6_mutation: true")),
    ("settle-established-profile-conflict", lambda root: replace(root / SERVICE, "Stage7aRecoveredProfileClassification::IdentityConflict", "Stage7aRecoveredProfileClassification::DeterministicConflict")),
    ("remove-real-paper-ready-proof", lambda root: replace(root / RECOVERY, "stage7b_d_c_r1_b066_real_service_reports_ready_only_while_supervised_task_lives", "stage7b_d_c_r1_b066_manual_state_only")),
    ("remove-subprocess-redis-parent-proof", lambda root: replace(root / SUBPROCESS_TEST, "stage7b_d_c_r1_b068_fresh_process_reclaims_old_pel_with_real_redis", "stage7b_d_c_r1_b068_same_process_reclaim_only")),
    ("remove-subprocess-redis-child-proof", lambda root: replace(root / SUBPROCESS_TEST, "async fn stage7b_d_c_r1_b068_subprocess_redis_reclaim_child", "async fn stage7b_d_c_r1_b068_no_redis_child")),
    ("remove-pre-admission-request-marker-lookup", lambda root: replace(root / SERVICE, "lookup_canonical_request_publication", "skip_canonical_request_publication")),
    ("ignore-marker-when-stage6-request-absent", lambda root: replace(root / SERVICE, "if !observation.request_identity_was_established() {", "if observation.request_identity_was_established() {")),
    ("allow-changed-command-after-marker-hit", lambda root: replace(root / SERVICE, "Stage7bCanonicalRequestPublicationLookup::Present(_) => {", "Stage7bCanonicalRequestPublicationLookup::ChangedIdentityAllowed => {")),
    ("compare-marker-using-dynamic-authority", lambda root: replace(root / SETTLEMENT, "self.canonical_command_sha256 == identity.canonical_command_sha256()", "self.terminal_request_ack_identity == identity.canonical_command_sha256()")),
    ("allow-exact-marker-duplicate-provider-call", lambda root: replace(root / SERVICE, "settle_marker_duplicate(", "provider_marker_duplicate(")),
    ("create-stage6-lifecycle-for-marker-duplicate", lambda root: replace(root / SERVICE, "settle_canonical_marker_duplicate(", "admit_paper_command(")),
    ("overwrite-canonical-request-marker", lambda root: replace(root / SETTLEMENT, "if kind == 'ack' and not existing_request then", "if kind == 'ack' then")),
)


def main() -> None:
    expected = json.loads((ROOT / DESCRIPTOR).read_text()).get("d_c_negative_case_count")
    if len(CASES) != expected:
        raise SystemExit(f"stage7b-d-c-negative: FAIL descriptor/case-count drift descriptor={expected} actual={len(CASES)}")
    for name, mutation in CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage7b-d-c-negative-{name}-") as tmp:
            clone = Path(tmp) / "repo"
            subprocess.run(["git", "clone", "--quiet", "--no-hardlinks", str(ROOT), str(clone)], check=True)
            for relative in COPY_PATHS:
                source = ROOT / relative
                target = clone / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
            mutation(clone)
            result = subprocess.run(
                ["python3", str(clone / CHECK)], cwd=clone,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage7b-d-c-negative: FAIL mutation survived: {name}")
            print(f"PASS {name}")
    print(f"stage7b-d-c-negative: PASS cases={len(CASES)}")


if __name__ == "__main__":
    main()
