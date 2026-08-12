#!/usr/bin/env python3
"""Descriptor-pinned Stage 7B-c recovery-boundary mutation inventory."""
from __future__ import annotations

import json
from pathlib import Path

import stage7b_c_check as checker

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = (ROOT / "Cargo.toml").read_text()
MANIFEST = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text()
RECOVERY = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
CLEAN = (ROOT / "crates/strategy-runtime-core/src/stage5g_clean_restart.rs").read_text()
LIVE = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
LIB = (ROOT / "crates/runtime-durable-service/src/lib.rs").read_text()
DESCRIPTOR = json.loads((ROOT / "docs/stage-7/stage7b-c-entry-descriptor.json").read_text())


def changed_descriptor(key: str, value: object) -> dict:
    changed = dict(DESCRIPTOR)
    changed[key] = value
    return changed


CASES = [
    ("redis-dependency", {"manifest": MANIFEST + "\nredis.workspace = true\n"}),
    ("serde-removed", {"manifest": MANIFEST.replace("serde.workspace = true", "serde_removed = true", 1)}),
    ("empty-seed-accepted", {"recovery": RECOVERY.replace("if stage5g_seed.is_empty()", "if false", 1)}),
    ("seed-validation-removed", {"recovery": RECOVERY.replace("restore_stage5g_clean_restart(stage5g_seed, commitment_key, fresh_runtime)", "removed_seed_validation(stage5g_seed)", 1)}),
    ("seal-construction-removed", {"recovery": RECOVERY.replace("Stage7bRecoverySealV1::new(", "removed_recovery_seal_new(", 1)}),
    ("seal-hmac-field-removed", {"recovery": RECOVERY.replace("    seal_commitment_hmac_sha256: String,\n", "", 1)}),
    ("seal-hmac-verification-removed", {"recovery": RECOVERY.replace("|| !commitment_key.stage7b_verify_recovery_seal_hmac_sha256(", "|| false && !commitment_key.stage7b_verify_recovery_seal_hmac_sha256(", 1)}),
    ("package-digest-removed", {"recovery": RECOVERY.replace("|| sha256_hex(&self.stage6d_authenticated_restart_package)\n                != self.stage6d_restart_package_sha256", "|| false", 1)}),
    ("checkpoint-digest-removed", {"recovery": RECOVERY.replace("|| sha256_hex(&self.stage6_checkpoint.encode_canonical())\n                != self.stage6_checkpoint_bytes_sha256", "|| false", 1)}),
    ("identity-binding-removed", {"recovery": RECOVERY.replace("|| self.operational_identity_sha256 != expected_operational_identity_sha256", "|| false", 1)}),
    ("zero-generation-accepted", {"recovery": RECOVERY.replace("|| self.seal_generation == 0", "|| false", 1)}),
    ("canonical-reread-removed", {"recovery": RECOVERY.replace("if seal.encode_canonical()? != bytes", "if false", 1)}),
    ("temp-exclusive-removed", {"recovery": RECOVERY.replace("libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW", "libc::O_WRONLY | libc::O_CREAT", 1)}),
    ("temp-sync-removed", {"recovery": RECOVERY.replace("            temp.sync_all()\n                .map_err(|error| Stage7bRecoveryError::SealWriteFailed(error.kind()))?;\n", "", 1)}),
    ("rename-removed", {"recovery": RECOVERY.replace("            rename_child_at(\n", "            removed_rename(\n", 1)}),
    ("parent-sync-removed", {"recovery": RECOVERY.replace("            self.root\n                .root_directory\n                .sync_all()", "            removed_parent_sync()", 1)}),
    ("committed-reread-removed", {"recovery": RECOVERY.replace("                .read_committed_recovery_seal()?", "                .removed_committed_reread()?", 1)}),
    ("seal-without-journal-accepted", {"recovery": RECOVERY.replace("if seal_exists && !journal_exists", "if false", 1)}),
    ("checkpoint-validation-removed", {"recovery": RECOVERY.replace("            .validate_checkpoint(committed_seal.stage6_checkpoint())", "            .removed_checkpoint_validation(committed_seal.stage6_checkpoint())", 1)}),
    ("restart-authentication-removed", {"recovery": RECOVERY.replace("let recovered = match restart_stage6d_paper_with_owned_journal", "let recovered = match removed_authenticated_restart", 1)}),
    ("owner-field-order-reversed", {"recovery": RECOVERY.replace("    recovered: Stage6dDurableRuntimeRecovered,\n    writer_lease: Stage7bKernelWriterLease,", "    writer_lease: Stage7bKernelWriterLease,\n    recovered: Stage6dDurableRuntimeRecovered,", 1)}),
    ("mutable-runtime-escape", {"recovery": RECOVERY + "\nimpl Stage7bRecoveryReadyOwner { pub fn recovered_mut(&mut self) {} }\n"}),
    ("cached-readiness", {"recovery": RECOVERY.replace("self.writer_lease.validate_namespace().is_ok()", "true", 1)}),
    ("blocked-provider-enabled", {"recovery": RECOVERY.replace("pub fn paper_provider_invocation_allowed(&self) -> bool {\n        false", "pub fn paper_provider_invocation_allowed(&self) -> bool {\n        true", 1)}),
    ("descriptor-overclaim-redis", {"descriptor": changed_descriptor("redis_consumer_attached", True)}),
    ("negative-count-drift", {"descriptor": changed_descriptor("negative_case_count", 25)}),
]


def must_fail(name: str, mutation: dict) -> None:
    try:
        if "descriptor" in mutation:
            checker.validate_descriptor(mutation["descriptor"])
        elif "manifest" in mutation:
            checker.check_dependencies(WORKSPACE, mutation["manifest"])
        else:
            checker.validate_source(
                mutation.get("recovery", RECOVERY),
                mutation.get("clean", CLEAN),
                mutation.get("live", LIVE),
                mutation.get("lib", LIB),
            )
    except (checker.CheckFailure, ValueError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"stage7b-c-negative: mutation survived: {name}")


if len(CASES) != DESCRIPTOR["negative_case_count"]:
    raise SystemExit("stage7b-c-negative: descriptor/case count drift")
for case_name, case_mutation in CASES:
    must_fail(case_name, case_mutation)
print(f"stage7b-c-negative: PASS cases={len(CASES)}")
