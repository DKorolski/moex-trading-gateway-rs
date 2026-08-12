#!/usr/bin/env python3
"""Stage 7B-c recovery-seal and linear restart-owner acceptance checker."""
from __future__ import annotations

import csv
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "ff3fa2e8908440863b40b838991d4716b33caad4"
STAGE7A_BASE = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
BRANCH = "stage7b-production-durability"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
MATRIX_SHA256 = "083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d"


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


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
                return source[start : index + 1]
    raise CheckFailure(f"unterminated source block: {needle}")


def check_lineage(root: Path) -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", BASE], cwd=root, text=True
    ).strip()
    require(merge_base == BASE, "candidate is not based on accepted Stage 7B-b-R2")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=root, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-c branch drift")


def check_governance(root: Path) -> None:
    status = " ".join((root / "docs/current-status.md").read_text().split())
    roadmap = " ".join((root / "docs/roadmap.md").read_text().split())
    onboarding = " ".join(
        (root / "docs/reviewer-onboarding-and-roadmap.md").read_text().split()
    )
    for text in (status, roadmap, onboarding):
        require(BASE in text, "accepted Stage 7B-b-R2 lineage absent")
        require("Stage 7B-c" in text, "active Stage 7B-c declaration absent")
    require("Stage 7B-b is CLOSED" in status, "Stage 7B-b closure absent")
    require("Redis-settlement work remains closed" in roadmap, "Stage 7B-d boundary absent")


def check_dependencies(workspace: str, manifest: str) -> None:
    require('"crates/runtime-durable-service"' in workspace, "service absent")
    for forbidden in ("redis", "broker-finam", "finam-gateway", "reqwest", "rusqlite"):
        require(forbidden not in manifest, f"forbidden service dependency: {forbidden}")
    for required in (
        'strategy-runtime-core = { path = "../strategy-runtime-core" }',
        "libc.workspace = true",
        "serde.workspace = true",
        "serde_json.workspace = true",
        "sha2.workspace = true",
    ):
        require(required in manifest, f"required dependency absent: {required}")


def validate_source(recovery: str, clean_restart: str, live_core: str, lib: str) -> None:
    required = (
        "pub struct Stage7bRecoverySealV1",
        "#[serde(deny_unknown_fields)]",
        "seal_generation: u64",
        "stage6d_authenticated_restart_package: Vec<u8>",
        "stage6d_restart_package_sha256: String",
        "stage6_checkpoint: Stage6JournalCheckpointV1",
        "stage6_checkpoint_bytes_sha256: String",
        "operational_identity_sha256: String",
        "seal_commitment_sha256: String",
        "seal_commitment_hmac_sha256: String",
        "moex.stage7b.recovery-seal.commitment.v1",
        "stage7b_recovery_seal_hmac_sha256",
        "stage7b_verify_recovery_seal_hmac_sha256",
        "|| !commitment_key.stage7b_verify_recovery_seal_hmac_sha256(",
        "|| sha256_hex(&self.stage6d_authenticated_restart_package)\n                != self.stage6d_restart_package_sha256",
        "|| sha256_hex(&self.stage6_checkpoint.encode_canonical())\n                != self.stage6_checkpoint_bytes_sha256",
        "|| self.operational_identity_sha256 != expected_operational_identity_sha256",
        "|| self.seal_generation == 0",
        "if seal.encode_canonical()? != bytes",
        "restore_stage5g_clean_restart(stage5g_seed, commitment_key, fresh_runtime)",
        "first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal",
        "restart_stage6d_paper_with_owned_journal",
        "Stage7bRecoveryBlockReason::MissingCommittedSeal",
        "Stage7bRecoveryBlockReason::CorruptCommittedSeal",
        "Stage7bRecoveryBlockReason::CheckpointMismatch",
        "Stage7bRecoveryBlockReason::AuthenticatedRestartRejected",
        "pub struct Stage7bRecoveryReadyOwner",
        "recovered: Stage6dDurableRuntimeRecovered",
        "writer_lease: Stage7bKernelWriterLease",
        "pub struct Stage7bRecoveryBlocked",
        "pub fn paper_provider_invocation_allowed(&self) -> bool",
        "pub fn redis_settlement_allowed(&self) -> bool",
        "pub fn xack_allowed(&self) -> bool",
        "first_boot_requires_stage5g_seed",
        "invalid_stage5g_seed_rejected_before_journal_creation",
        "initial_recovery_seal_before_ready_and_lease_lifetime",
        "recovery_seal_canonical_roundtrip_and_restart",
        "recovery_seal_atomic_replace_and_orphan_temp_is_not_authority",
        "corrupt_recovery_seal_rejected_and_blocked_has_zero_effect",
        "seal_without_journal_rejected_without_creating_journal",
        "journal_without_seal_is_explicit_recovery_blocked",
        "recovery_operational_identity_mismatch_is_blocked",
        "recovery_hmac_digest_mismatch_is_blocked",
    )
    for token in required:
        require(token in recovery, f"recovery invariant absent: {token}")

    first_boot = block(recovery, "    pub fn first_boot(")
    first_order = (
        "if stage5g_seed.is_empty()",
        "restore_stage5g_clean_restart(stage5g_seed, commitment_key, fresh_runtime)",
        "Stage7bRecoverySealV1::new(",
        "Stage7bWritableDurableAuthority::create_new",
        "first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal",
        "writer_lease.commit_recovery_seal(&committed_seal)?",
        "Ok(Self",
    )
    positions = [first_boot.index(token) for token in first_order]
    require(positions == sorted(positions), "seed/seal/ready first-boot ordering drift")

    restart = block(recovery, "    pub fn restart(")
    restart_order = (
        "if seal_exists && !journal_exists",
        "Stage7bWritableDurableAuthority::open_existing",
        "Stage7bRecoverySealV1::decode_canonical",
        ".validate_checkpoint(committed_seal.stage6_checkpoint())",
        "restart_stage6d_paper_with_owned_journal",
        "validate_recovered_binding",
        "writer_lease.validate_namespace()?",
        "Stage7bRestartOutcome::Ready",
    )
    positions = [restart.index(token) for token in restart_order]
    require(positions == sorted(positions), "restart validation/ready ordering drift")

    commit = block(recovery, "    fn commit_recovery_seal(")
    atomic_order = (
        "libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW",
        "temp.write_all(&bytes)",
        "temp.sync_all()",
        "rename_child_at(",
        ".root_directory\n                .sync_all()",
        "read_committed_recovery_seal()?",
        "if committed != bytes",
    )
    positions = [commit.index(token) for token in atomic_order]
    require(positions == sorted(positions), "atomic recovery-seal ordering drift")
    require("File::create" not in commit and "fs::write" not in commit, "in-place seal write admitted")

    owner = block(recovery, "pub struct Stage7bRecoveryReadyOwner")
    require(owner.index("recovered:") < owner.index("writer_lease:"), "runtime must drop before lease")
    require("#[derive" not in recovery[max(0, recovery.index("pub struct Stage7bRecoveryReadyOwner") - 80):recovery.index("pub struct Stage7bRecoveryReadyOwner")], "ready owner became derivable")
    for forbidden in ("recovered_mut", "into_recovered", "into_writer_lease", "Serialize for Stage7bRecoveryReadyOwner"):
        require(forbidden not in recovery, f"linear ready-owner escape: {forbidden}")
    blocked = block(recovery, "impl Stage7bRecoveryBlocked")
    require(blocked.count("false") >= 4, "RecoveryBlocked is not zero-effect")
    owner_impl = block(recovery, "impl Stage7bRecoveryReadyOwner")
    ready = block(owner_impl, "    pub fn recovery_ready(&self) -> bool")
    require("self.writer_lease.validate_namespace().is_ok()" in ready, "readiness trusts cached state")
    recovered = block(owner_impl, "    pub fn recovered(&self)")
    require("self.writer_lease.validate_namespace()?" in recovered, "authority-sensitive read lacks namespace validation")

    require("moex.stage7b.recovery-seal.v1\\0" in clean_restart, "HMAC domain absent")
    require("stage7b_verify_recovery_seal_hmac_sha256" in clean_restart, "HMAC verifier absent")
    require("first_boot_stage6d_paper_from_validated_stage5g_seed_with_owned_journal" in live_core, "validated Stage6 first-boot bridge absent")
    for test in (
        "stage6e_extra_finalized_stage6_history_does_not_need_current_stage5_slot",
        "stage6e_extra_unresolved_stage6_authority_is_rejected",
        "stage6e_matching_stage5_stage6_pair_is_cross_bound_before_capability",
    ):
        require(test in live_core, f"inherited Stage6 cross-binding witness absent: {test}")
    require("recovered_mut" in lib, "compile-fail no-mutable-owner witness absent")


