#!/usr/bin/env python3
"""Mutation harness proving the I1 semantic checker fails closed."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_i1_check as checker

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
        raise RuntimeError(f"mutation anchor absent: {relative}: {old}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def mutate_authority(root: Path, key: str, value: object) -> None:
    path = root / checker.AUTHORITY
    data = json.loads(path.read_text(encoding="utf-8"))
    data[key] = value
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def main() -> None:
    cases = [
        ("accepted-ref-drift", lambda root: mutate_authority(root, "accepted_implementation_spec_r2_ref", "0" * 40)),
        ("review-hash-drift", lambda root: mutate_authority(root, "accepted_implementation_spec_r2_review_sha256", "0" * 64)),
        ("status-opened", lambda root: mutate_authority(root, "status", "accepted")),
        ("version-three-opened", lambda root: mutate_authority(root, "supported_record_schema_versions", [1, 2, 3])),
        ("writer-opened", lambda root: mutate_authority(root, "v2_writer_enabled", True)),
        ("apply-opened", lambda root: mutate_authority(root, "durable_apply_enabled", True)),
        ("redis-opened", lambda root: mutate_authority(root, "redis_live_enabled", True)),
        ("finam-opened", lambda root: mutate_authority(root, "finam_post_delete_enabled", True)),
        ("runtime-opened", lambda root: mutate_authority(root, "runtime_live_enabled", True)),
        ("golden-count-reduced", lambda root: mutate_authority(root, "golden_case_count", 19)),
        ("dto-field-removed", lambda root: mutate_text(root, checker.SOURCE, "    source_ts: DateTime<Utc>,\n    received_ts: DateTime<Utc>,", "    source_ts: DateTime<Utc>,\n    trade_receipt: DateTime<Utc>,")),
        ("cas-field-removed", lambda root: mutate_text(root, checker.SOURCE, "expected_request_state_fingerprint", "expected_state")),
        ("generic-deserialize-bypass", lambda root: mutate_text(root, checker.SOURCE, "impl From<Stage6JournalRecordWireV2>", "impl<'de> Deserialize<'de> for Stage6JournalRecordV2 /* impl From<Stage6JournalRecordWireV2> */")),
        ("writer-api-added", lambda root: mutate_text(root, checker.SOURCE, "impl Stage6VersionedJournalReader {", "impl Stage6VersionedJournalReader {\n    pub fn append() {}")),
        ("stable-key-test-removed", lambda root: mutate_text(root, checker.SOURCE, "same_stable_transition_key_with_different_v2_payload_fails_closed", "stable_key_test_removed")),
        ("full-record-test-removed", lambda root: mutate_text(root, checker.SOURCE, "exact_duplicate_v2_is_idempotent_but_suffix_source_or_causality_drift_fails", "full_record_test_removed")),
        ("dependency-inversion", lambda root: mutate_text(root, checker.CARGO, "[dependencies]", "[dependencies]\nfinam-gateway = { path = \"../finam-gateway\" }")),
        ("module-public", lambda root: mutate_text(root, checker.LIB, "mod stage6_reconciliation_v2;", "pub mod stage6_reconciliation_v2;")),
        ("golden-changed", lambda root: mutate_text(root, checker.GOLDEN, "520903fbf5130ce54fce5be3b74233a0d74a8a4c53f6f402381ec4c95ef4ead2", "0" * 64)),
        ("matrix-row-removed", lambda root: mutate_text(root, checker.MATRIX, "I1-040,Writer apply Redis FINAM dispatch runtime-live and real orders remain closed,authority and checker\n", "")),
        ("terminal-filled-accepts-zero", lambda root: mutate_text(
            root,
            checker.SOURCE,
            "OrderStatus::Filled, Stage6ReconciliationFillEffectV2::Full { filled_qty }",
            "OrderStatus::Filled, Stage6ReconciliationFillEffectV2::Zero",
        )),
        ("terminal-rejected-accepts-nonzero", lambda root: mutate_text(
            root,
            checker.SOURCE,
            "OrderStatus::Rejected, Stage6ReconciliationFillEffectV2::Zero",
            "OrderStatus::Rejected, Stage6ReconciliationFillEffectV2::Partial { filled_qty: _ }",
        )),
        ("terminal-cancelled-accepts-full", lambda root: mutate_text(
            root,
            checker.SOURCE,
            "OrderStatus::Canceled, Stage6ReconciliationFillEffectV2::Partial { filled_qty }",
            "OrderStatus::Canceled, Stage6ReconciliationFillEffectV2::Full { filled_qty }",
        )),
        ("working-accepts-partial", lambda root: mutate_text(
            root,
            checker.SOURCE,
            "OrderStatus::New | OrderStatus::Working, Stage6ReconciliationFillEffectV2::Zero",
            "OrderStatus::New | OrderStatus::Working, Stage6ReconciliationFillEffectV2::Partial { filled_qty: _ }",
        )),
        ("stale-governance-active-candidate", lambda root: mutate_text(
            root,
            checker.CURRENT_STATUS,
            "I1 R2 is\n  the sole active candidate and its independent acceptance is pending",
            "Corrected implementation Specification R2 is\n  the sole active candidate and its independent acceptance is pending",
        )),
    ]
    passed = 0
    with tempfile.TemporaryDirectory(prefix="stage8a4-i1-negative-") as raw:
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
                raise SystemExit(f"stage8a4-durable-composition-i1-negative: FAIL survived={name}")
    if passed != len(cases):
        raise SystemExit("stage8a4-durable-composition-i1-negative: FAIL inventory")
    print(f"stage8a4-durable-composition-i1-negative: PASS {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
