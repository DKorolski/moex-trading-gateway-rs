#!/usr/bin/env python3
"""Mutation harness proving the I2 checker fails closed."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_i2_check as checker

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


def mutate_authority(root: Path, key: str, value: object) -> None:
    path = root / checker.AUTHORITY
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    cases = [
        ("predecessor-drift", lambda root: mutate_authority(root, "accepted_i1_r2_ref", "0" * 40)),
        ("review-hash-drift", lambda root: mutate_authority(root, "accepted_i1_r2_review_sha256", "0" * 64)),
        ("premature-acceptance", lambda root: mutate_authority(root, "status", "accepted")),
        ("append-opened", lambda root: mutate_authority(root, "durable_append_enabled", True)),
        ("redis-opened", lambda root: mutate_authority(root, "redis_live_enabled", True)),
        ("private-outcome-public", lambda root: mutate_text(root, checker.REDUCER, "struct Stage8a4AuthoritativeReconciliationOutcome", "pub struct Stage8a4AuthoritativeReconciliationOutcome")),
        ("diagnostic-authority", lambda root: mutate_text(root, checker.REDUCER, "    outcome_kind: Stage8a4OutcomeKind,", "    diagnostic: Stage8a4ReconciliationDiagnostic,")),
        ("module-public", lambda root: mutate_text(root, checker.REDUCER, "mod durable_composition_i2;", "pub mod durable_composition_i2;")),
        ("candidate-public", lambda root: mutate_text(root, checker.SOURCE, "struct Stage8a4I2DurableCandidate", "pub struct Stage8a4I2DurableCandidate")),
        ("builder-public", lambda root: mutate_text(root, checker.SOURCE, "fn build_private_durable_candidate", "pub fn build_private_durable_candidate")),
        ("stable-domain-drift", lambda root: mutate_text(root, checker.SOURCE, "stage8a4-stable-transition-key-v1", "stage8a4-stable-transition-key-v2")),
        ("stable-mutable-generation", lambda root: mutate_text(root, checker.SOURCE, "&transition_bytes,", "&transition_bytes,\n            &input.pre_append.expected_recovery_seal_generation.to_be_bytes(),")),
        ("cas-field-removed", lambda root: mutate_text(root, checker.SOURCE, "struct PrivatePreAppendEvidence {\n    expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest,\n    expected_recovery_seal_generation: u64,\n    expected_recovery_seal_fingerprint: Stage6Sha256Digest,\n    expected_request_state_fingerprint: Stage6Sha256Digest,", "struct PrivatePreAppendEvidence {\n    expected_stage6_checkpoint_or_frontier_fingerprint: Stage6Sha256Digest,\n    expected_recovery_seal_generation: u64,\n    expected_recovery_seal_fingerprint: Stage6Sha256Digest,\n    expected_state: Stage6Sha256Digest,")),
        ("suffix-hash-removed", lambda root: mutate_text(root, checker.SOURCE, "struct PrivateSuffixManifestEntry {\n    ordinal: u16,\n    event_kind: Stage6JournalEventKind,\n    journal_record_id: Stage6JournalRecordId,\n    lifecycle_sequence: Stage6LifecycleSequence,\n    canonical_payload_sha256: Stage6Sha256Digest,\n    canonical_record_sha256: Stage6Sha256Digest,", "struct PrivateSuffixManifestEntry {\n    ordinal: u16,\n    event_kind: Stage6JournalEventKind,\n    journal_record_id: Stage6JournalRecordId,\n    lifecycle_sequence: Stage6LifecycleSequence,\n    canonical_payload_sha256: Stage6Sha256Digest,\n    record_hash: Stage6Sha256Digest,")),
        ("not-found-exact", lambda root: mutate_text(root, checker.SOURCE, "PrivateTransitionKind::ReconciliationConflictHold", "PrivateTransitionKind::Exact { lifecycle: Stage8a4ExactLifecycle::Working }")),
        ("unavailable-exact", lambda root: mutate_text(root, checker.SOURCE, "Stage8a4PrivateExactLookup::Unavailable", "Stage8a4PrivateExactLookup::NotAttempted /* unavailable */")),
        ("cancel-working-test-removed", lambda root: mutate_text(root, checker.TESTS, "cancel_working_remains_unresolved_without_suffix", "cancel_working_test_removed")),
        ("fabrication-test-removed", lambda root: mutate_text(root, checker.TESTS, "place_without_broker_id_never_fabricates_order_or_trade_suffix", "fabrication_test_removed")),
        ("append-api-added", lambda root: mutate_text(root, checker.SOURCE, "fn build_private_durable_candidate(", "fn append() {}\nfn build_private_durable_candidate(")),
        ("matrix-row-removed", lambda root: mutate_text(root, checker.MATRIX, "I2-042,no ACK readiness Redis FINAM dispatch runtime-live,checker\n", "")),
        ("failed-exact-source-producer-removed", lambda root: mutate_text(root, checker.REDUCER, "    exact_lookup: Stage8a4PrivateExactLookup,", "    exact_lookup_removed: Stage8a4PrivateExactLookup,")),
        ("attempted-failure-downgraded", lambda root: mutate_text(root, checker.REDUCER, "    let exact_lookup = admission.exact_lookup;", "    let exact_lookup = Stage8a4PrivateExactLookup::NotAttempted;")),
        ("cancel-rejected-command-rejection", lambda root: mutate_text(root, checker.SOURCE, "Stage6CancelOutcomeV1::AlreadyTerminalNonExecution\n                }\n                Stage8a4ExactLifecycle::TerminalCancelled", "Stage6CancelOutcomeV1::Rejected\n                }\n                Stage8a4ExactLifecycle::TerminalCancelled")),
        ("cancel-expired-command-rejection", lambda root: mutate_text(root, checker.SOURCE, "Stage8a4ExactLifecycle::TerminalExpired => {\n                    Stage6CancelOutcomeV1::AlreadyTerminalNonExecution", "Stage8a4ExactLifecycle::TerminalExpired => {\n                    Stage6CancelOutcomeV1::Rejected")),
        ("canonical-orphan-shortcut", lambda root: mutate_text(root, checker.REDUCER, "    let summary = truth.summarize_for_instrument(target);", "    let summary = weak_two_id_none_summary(truth, target);")),
        ("filled-without-trade-coverage-removed", lambda root: mutate_text(root, checker.TESTS, "account_safety_uses_canonical_broker_truth_for_all_orphan_classes", "account_safety_orphan_coverage_removed")),
        ("trade-projection-requires-selected-id", lambda root: mutate_text(root, checker.SOURCE, "            if let Some(order_id) = projected_trade_order_id {", "            if let Some(order_id) = selected_order_id {")),
        ("multi-trade-id-ambiguity-accepted", lambda root: mutate_text(root, checker.SOURCE, "                    if material_broker_ids.len() > 1 {", "                    if false && material_broker_ids.len() > 1 {")),
    ]
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8a4-i2-negative-") as raw:
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
                raise SystemExit(f"stage8a4-durable-composition-i2-negative: FAIL survived={name}")
    if passed != len(cases):
        raise SystemExit("stage8a4-durable-composition-i2-negative: FAIL inventory")
    print(f"stage8a4-durable-composition-i2-negative: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
