#!/usr/bin/env python3
"""Mutation controls for the P1-PSR01 correction gate."""

from __future__ import annotations

import shutil
import tempfile
from pathlib import Path

import stage8b_p1e_i1_pre_seal_recovery_correction_check as checker


ROOT = Path(__file__).resolve().parents[1]
TRANSACTION = "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs"
SOURCE = "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs"
LIB = "crates/runtime-durable-service/src/lib.rs"
DOC = "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-correction.md"

MUTATIONS = (
    ("drop-admin-entry", TRANSACTION, "pub fn recover_stage8b_p1e_first_boot_pre_seal_administrative_v5", "fn removed_admin_entry"),
    ("drop-supervisor-recovery", TRANSACTION, "pub fn recover_stage8b_p1e_first_boot_pre_seal_from_supervisor_v5", "fn removed_supervisor_recovery"),
    ("drop-durable-marker-binding", TRANSACTION, "fn marker_matches_durable_recovery", "fn removed_durable_marker_binding"),
    ("drop-transaction-self-binding", TRANSACTION, "transaction_id_sha256(\n            &marker.operational_identity_sha256", "derive_removed(\n            &marker.operational_identity_sha256"),
    ("drop-runtime-profile-binding", TRANSACTION, "marker.runtime_profile_sha256 == STAGE8B_P1E_RUNTIME_PROFILE_SHA256", "true"),
    ("drop-source-plan-binding", TRANSACTION, "marker.source_plan_sha256 == STAGE8B_P1E_FIRST_BOOT_SOURCE_PLAN_V2_SHA256", "true"),
    ("drop-fresh-classification", TRANSACTION, "inspection.required_action != selector.action.as_str()", "false && inspection.required_action != selector.action.as_str()"),
    ("drop-historical-policy", SOURCE, "enum FirstBootTruthPolicy", "enum RemovedTruthPolicy"),
    ("weaken-fresh-age", SOURCE, "truth_policy == FirstBootTruthPolicy::FreshAdmission", "false"),
    ("drop-byte-exact-source", SOURCE, "source.source_bundle_sha256 == binding.source_bundle_sha256", "true"),
    ("drop-history-binding", SOURCE, "source.history_bars_sha256 == binding.history_bars_sha256", "true"),
    ("drop-admin-downtime-test", SOURCE, "pre_seal_administrative_recovery_remains_available_after_long_downtime", "removed_admin_downtime_test"),
    ("drop-admin-negative-test", SOURCE, "administrative_recovery_rejects_wrong_selector_and_marker_auth_without_mutation", "removed_admin_negative_test"),
    ("drop-boundary-test", SOURCE, "historical_continuation_is_marker_bound_at_300_301_and_long_downtime", "removed_boundary_test"),
    ("drop-changed-bundle-test", SOURCE, "historical_continuation_rejects_changed_bundle_without_mutation", "removed_changed_bundle_test"),
    ("drop-admin-export", LIB, "recover_stage8b_p1e_first_boot_pre_seal_administrative_v5", "removed_admin_export"),
    ("drop-psr01-contract", DOC, "P1-PSR01", "removed-finding"),
)

FILES = (TRANSACTION, SOURCE, LIB, DOC)


def main() -> None:
    passed = 0
    for name, relative, old, new in MUTATIONS:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-p1e-psr01-{name}-") as directory:
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
        "PASS stage8b-p1e-i1-pre-seal-recovery-correction-negative-harness "
        f"{passed}/{len(MUTATIONS)}"
    )


if __name__ == "__main__":
    main()
