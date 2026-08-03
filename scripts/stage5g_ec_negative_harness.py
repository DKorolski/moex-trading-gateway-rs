#!/usr/bin/env python3
"""Forty fail-closed source mutations for Stage 5G-e-c R4."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage5g_ec_check as checker

ROOT = Path(__file__).resolve().parents[1]
PATHS = tuple(str(path) for path in checker.FILES.values())


def mutate(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"missing mutation anchor: {old}")
    path.write_text(source.replace(old, new, 1))


def mutate_all(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    source = path.read_text()
    if old not in source:
        raise RuntimeError(f"missing mutation anchor: {old}")
    path.write_text(source.replace(old, new))


def must_fail(label: str, mutation) -> None:
    with tempfile.TemporaryDirectory(prefix="stage5g-ec-r4-negative-") as raw:
        root = Path(raw)
        for relative in PATHS:
            destination = root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)
        mutation(root)
        try:
            checker.validate(root, check_git=False)
        except (checker.CheckFailure, ValueError, KeyError, json.JSONDecodeError):
            print(f"PASS {label}")
            return
        raise SystemExit(f"FAIL mutation escaped e-c R4 checker: {label}")


def main() -> int:
    workspace = str(checker.FILES["workspace"])
    crate = str(checker.FILES["crate"])
    restart = str(checker.FILES["restart"])
    stage5d = str(checker.FILES["stage5d"])
    order = str(checker.FILES["order"])
    paper = str(checker.FILES["paper"])
    lib = str(checker.FILES["lib"])
    contract = str(checker.FILES["contract"])
    descriptor = str(checker.FILES["descriptor"])
    status = str(checker.FILES["status"])
    cases = (
        ("threat-model-downgrade", lambda r: mutate(r, descriptor, '"authenticated_package_hmac_sha256"', '"rehashable_anchor"')),
        ("operator-key-serialized", lambda r: mutate(r, descriptor, '"operator_key_serialized_in_package": false', '"operator_key_serialized_in_package": true')),
        ("in-package-anchor-promoted", lambda r: mutate(r, descriptor, '"in_package_anchor_is_trust_root": false', '"in_package_anchor_is_trust_root": true')),
        ("hmac-workspace-dependency-removed", lambda r: mutate(r, workspace, 'hmac = "0.12"', 'removed_crypto = "0.12"')),
        ("hmac-crate-dependency-removed", lambda r: mutate(r, crate, "hmac.workspace = true", "removed_crypto.workspace = true")),
        ("operator-key-type-removed", lambda r: mutate(r, restart, "pub struct Stage5gLifecycleCommitmentKey([u8; 32]);", "pub struct RemovedLifecycleCommitmentKey([u8; 32]);")),
        ("operator-key-length-weakened", lambda r: mutate(r, restart, "pub struct Stage5gLifecycleCommitmentKey([u8; 32]);", "pub struct Stage5gLifecycleCommitmentKey(Vec<u8>);")),
        ("export-key-boundary-removed", lambda r: mutate(r, restart, "commitment_key: &Stage5gLifecycleCommitmentKey,", "_commitment_key_removed: (),")),
        ("hmac-derivation-removed", lambda r: mutate(r, restart, "fn lifecycle_commitment_hmac_sha256(", "fn removed_lifecycle_commitment_hmac_sha256(")),
        ("constant-time-verification-removed", lambda r: mutate(r, restart, "mac.verify_slice(&tag).is_ok()", "true")),
        ("authenticated-error-removed", lambda r: mutate_all(r, restart, "AuthenticatedLifecycleCommitmentMismatch", "RemovedKeyedError")),
        ("restore-hmac-field-read-removed", lambda r: mutate(r, restart, ".stage5g_source_authority_hmac_sha256", ".stage5g_source_authority_anchor_sha256")),
        ("restore-hmac-comparison-removed", lambda r: mutate(r, restart, "if !verify_lifecycle_commitment_hmac(", "if false && !verify_lifecycle_commitment_hmac(")),
        ("embedded-default-key", lambda r: mutate(r, restart, "pub fn export_stage5g_clean_restart(", "const FORGED: () = { let _ = Stage5gLifecycleCommitmentKey::from_secret_bytes(&[0_u8; 32]); };\n\npub fn export_stage5g_clean_restart(")),
        ("stage5d-hmac-field-removed", lambda r: mutate(r, stage5d, "pub stage5g_source_authority_hmac_sha256: Option<String>,", "pub removed_stage5g_hmac: Option<String>,")),
        ("stage5d-hmac-binder-removed", lambda r: mutate(r, stage5d, "envelope.stage5g_source_authority_hmac_sha256 = Some(hmac_sha256.to_string());", "let _unbound_hmac = hmac_sha256;")),
        ("test-rehasher-forges-keyed-commitment", lambda r: mutate(r, stage5d, "if envelope.stage5g_source_authority_anchor_sha256.is_some() {", "envelope.stage5g_source_authority_hmac_sha256 = Some(\"0\".repeat(64));\n    if envelope.stage5g_source_authority_anchor_sha256.is_some() {")),
        ("duplicate-source-summary-reintroduced", lambda r: mutate(r, restart, "pub(crate) struct Stage5gTimerReadyRestartProjectionV1 {", "pub(crate) struct Stage5gTimerReadyRestartProjectionV1 {\n    source_summary: String,")),
        ("duplicate-source-checkpoint-reintroduced", lambda r: mutate(r, restart, "pub(crate) struct Stage5gTimerReadyRestartProjectionV1 {", "pub(crate) struct Stage5gTimerReadyRestartProjectionV1 {\n    source_checkpoint: String,")),
        ("summary-excluded-from-authenticated-projection", lambda r: mutate_all(r, restart, "summary: &projection.summary,", "summary: &projection.checkpoint,")),
        ("checkpoint-excluded-from-authenticated-projection", lambda r: mutate_all(r, restart, "checkpoint: &projection.checkpoint,", "checkpoint: &projection.summary,")),
        ("history-source-excluded-from-authenticated-projection", lambda r: mutate_all(r, restart, "timer_ready_source: &projection.timer_ready_source,", "timer_ready_source: &projection.order_position_state,")),
        ("recovery-projection-type-removed", lambda r: mutate(r, paper, "pub(crate) struct Stage5cRecoveryReceiptProjectionV1", "pub(crate) struct RemovedRecoveryReceiptProjectionV1")),
        ("recovery-projection-field-removed", lambda r: mutate(r, paper, "pub(crate) recovery_receipt: Stage5cRecoveryReceiptProjectionV1", "pub(crate) removed_recovery_receipt: Stage5cRecoveryReceiptProjectionV1")),
        ("recovery-identity-recompute-removed", lambda r: mutate(r, restart, "stage5c_recovery_receipt_projection_sha256(", "removed_recovery_receipt_projection_sha256(")),
        ("coherent-recovery-reseal-removed", lambda r: mutate(r, stage5d, "stage5c_recovery_receipt_projection_sha256(", "removed_recovery_receipt_projection_sha256(")),
        ("coherent-reseal-acceptance-test-removed", lambda r: mutate(r, order, "stage5ge_c_r4_fully_coherent_unkeyed_reseal_cannot_forge_commitment", "removed_fully_coherent_unkeyed_reseal_test")),
        ("missing-commitment-test-removed", lambda r: mutate(r, order, "stage5ge_c_r4_missing_authenticated_commitment_fails_closed", "removed_missing_commitment_test")),
        ("wrong-key-test-removed", lambda r: mutate(r, order, "stage5ge_c_r4_wrong_operator_commitment_key_fails_closed", "removed_wrong_key_test")),
        ("old-epoch-test-removed", lambda r: mutate(r, order, "stage5ge_c_r4_old_package_fails_after_operator_key_epoch_rotation", "removed_old_epoch_test")),
        ("history-mutation-test-removed", lambda r: mutate(r, order, "stage5ge_c_r3_fully_resealed_timer_history_state_fingerprint_fails_anchor", "removed_history_mutation_test")),
        ("checkpoint-graft-test-removed", lambda r: mutate(r, order, "stage5ge_c_r2_fully_resealed_valid_checkpoint_graft_with_watermarks_fails", "removed_checkpoint_graft_test")),
        ("complete-extension-graft-test-removed", lambda r: mutate(r, order, "stage5ge_c_r2_fully_resealed_complete_extension_graft_fails_package_binding", "removed_complete_extension_graft_test")),
        ("source-linearity-proof-removed", lambda r: mutate(r, lib, "moved_source_cannot_be_reused", "removed_source_linearity_proof")),
        ("key-nonclone-proof-removed", lambda r: mutate(r, lib, "let _copy = key.clone();", "let _copy = &key;")),
        ("key-nonserialize-proof-removed", lambda r: mutate(r, lib, "serde_json::to_string(&key)", "serde_json::to_string(&())")),
        ("key-nondebug-proof-removed", lambda r: mutate(r, lib, 'println!("{key:?}");', 'println!("hidden");')),
        ("stage5g-f-opened", lambda r: mutate(r, descriptor, '"stage5g_f": false', '"stage5g_f": true')),
        ("runtime-live-opened", lambda r: mutate(r, descriptor, '"runtime_live": false', '"runtime_live": true')),
        ("r4-status-removed", lambda r: mutate(r, status, "Stage 5G-e-c R4 is the only current implementation review candidate", "Stage 5G-e-c is unspecified")),
    )
    for label, mutation in cases:
        must_fail(label, mutation)
    print(f"stage5g-ec-negative-harness: PASS {len(cases)}/{len(cases)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
