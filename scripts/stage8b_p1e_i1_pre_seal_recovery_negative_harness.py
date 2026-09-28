#!/usr/bin/env python3
"""Mutation controls for the pre-seal administrative recovery gate."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1e_i1_pre_seal_recovery_check as checker


ROOT = Path(__file__).resolve().parents[1]
TRANSACTION = "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs"
SOURCE = "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
RECOVERY = "crates/runtime-durable-service/src/recovery.rs"
BOOTSTRAP = "crates/runtime-durable-service/src/stage8b_p1_bootstrap.rs"
LIB = "crates/runtime-durable-service/src/lib.rs"
DOC = "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-implementation.md"

MUTATIONS = (
    ("drop-authorizer", TRANSACTION, "pub fn authorize_stage8b_p1e_pre_seal_recovery_v5", "fn removed_authorizer"),
    ("drop-recovery-entry", TRANSACTION, "pub fn recover_stage8b_p1e_first_boot_pre_seal_v5", "fn removed_recovery_entry"),
    ("weaken-confirmation", TRANSACTION, "RECOVER_EXISTING_STAGE8B_P1_FIRST_BOOT_V4", "RECOVER_ANY_FIRST_BOOT"),
    ("drop-action-match", TRANSACTION, "inspection.required_action != selector.action.as_str()", "false && inspection.required_action != selector.action.as_str()"),
    ("drop-transaction-match", TRANSACTION, "expected_transaction_id != selector.transaction_id_sha256", "false && expected_transaction_id != selector.transaction_id_sha256"),
    ("drop-resume", TRANSACTION, 'Self::ResumePrepared => "resume-prepared"', 'Self::ResumePrepared => "removed"'),
    ("drop-marker-completion", TRANSACTION, "complete_marker_temp_transition(", "removed_marker_temp_transition("),
    ("drop-quarantine", TRANSACTION, "quarantine_incomplete_root(", "removed_quarantine_root("),
    ("drop-finalize", TRANSACTION, "finalize_quarantine(", "removed_quarantine_finalization("),
    ("drop-generation-history", TRANSACTION, "require_generation_after_quarantine_history(", "removed_generation_history("),
    ("drop-noreplace", TRANSACTION, "libc::RENAME_NOREPLACE", "0"),
    ("drop-quarantine-parent-sync", TRANSACTION, "sync_directory(&quarantine_parent)?", "removed_sync_quarantine_parent()?"),
    ("drop-quarantine-root-sync", TRANSACTION, "sync_directory(&quarantine_root)?", "removed_sync_quarantine_root()?"),
    ("drop-existing-journal-seam", RECOVERY, "resume_stage8b_p1e_journal_durable_first_boot", "removed_existing_journal_seam"),
    ("recreate-journal", RECOVERY, "Stage7bWritableDurableAuthority::open_existing(root, &identity)?", "Stage7bWritableDurableAuthority::create_new(root, &identity, &authorization)?"),
    ("drop-selector-test", SOURCE, "pre_seal_recovery_requires_exact_selector_and_completes_all_continuable_frontiers", "removed_selector_test"),
    ("drop-quarantine-test", SOURCE, "pre_seal_remove_and_quarantine_actions_are_durable_and_generation_guarded", "removed_quarantine_test"),
    ("drop-response-loss-test", SOURCE, "pre_seal_response_loss_reclassifies_without_repeating_completed_effects", "removed_response_loss_test"),
    ("drop-opaque-config-seam", BOOTSTRAP, "duplicate_for_internal_classification", "removed_internal_classification"),
    ("drop-selector-export", LIB, "Stage8bP1ePreSealRecoverySelectorV5", "RemovedPreSealSelector"),
    ("drop-closed-surface-doc", DOC, "Operational Redis DB0/DB15", "Operational boundary"),
)

FILES = (TRANSACTION, SOURCE, RECOVERY, BOOTSTRAP, LIB, DOC)


def main() -> None:
    passed = 0
    for name, relative, old, new in MUTATIONS:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1e-pre-seal-{name}-") as directory:
            root = Path(directory)
            for source in FILES:
                target = root / source
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / source, target)
            target = root / relative
            body = target.read_text(encoding="utf-8")
            if old not in body:
                raise SystemExit(f"mutation source missing: {name}")
            target.write_text(body.replace(old, new, 1), encoding="utf-8")
            try:
                checker.check_content(root)
            except (OSError, ValueError):
                passed += 1
                print(f"PASS {name}")
            else:
                raise SystemExit(f"mutation was accepted: {name}")
    print(
        f"PASS stage8b-p1e-i1-pre-seal-recovery-negative-harness "
        f"{passed}/{len(MUTATIONS)}"
    )


if __name__ == "__main__":
    main()
