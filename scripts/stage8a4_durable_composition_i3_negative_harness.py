#!/usr/bin/env python3
"""Mutation harness proving the I3 R2 checker fails closed."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_i3_check as checker

ROOT = Path(__file__).resolve().parents[1]


def copy_required(destination: Path) -> None:
    for relative in checker.REQUIRED:
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing mutation anchor: {relative}: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def mutate_all(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing mutation anchor: {relative}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def mutate_authority(root: Path, key: str, value: object) -> None:
    path = root / checker.AUTHORITY
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    cases = [
        ("i2-predecessor-drift", lambda r: mutate_authority(r, "accepted_i2_r3_ref", "0" * 40)),
        ("i2-review-hash-drift", lambda r: mutate_authority(r, "accepted_i2_r3_review_sha256", "0" * 64)),
        ("i3-r1-ref-drift", lambda r: mutate_authority(r, "rejected_i3_r1_ref", "1" * 40)),
        ("i3-r1-review-drift", lambda r: mutate_authority(r, "rejected_i3_r1_review_sha256", "1" * 64)),
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
        ("runtime-live-opened", lambda r: mutate_authority(r, "runtime_live_enabled", True)),
        ("stage8a5-opened", lambda r: mutate_authority(r, "stage8a5_authorized", True)),
        ("raw-stage7-writer-restored", lambda r: mutate_text(r, checker.RUNTIME, "pub fn append_stage8a4_durable_authority_and_cover", "pub fn append_stage8a4_durable_batch_and_cover")),
        ("sealed-stage7-entry-removed", lambda r: mutate_text(r, checker.RUNTIME, "pub fn append_stage8a4_durable_authority_and_cover", "pub fn removed_stage8a4_authority_entry")),
        ("raw-core-append-restored", lambda r: mutate_text(r, checker.CORE, "pub fn stage8a4_internal_append_durable_batch", "pub fn append_stage8a4_durable_batch")),
        ("raw-core-reexport-restored", lambda r: mutate_text(r, checker.CORE_LIB, "pub use stage6d_live_core::stage8a4_internal_append_durable_batch", "pub use stage6d_live_core::append_stage8a4_durable_batch")),
        ("private-issuer-exported", lambda r: mutate_text(r, checker.I3, "fn issue_private_durable_write_authority", "pub fn issue_private_durable_write_authority")),
        ("opaque-authority-removed", lambda r: mutate_text(r, checker.I3, "pub struct Stage8a4DurableWriteAuthority", "pub struct RemovedDurableWriteAuthority")),
        ("raw-writer-compile-fail-removed", lambda r: mutate_all(r, checker.RUNTIME_LIB, "append_stage8a4_durable_batch_and_cover", "removed_raw_writer_proof")),
        ("opaque-compile-fail-removed", lambda r: mutate_all(r, checker.RECONCILIATION, "Stage8a4DurableWriteAuthority", "RemovedDurableWriteAuthority")),
        ("truth-request-binding-removed", lambda r: mutate_text(r, checker.I3, "admission.admitted_request_id != identity.strategy_request_id()", "false")),
        ("truth-account-binding-removed", lambda r: mutate_text(r, checker.I3, "&admission.admitted_account_id != identity.account_id()", "false")),
        ("truth-instrument-binding-removed", lambda r: mutate_text(r, checker.I3, "&admission.admitted_instrument != identity.instrument()", "false")),
        ("truth-durable-binding-removed", lambda r: mutate_text(r, checker.I3, "admission.admitted_durable_binding_sha256 != durable_binding.as_str()", "false")),
        ("truth-freshness-removed", lambda r: mutate_text(r, checker.I3, "admission.writer_entry_valid_until <= now", "false")),
        ("control-operational-binding-removed", lambda r: mutate_text(r, checker.I3, "controls.operational_identity_sha256() != current_operational_identity_sha256", "false")),
        ("control-runtime-binding-removed", lambda r: mutate_all(r, checker.I3, "controls.runtime_config_fingerprint_sha256()", "removed_runtime_config_binding()")),
        ("control-scope-binding-removed", lambda r: mutate_all(r, checker.I3, "controls.authority_scope_sha256()", "removed_authority_scope()")),
        ("arm-registry-binding-removed", lambda r: mutate_all(r, checker.STAGE8A1, "read_arm_registration", "removed_arm_registration")),
        ("mutation-classification-removed", lambda r: mutate_all(r, checker.CORE, "JournalMutationMayHaveOccurred", "RemovedMutationUncertainty")),
        ("owner-sticky-flag-removed", lambda r: mutate_all(r, checker.RUNTIME, "journal_mutation_uncertain: bool", "mutation_poison_removed: bool")),
        ("owner-poison-assignment-removed", lambda r: mutate_all(r, checker.RUNTIME, "self.journal_mutation_uncertain = true", "self.journal_mutation_uncertain = false")),
        ("recovery-ready-poison-guard-removed", lambda r: mutate_text(r, checker.RUNTIME, "if self.seal_commit_uncertain || self.journal_mutation_uncertain", "if self.seal_commit_uncertain")),
        ("poisoned-seal-advance-assertion-removed", lambda r: mutate_all(r, checker.RUNTIME, "owner.advance_recovery_seal(&setup.key).is_err()", "true")),
        ("v2-fault-matrix-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_i3_post_write_fault_matrix_poison_is_sticky_in_process", "removed_v2_fault_matrix")),
        ("suffix-fault-matrix-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_i3_suffix_post_write_faults_are_sticky_in_process", "removed_suffix_fault_matrix")),
        ("prewrite-test-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_i3_pre_write_failure_does_not_poison_owner", "removed_prewrite_test")),
        ("v2-crash-test-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_i3_restart_covers_v2_only_crash_then_repairs_exact_suffix", "removed_v2_crash_test")),
        ("unrelated-suffix-test-removed", lambda r: mutate_text(r, checker.RUNTIME, "stage8a4_i3_restart_rejects_unrelated_record_after_uncovered_v2", "removed_unrelated_suffix_test")),
        ("transport-added", lambda r: mutate_text(r, checker.I3, "use super::super", "use reqwest::Method;\nuse super::super")),
        ("matrix-row-removed", lambda r: mutate_text(r, checker.MATRIX, "I3-060,I4 and Stage8A5 remain separately gated,authority/docs\n", "")),
    ]
    if len(cases) != checker.NEGATIVE_CASES:
        raise SystemExit(f"stage8a4-durable-composition-i3-negative: FAIL inventory={len(cases)}")
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8a4-i3-r2-negative-") as raw:
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
