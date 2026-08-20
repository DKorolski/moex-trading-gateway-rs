#!/usr/bin/env python3
"""Run the 80 fail-closed Stage 8A-4 durable composition I3 R4 mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_i3_check as checker

ROOT = Path(__file__).resolve().parents[1]


def copy_required(destination: Path) -> None:
    for relative in checker.REQUIRED:
        source, target = ROOT / relative, destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def mutate_text(root: Path, relative: Path, old: str, new: str, all_matches: bool = False) -> None:
    path = root / relative
    document = path.read_text(encoding="utf-8")
    if old not in document:
        raise RuntimeError(f"missing mutation anchor: {relative}: {old}")
    path.write_text(document.replace(old, new, -1 if all_matches else 1), encoding="utf-8")


def mutate_authority(root: Path, key: str, value: object) -> None:
    path = root / checker.AUTHORITY
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    cases = [
        ("i2-predecessor-drift", lambda r: mutate_authority(r, "accepted_i2_r3_ref", "0" * 40)),
        ("i2-review-drift", lambda r: mutate_authority(r, "accepted_i2_r3_review_sha256", "0" * 64)),
        ("i3-r1-ref-drift", lambda r: mutate_authority(r, "rejected_i3_r1_ref", "1" * 40)),
        ("i3-r1-review-drift", lambda r: mutate_authority(r, "rejected_i3_r1_review_sha256", "1" * 64)),
        ("i3-r2-ref-drift", lambda r: mutate_authority(r, "rejected_i3_r2_ref", "2" * 40)),
        ("i3-r2-review-drift", lambda r: mutate_authority(r, "rejected_i3_r2_review_sha256", "2" * 64)),
        ("r4-spec-drift", lambda r: mutate_authority(r, "i3_r4_correction_spec_sha256", "3" * 64)),
        ("premature-acceptance", lambda r: mutate_authority(r, "status", "accepted")),
        ("branch-drift", lambda r: mutate_authority(r, "branch", "main")),
        ("raw-writer-authorized", lambda r: mutate_authority(r, "raw_batch_writer_publicly_callable", True)),
        ("raw-core-authorized", lambda r: mutate_authority(r, "raw_core_append_normal_public_api", True)),
        ("sealed-authority-disabled", lambda r: mutate_authority(r, "sealed_linear_writer_authority", False)),
        ("truth-binding-disabled", lambda r: mutate_authority(r, "exact_request_truth_binding", False)),
        ("freshness-disabled", lambda r: mutate_authority(r, "writer_entry_truth_freshness", False)),
        ("control-binding-disabled", lambda r: mutate_authority(r, "exact_control_operational_binding", False)),
        ("sticky-poison-disabled", lambda r: mutate_authority(r, "post_write_error_poison_sticky", False)),
        ("suffix-matrix-disabled", lambda r: mutate_authority(r, "suffix_fault_matrix_covered", False)),
        ("ack-opened", lambda r: mutate_authority(r, "ack_readiness_enabled", True)),
        ("redis-opened", lambda r: mutate_authority(r, "redis_live_enabled", True)),
        ("finam-opened", lambda r: mutate_authority(r, "finam_post_delete_enabled", True)),
        ("broker-dispatch-opened", lambda r: mutate_authority(r, "broker_dispatch_enabled", True)),
        ("runtime-live-opened", lambda r: mutate_authority(r, "runtime_live_enabled", True)),
        ("real-orders-opened", lambda r: mutate_authority(r, "real_orders_enabled", True)),
        ("stage8a5-opened", lambda r: mutate_authority(r, "stage8a5_authorized", True)),
        ("stage8a1-restoration-disabled", lambda r: mutate_authority(r, "stage8a1_r3_authority_restored", False)),
        ("broker-neutrality-disabled", lambda r: mutate_authority(r, "broker_neutral_runtime_dependency", False)),
        ("restart-without-i2-disabled", lambda r: mutate_authority(r, "production_restart_without_i2_candidate", False)),
        ("external-compile-fail-disabled", lambda r: mutate_authority(r, "external_raw_mutator_compile_fail", False)),
        ("raw-core-public", lambda r: mutate_text(r, checker.CORE, "pub(crate) fn stage8a4_internal_append_durable_batch", "pub fn stage8a4_internal_append_durable_batch")),
        ("raw-core-reexport", lambda r: mutate_text(r, checker.CORE_LIB, "apply_stage8a4_validated_writer_entry,", "apply_stage8a4_validated_writer_entry, stage8a4_internal_append_durable_batch,")),
        ("sealed-core-entry-removed", lambda r: mutate_text(r, checker.CORE, "pub fn apply_stage8a4_validated_writer_entry", "fn removed_stage8a4_validated_writer_entry")),
        ("sealed-verification-removed", lambda r: mutate_text(r, checker.CORE, "fn verify(", "fn verification_removed(")),
        ("stage8a1-owner-constructor-removed", lambda r: mutate_text(r, checker.STAGE8A1, "pub fn from_stage7b_owner", "fn removed_from_stage7b_owner", True)),
        ("stage8a1-caller-seal-constructor", lambda r: mutate_text(r, checker.STAGE8A1, "pub fn from_stage7b_owner", "pub fn from_current_stage6_authority")),
        ("local-readiness-lookalike", lambda r: mutate_text(r, checker.STAGE8A1, "Stage7bCompositeReadinessSnapshot", "Stage8a1CompositeReadinessSnapshot", True)),
        ("stage8a1-current-owner-call-removed", lambda r: mutate_text(r, checker.STAGE8A1, "authorize_stage8a1_durable_request", "removed_current_owner_authority", True)),
        ("place-continuation-revalidation-removed", lambda r: mutate_text(r, checker.STAGE8A1, "revalidate_place_capability", "removed_place_revalidation", True)),
        ("cancel-continuation-revalidation-removed", lambda r: mutate_text(r, checker.STAGE8A1, "revalidate_cancel_capability", "removed_cancel_revalidation", True)),
        ("runtime-finam-dependency", lambda r: mutate_text(r, checker.RUNTIME_CARGO, "[dependencies]", "[dependencies]\nfinam-gateway = { path = \"../finam-gateway\" }")),
        ("runtime-broker-specific-dependency", lambda r: mutate_text(r, checker.RUNTIME_CARGO, "[dependencies]", "[dependencies]\nbroker-finam = { path = \"../broker-finam\" }")),
        ("finam-composition-dependency-removed", lambda r: mutate_text(r, checker.FINAM_CARGO, 'runtime-durable-service = { path = "../runtime-durable-service" }', "# removed runtime composition dependency")),
        ("stage7-sealed-writer-removed", lambda r: mutate_text(r, checker.RUNTIME, "pub fn append_stage8a4_validated_entry_and_cover", "fn removed_stage8a4_validated_writer")),
        ("stage7-raw-batch-argument", lambda r: mutate_text(r, checker.RUNTIME, "entry: Stage6Stage8a4ValidatedWriteEntry", "batch: Stage6Stage8a4DurableBatch", True)),
        ("production-persist-operation-removed", lambda r: mutate_text(r, checker.I3, "pub fn reconcile_persist_and_cover_stage8a4", "fn removed_reconcile_persist_and_cover_stage8a4")),
        ("private-normal-issuer-removed", lambda r: mutate_text(r, checker.I3, "fn issue_private_durable_write_authority", "fn removed_private_durable_write_authority")),
        ("private-issuer-exported", lambda r: mutate_text(r, checker.I3, "fn issue_private_durable_write_authority", "pub fn issue_private_durable_write_authority")),
        ("raw-writer-parts-restored", lambda r: mutate_text(r, checker.I3, "pub struct Stage8a4DurableWriteAuthority", "pub struct Stage8a4DurableWriterParts;\npub struct Stage8a4DurableWriteAuthority")),
        ("production-recovery-entry-removed", lambda r: mutate_text(r, checker.I3, "pub fn recover_persisted_stage8a4_suffix_and_cover", "fn removed_persisted_stage8a4_suffix_recovery")),
        ("pending-recovery-material-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_pending_recovery_material", "removed_pending_recovery_material", True)),
        ("persisted-v2-reconstruction-removed", lambda r: mutate_text(r, checker.CORE, "pub fn recover_from_persisted_transition", "fn removed_recover_from_persisted_transition")),
        ("manifest-reconstruction-removed", lambda r: mutate_text(r, checker.REPLAY_V2, "pub(crate) fn reconstruct_stage8a4_suffix_from_v2", "fn removed_reconstruct_stage8a4_suffix_from_v2")),
        ("lost-i2-object-retained", lambda r: mutate_text(r, checker.RUNTIME, "drop(transition);", "let _lost_transition = &transition;", True)),
        ("truth-request-binding-removed", lambda r: mutate_text(r, checker.I3, "admission.admitted_request_id != identity.strategy_request_id()", "false")),
        ("truth-freshness-removed", lambda r: mutate_text(r, checker.I3, "admission.writer_entry_valid_until <= now", "false")),
        ("control-operational-binding-removed", lambda r: mutate_text(r, checker.I3, "controls.operational_identity_sha256() != current_operational_identity_sha256", "false")),
        ("owner-poison-assignment-removed", lambda r: mutate_text(r, checker.RUNTIME, "self.journal_mutation_uncertain = true", "self.journal_mutation_uncertain = false", True)),
        ("external-compile-fail-current-name-removed", lambda r: mutate_text(r, checker.COMPILE_FAIL, "stage8a4_internal_append_durable_batch", "removed_internal_append_name", True)),
        ("matrix-row-removed", lambda r: mutate_text(r, checker.MATRIX, "I3-060,I4 and Stage8A5 remain separately gated,authority/docs\n", "")),
        ("i3-r3-ref-drift", lambda r: mutate_authority(r, "rejected_i3_r3_ref", "4" * 40)),
        ("i3-r3-review-drift", lambda r: mutate_authority(r, "rejected_i3_r3_review_sha256", "4" * 64)),
        ("public-sealer-authorized", lambda r: mutate_authority(r, "sealed_authority_publicly_constructible", True)),
        ("pending-restart-disabled", lambda r: mutate_authority(r, "incomplete_restart_remains_pending", False)),
        ("authority-evidence-binding-disabled", lambda r: mutate_authority(r, "source_truth_control_bound_in_authority_hmac", False)),
        ("sealed-core-type-public", lambda r: mutate_text(r, checker.CORE, "struct Stage6Stage8a4SealedWriteAuthority", "pub struct Stage6Stage8a4SealedWriteAuthority")),
        ("pending-owner-removed", lambda r: mutate_text(r, checker.RUNTIME, "Stage8a4I3RecoveryPendingOwner", "RemovedPendingOwner", True)),
        ("pending-outcome-removed", lambda r: mutate_text(r, checker.RUNTIME, "Stage7bRestartOutcome::Stage8a4I3Pending", "Stage7bRestartOutcome::Ready", True)),
        ("production-source-bridge-removed", lambda r: mutate_text(r, checker.I3, "pub(crate) fn reconcile_persist_and_cover_stage8a4_from_production_sources", "fn removed_production_source_bridge")),
        ("production-context-issuer-removed", lambda r: mutate_text(r, checker.I3, "issue_durable_request_context_from_current_authority", "removed_context_issuer", True)),
        ("production-policy-issuer-removed", lambda r: mutate_text(r, checker.I3, "issue_stage8a4_policy_from_frozen_config", "removed_policy_issuer", True)),
        ("production-source-issuer-removed", lambda r: mutate_text(r, checker.I3, "issue_stage8a4_source_evidence_from_readonly_acquisition", "removed_source_issuer", True)),
        ("issuer-signature-verification-removed", lambda r: mutate_text(r, checker.CORE, "verify_stage8a4_writer_signature", "removed_writer_signature_verification", True)),
        ("issuer-key-pin-compare-removed", lambda r: mutate_text(r, checker.CORE, "stage8a4_writer_issuer_public_key_hex != entry.issuer_public_key_hex()", "false", True)),
        ("issuer-private-key-permission-check-removed", lambda r: mutate_text(r, checker.STAGE8A1, "metadata.permissions().mode() & 0o077 != 0", "false", True)),
        ("caller-forgeable-entry-issuer-restored", lambda r: mutate_text(r, checker.I3, "Stage6Stage8a4ValidatedWriteEntry::verify_issuer_attestation", "Stage6Stage8a4ValidatedWriteEntry::issue", True)),
        ("production-normal-integration-test-removed", lambda r: mutate_text(r, checker.I3, "stage8a4_i3_normal_production_path_persists_exact_batch_covers_s1_and_restarts_ready", "removed_normal_production_integration_test", True)),
        ("production-v2-only-recovery-test-removed", lambda r: mutate_text(r, checker.I3, "stage8a4_i3_production_recovery_repairs_v2_only_crash_and_covers_s1", "removed_v2_only_production_recovery_test", True)),
        ("production-partial-recovery-test-removed", lambda r: mutate_text(r, checker.I3, "stage8a4_i3_production_recovery_repairs_partial_exact_suffix_and_covers_s1", "removed_partial_production_recovery_test", True)),
        ("production-complete-before-s1-test-removed", lambda r: mutate_text(r, checker.I3, "stage8a4_i3_production_recovery_covers_complete_batch_without_s1", "removed_complete_before_s1_production_recovery_test", True)),
        ("production-recovery-raw-test-writer-restored", lambda r: mutate_text(r, checker.I3, "let (receipt, ready) = recover_persisted_stage8a4_suffix_and_cover(", "let (receipt, ready) = stage8a4_test_append_durable_batch_with_suffix_limit(", True)),
        ("complete-uncovered-batch-recovery-disabled", lambda r: mutate_text(r, checker.CORE, "let Some(batch) = mixed.reconciliation_batches().iter().find(|batch| {\n            Some(batch.last_mixed_record_id())", "let Some(batch) = mixed.reconciliation_batches().iter().find(|batch| {\n            batch.completion() == Stage6ReconciliationBatchCompletionV2::Incomplete\n                && Some(batch.last_mixed_record_id())", True)),
    ]
    if len(cases) != checker.NEGATIVE_CASES:
        raise SystemExit(f"stage8a4-durable-composition-i3-negative: FAIL inventory={len(cases)}")
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8a4-i3-r3-negative-") as raw:
        base = Path(raw)
        for name, mutation in cases:
            candidate = base / name
            copy_required(candidate)
            mutation(candidate)
            try:
                checker.check(candidate, git_scope=False)
            except Exception:
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-durable-composition-i3-negative: FAIL survived={name}")
    print(f"stage8a4-durable-composition-i3-negative: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
