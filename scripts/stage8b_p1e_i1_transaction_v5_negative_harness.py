#!/usr/bin/env python3
"""Mutation controls for the Stage 8B-P1-e transaction V5 static gate."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1e_i1_transaction_v5_check as checker


ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = (
    ("drop-v5-entry", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "pub fn first_boot_stage8b_p1e_transaction_v5", "fn first_boot_stage8b_p1e_transaction_v5_removed"),
    ("drop-exactly-one", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "if matches.len() != 1", "if matches.is_empty()"),
    ("downgrade-receipt", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION: u16 = 2", "STAGE8B_P1E_FIRST_BOOT_RECEIPT_SCHEMA_VERSION: u16 = 1"),
    ("drop-cross-validation", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "cross_validate_adopted(", "cross_validate_adopted_removed("),
    ("drop-receipt-sync-hook", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", 'observer("after-receipt-temp-sync-before-final-rename")', 'observer("removed-receipt-hook")'),
    ("drop-adopt-recovery", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "AdoptCommittedRoot", "RemovedAdoptionAction"),
    ("drop-corrupt-class", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs", "CorruptOrIdentityMismatch", "RemovedCorruptClass"),
    ("drop-source-entry", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs", "first_boot_stage8b_p1e_transaction_v5(", "first_boot_stage8b_p1e_transaction_v5_removed("),
    ("drop-crash-matrix", "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs", "every_v5_crash_hook_has_one_exact_fail_closed_classification", "removed_v5_crash_matrix"),
    ("drop-linear-seam", "crates/runtime-durable-service/src/recovery.rs", "Stage7bP1eJournalDurableFirstBoot", "RemovedJournalDurableFirstBoot"),
    ("drop-preserving-replacement", "crates/runtime-durable-service/src/recovery.rs", "replace_stage5g_in_stage6d_restart_package(", "seal_stage6d_restart_package("),
    ("drop-v2-seal", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "pub fn seal_stage6d_restart_package_v2", "fn seal_stage6d_restart_package_v2_removed"),
    ("drop-v2-replacement", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "pub fn replace_stage5g_in_stage6d_restart_package_v2", "fn replace_stage5g_in_stage6d_restart_package_v2_removed"),
    ("drop-v2-preservation-test", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "stage8b_p1e_v2_advance_preserves_exact_first_boot_provenance", "removed_v2_preservation_test"),
    ("drop-v2-negative-test", "crates/strategy-runtime-core/src/stage6d_live_core.rs", "stage8b_p1e_v2_missing_or_changed_provenance_fails_closed", "removed_v2_negative_test"),
    ("drop-v2-hmac", "crates/strategy-runtime-core/src/stage5g_clean_restart.rs", "stage6d_v2_hmac_sha256", "removed_stage6d_v2_hmac"),
    ("drop-transaction-module", "crates/runtime-durable-service/src/lib.rs", "mod stage8b_p1e_first_boot_transaction;", "// removed transaction module"),
    ("drop-admin-boundary-doc", "docs/stage-8/stage8b-p1e-i1-transaction-v5-implementation.md", "Pre-seal administrative actions", "Deferred actions"),
)


def main() -> None:
    passed = 0
    for name, relative, old, new in MUTATIONS:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1e-v5-{name}-") as directory:
            root = Path(directory)
            for source in (
                "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
                "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
                "crates/runtime-durable-service/src/recovery.rs",
                "crates/runtime-durable-service/src/lib.rs",
                "crates/strategy-runtime-core/src/stage6d_live_core.rs",
                "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
                "crates/strategy-runtime-core/src/lib.rs",
                "docs/stage-8/stage8b-p1e-i1-transaction-v5-implementation.md",
            ):
                target = root / source
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / source, target)
            target = root / relative
            body = target.read_text(encoding="utf-8")
            if old not in body:
                raise SystemExit(f"mutation source missing: {name}")
            target.write_text(body.replace(old, new), encoding="utf-8")
            try:
                checker.check_content(root)
            except (OSError, ValueError):
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"mutation was accepted: {name}")
    print(f"PASS stage8b-p1e-i1-transaction-v5-negative-harness {passed}/{len(MUTATIONS)}")


if __name__ == "__main__":
    main()