def validate_descriptor(descriptor: dict) -> None:
    expected = {
        "stage": "7B",
        "slice": "7B-c",
        "accepted_stage7a_predecessor": STAGE7A_BASE,
        "accepted_slice_predecessor": BASE,
        "branch": BRANCH,
        "blocking_acceptance_rows": 80,
        "semantic_proof_map_count": 80,
        "implemented_count": 42,
        "pending_count": 38,
        "negative_case_count": 26,
        "source_stage5g_seed_required": True,
        "recovery_seal_implemented": True,
        "recovery_seal_hmac_authenticated": True,
        "atomic_recovery_seal_commit": True,
        "linear_recovered_runtime_and_writer_lease_owner": True,
        "recovery_blocked_zero_effect": True,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "normative_tz_sha256": TZ_SHA256,
        "normative_matrix_sha256": MATRIX_SHA256,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"descriptor drift: {key}")


def check(root: Path) -> None:
    check_lineage(root)
    check_governance(root)
    tz = root / "docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md"
    matrix = root / "docs/stage-7/STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv"
    require(sha256(tz) == TZ_SHA256, "normative TZ drift")
    require(sha256(matrix) == MATRIX_SHA256, "normative matrix drift")
    with matrix.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 80, "matrix row count drift")
    require([row["ID"] for row in rows] == [f"B-{i:03d}" for i in range(1, 81)], "matrix IDs drift")
    check_dependencies(
        (root / "Cargo.toml").read_text(),
        (root / "crates/runtime-durable-service/Cargo.toml").read_text(),
    )
    validate_source(
        (root / "crates/runtime-durable-service/src/recovery.rs").read_text(),
        (root / "crates/strategy-runtime-core/src/stage5g_clean_restart.rs").read_text(),
        (root / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text(),
        (root / "crates/runtime-durable-service/src/lib.rs").read_text(),
    )
    validate_descriptor(json.loads((root / "docs/stage-7/stage7b-c-entry-descriptor.json").read_text()))
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=root, check=True)
    print("stage7b-c-check: PASS rows=80 implemented=42 pending=38 accepted=false")


if __name__ == "__main__":
    check(ROOT)
