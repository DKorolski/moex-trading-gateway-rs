#!/usr/bin/env python3
"""Fail-closed source/contract checker for Stage 5G-e-c R4."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

BASE = "2394dbcd15d953e1799e07f7c903fdb3b072fc3f"
FILES = {
    "workspace": Path("Cargo.toml"),
    "lock": Path("Cargo.lock"),
    "crate": Path("crates/strategy-runtime-core/Cargo.toml"),
    "restart": Path("crates/strategy-runtime-core/src/stage5g_clean_restart.rs"),
    "stage5d": Path("crates/strategy-runtime-core/src/stage5d_persistence.rs"),
    "order": Path("crates/strategy-runtime-core/src/stage5g_order_position.rs"),
    "timer": Path("crates/strategy-runtime-core/src/stage5g_timer.rs"),
    "paper": Path("crates/strategy-runtime-core/src/stage5c_paper_host.rs"),
    "lib": Path("crates/strategy-runtime-core/src/lib.rs"),
    "contract": Path("docs/stage-5/stage5g-e-c-clean-process-reconstruction.md"),
    "descriptor": Path("docs/stage-5/stage5g-e-c-clean-process-reconstruction.json"),
    "status": Path("docs/current-status.md"),
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


def require_all(source: str, tokens: tuple[str, ...], label: str) -> None:
    for token in tokens:
        require(token in source, f"{label} drift: {token}")


def validate(root: Path, *, check_git: bool = True) -> None:
    for path in FILES.values():
        require((root / path).is_file(), f"missing e-c file: {path}")

    workspace = (root / FILES["workspace"]).read_text()
    crate = (root / FILES["crate"]).read_text()
    restart = (root / FILES["restart"]).read_text()
    stage5d = (root / FILES["stage5d"]).read_text()
    order = (root / FILES["order"]).read_text()
    paper = (root / FILES["paper"]).read_text()
    lib = (root / FILES["lib"]).read_text()
    contract = (root / FILES["contract"]).read_text()
    status = (root / FILES["status"]).read_text()
    descriptor = json.loads((root / FILES["descriptor"]).read_text())

    require(descriptor["stage"] == "5G-e-c", "descriptor stage drift")
    require(descriptor["reviewed_predecessor"] == BASE, "predecessor drift")
    require(descriptor["threat_model"] == "authenticated_package_hmac_sha256",
            "authenticated-package threat model drift")
    require(descriptor["operator_key_bytes"] == 32, "operator key length drift")
    require(descriptor["operator_key_serialized_in_package"] is False,
            "operator key entered package")
    require(descriptor["in_package_anchor_is_trust_root"] is False,
            "in-package anchor promoted to trust root")
    for field in (
        "authenticated_commitment_verified_before_runtime_mutation",
        "stage5d_is_single_persistence_authority",
        "source_capability_consumed_before_return",
        "strict_byte_boundary",
        "fresh_runtime_required",
        "semantic_private_riskgate_state_applied",
        "source_owned_cross_binding",
        "projection_validated_before_runtime_mutation",
        "callback_authority_is_type_derived",
        "rehash_aware_semantic_negatives",
        "nested_lifecycle_reseal_before_semantic_validation",
        "timer_ready_settlement_authority_persisted",
        "timer_summary_is_single_authenticated_source_projection",
        "duplicate_timer_summary_removed",
        "duplicate_timer_checkpoint_removed",
        "versioned_recovery_projection_persisted",
        "recovery_identity_recomputed_on_restore",
        "checkpoint_bound_to_authenticated_commitment",
        "history_bound_to_authenticated_commitment",
        "package_instance_source_commit_bound",
        "next_reconciliation_uses_validated_authority",
        "stage5d_internal_source_authority_anchor",
        "fully_resealed_unkeyed_package_rejected",
        "timer_history_tail_bound_to_settled_batch",
        "operator_key_rotation_rejects_old_epoch_package",
        "same_epoch_storage_rollback_prevention_external",
    ):
        require(descriptor[field] is True, f"invariant lost: {field}")
    require(descriptor["focused_test_count"] == 46, "focused test count drift")
    require(descriptor["negative_matrix_count"] == 40, "negative matrix count drift")
    require(descriptor["compile_fail_witness_count"] == 5,
            "compile-fail witness count drift")
    require(descriptor["public_clean_process_roundtrips"] == 4,
            "public roundtrip count drift")
    require(all(value is False for value in descriptor["closed_surfaces"].values()),
            "closed surface opened")
    require(f"Reviewed predecessor: `{BASE}`" in contract, "contract base drift")
    require("Stage 5G-e-c R4 is the only current implementation review candidate" in status,
            "status drift")
    require("one key epoch" in contract.lower()
            and "operator/storage responsibility" in contract,
            "rollback limitation must remain explicit")

    require('hmac = "0.12"' in workspace and "hmac.workspace = true" in crate,
            "HMAC dependency drift")
    require_all(restart, (
        "use hmac::{Hmac, Mac};",
        "pub struct Stage5gLifecycleCommitmentKey([u8; 32]);",
        "pub fn from_secret_bytes(secret: &[u8])",
        "pub fn export_stage5g_clean_restart(",
        "commitment_key: &Stage5gLifecycleCommitmentKey",
        "pub fn restore_stage5g_clean_restart(",
        "fn lifecycle_commitment_hmac_sha256(",
        "fn verify_lifecycle_commitment_hmac(",
        "Hmac::<Sha256>::new_from_slice",
        "mac.verify_slice(&tag).is_ok()",
        "AuthenticatedLifecycleCommitmentMismatch",
        ".stage5g_source_authority_hmac_sha256",
        "stage5d_source_anchor != independent_source_authority_sha256(projection)?",
        "if !verify_lifecycle_commitment_hmac(",
        "validate_projection(&projection)?;",
        "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;",
        "drop(source);",
        "pub(crate) struct Stage5gTimerReadyRestartProjectionV1",
        "pub(crate) enum Stage5gValidatedReconciliationAuthority",
        "stage5c_recovery_receipt_projection_sha256(",
        "settlement.settled_batch_history.last() != Some(&settlement.settled_batch)",
        "validated_reconciliation_authority",
        "let summary = self.reconciliation_authority.summary();",
    ), "authenticated lifecycle boundary")
    require("source_summary" not in restart, "duplicate TimerReady summary returned")
    require("source_checkpoint" not in restart, "duplicate TimerReady checkpoint returned")
    require(restart.count("from_secret_bytes(&[") == 1
            and "stage5ge_c_r4_debug_release_commitment_vector_is_deterministic" in restart,
            "embedded/default operator key entered production source")
    key_decl = restart.split("pub struct Stage5gLifecycleCommitmentKey", 1)[0][-200:]
    require("derive" not in key_decl.split("\n\n")[-1], "operator key gained derives")
    export_signature = restart.split("pub fn export_stage5g_clean_restart(", 1)[1].split(
        ") -> Result", 1)[0]
    restore_signature = restart.split("pub fn restore_stage5g_clean_restart(", 1)[1].split(
        ") -> Result", 1)[0]
    require("commitment_key: &Stage5gLifecycleCommitmentKey" in export_signature,
            "export lost operator key")
    require("commitment_key: &Stage5gLifecycleCommitmentKey" in restore_signature,
            "restore lost operator key")
    for forbidden in ("reqwest", "redis::", ".post(", ".delete(", "tokio::spawn"):
        require(forbidden not in restart, f"forbidden e-c surface: {forbidden}")
    independent = restart.split("fn independent_source_authority_sha256(", 1)[1].split(
        "#[cfg(test)]", 1)[0]
    require_all(independent, (
        "summary: &projection.summary",
        "checkpoint: &projection.checkpoint",
        "order_position_state: &projection.order_position_state",
        "timer_ready_source: &projection.timer_ready_source",
        "strategy_state_fingerprint_sha256: &projection.strategy_state_fingerprint_sha256",
    ), "authenticated source projection coverage")

    projection_at = restart.index("let projection: Stage5gCleanRestartProjectionV1")
    semantic_at = restart.index("validate_projection(&projection)?;", projection_at)
    binding_at = restart.index("validate_projection_binding(", semantic_at)
    mutation_at = restart.index(
        "stage5d_reconstruct_runtime_from_clean_restart(decoded, fresh_runtime)?;", binding_at)
    require(projection_at < semantic_at < binding_at < mutation_at,
            "semantic/HMAC validation must precede runtime mutation")
    binding_body = restart.split("fn validate_projection_binding(", 1)[1].split(
        "fn validated_reconciliation_authority(", 1)[0]
    require("if !verify_lifecycle_commitment_hmac(" in binding_body,
            "binding validation lost authenticated commitment")

    require_all(stage5d, (
        "pub stage5g_source_authority_anchor_sha256: Option<String>",
        "pub stage5g_source_authority_hmac_sha256: Option<String>",
        "stage5g_source_authority_hmac_sha256: Option<&str>",
        "stage5d_bind_stage5g_source_authority_anchor(&mut envelope, anchor, hmac)?;",
        "envelope.stage5g_source_authority_hmac_sha256 = Some(hmac_sha256.to_string());",
        "stage5g_test_reseal_lifecycle_authority(&mut extension)",
        "stage5g_test_source_authority_anchor_sha256(&extension)",
        "stage5c_recovery_receipt_projection_sha256(",
    ), "Stage 5D authenticated cross-binding")
    require("stage5g_source_authority_hmac_sha256 = Some(" not in stage5d.split(
        "pub(crate) fn stage5g_test_rehash_clean_restart_package", 1)[1],
        "test rehasher must not forge keyed commitment")

    require_all(paper, (
        "pub(crate) struct Stage5cRecoveryReceiptProjectionV1",
        "pub(crate) recovery_receipt: Stage5cRecoveryReceiptProjectionV1",
        "pub(crate) fn stage5c_recovery_receipt_projection_sha256(",
        "stage5c_recovery_receipt_projection(receipt)",
    ), "versioned recovery projection")

    required_tests = (
        "stage5ge_c_r1_public_timer_ready_clean_process_roundtrip",
        "stage5ge_c_r1_public_awaiting_clean_process_roundtrip",
        "stage5ge_c_r1_rehashed_stage5d_account_cross_binding_fails_closed",
        "stage5ge_c_r1_rehashed_stage5d_instrument_cross_binding_fails_closed",
        "stage5ge_c_r2_fully_resealed_recovery_receipt_graft_fails",
        "stage5ge_c_r2_fully_resealed_valid_checkpoint_graft_with_watermarks_fails",
        "stage5ge_c_r3_fully_resealed_timer_history_state_fingerprint_fails_anchor",
        "stage5ge_c_r4_missing_authenticated_commitment_fails_closed",
        "stage5ge_c_r4_authenticated_commitment_substitution_fails_closed",
        "stage5ge_c_r4_wrong_operator_commitment_key_fails_closed",
        "stage5ge_c_r4_fresh_runtime_config_mismatch_fails_closed",
        "stage5ge_c_r4_old_package_fails_after_operator_key_epoch_rotation",
        "stage5ge_c_r4_fully_coherent_unkeyed_reseal_cannot_forge_commitment",
        "stage5ge_c_r2_fully_resealed_complete_extension_graft_fails_package_binding",
    )
    require_all(order, required_tests, "acceptance witness")
    require("stage5ge_c_r4_debug_release_commitment_vector_is_deterministic" in restart,
            "debug/release deterministic commitment vector missing")
    require("AuthenticatedLifecycleCommitmentMismatch" in order,
            "typed authenticated rejection missing")
    require_all(lib, (
        "moved_source_cannot_be_reused",
        "let _copy = restored.clone();",
        "let _copy = key.clone();",
        "serde_json::to_string(&key)",
        'println!("{key:?}");',
    ), "linear/non-exportable compile-fail witness")

    if check_git and (root / ".git").exists():
        require(git(root, "rev-parse", f"{BASE}^{{commit}}") == BASE, "base missing")
        head = git(root, "rev-parse", "HEAD")
        if head != BASE:
            require(git(root, "rev-parse", "HEAD^") == BASE,
                    "R4 must be exactly one successor")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root, check_git=not args.skip_git)
    except (CheckFailure, OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"stage5g-ec-check: FAIL: {error}")
        return 1
    print("stage5g-ec-check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
