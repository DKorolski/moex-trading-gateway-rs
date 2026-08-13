#!/usr/bin/env python3
"""Build and validate the exact 80-row Stage 7B semantic proof map."""
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-7/STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv"
OUTPUT = ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json"

FOUNDATION_WITNESSES = {
    "B-001": ("git_gate", "scripts/stage7b_b_check.py::check_lineage"),
    "B-002": ("governance_gate", "scripts/stage7b_b_check.py::check_governance"),
    "B-003": ("static_gate", "scripts/stage7b_b_closed_surface_check.py"),
    "B-004": ("static_gate", "scripts/stage7b_b_check.py::check_dependencies"),
    "B-008": ("unit", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-010": ("unit", "stage6d_first_boot_requires_explicit_create_authority + stage7b_b_first_boot_creation_requires_matching_linear_authorization"),
    "B-011": ("fs_integration", "stage7b_open_existing_never_creates_missing_journal"),
    "B-012": ("fs_integration", "stage7b_create_new_and_open_existing_are_explicit_and_disjoint"),
    "B-014": ("fs_integration", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-015": ("unit", "stage6b_torn_write_failpoints_leave_reopen_fail_closed"),
    "B-016": (
        "unit",
        "stage7b_same_length_earlier_record_mutation_is_detected_before_append + stage7b_same_length_last_record_mutation_is_detected_before_append",
    ),
    "B-017": ("unit", "stage6b_sync_failure_returns_durability_uncertain_without_receipt"),
    "B-018": ("unit", "stage6b_file_receipt_is_returned_after_sync_path"),
    "B-019": (
        "unit",
        "stage7b_memory_file_checkpoint_and_replay_fingerprints_are_identical",
    ),
    "B-020": (
        "fs_integration",
        "stage7b_file_reopen_checkpoint_and_replay_fingerprints_are_identical",
    ),
    "B-009": ("negative", "anchored root/openat witnesses + direct post-validation full-digest rebind rejection"),
    "B-021": ("subprocess", "root-FD and sidecar flock before openat journal + root-race barrier witness"),
    "B-022": ("subprocess", "normal and replaced-lock-path second-writer rejection witnesses"),
    "B-023": ("subprocess", "stage7b_b_second_process_is_rejected_and_sigkill_releases_kernel_lock"),
    "B-024": ("integration", "linear authority owns anchored root FD, root/sidecar leases and journal for full lifetime"),
    "B-025": ("ordered_trace", "STAGE7B_STORAGE_OPEN_ORDER ends at StorageReady; crate has no Redis dependency"),
    "B-026": ("compile_fail", "root and writable authority linear compile-fail doctests + privacy checker"),
    "B-027": ("integration", "first_boot_requires_stage5g_seed"),
    "B-028": ("negative", "invalid_stage5g_seed_rejected_before_journal_creation"),
    "B-029": ("ordered_trace", "initial_recovery_seal_before_ready_and_lease_lifetime"),
    "B-030": ("unit+static", "recovery_seal_canonical_roundtrip_and_restart + Stage7bRecoverySealV1 deny_unknown_fields"),
    "B-031": ("fault_injection", "stage7b_c_b032_sigkill_after_temp_sync_keeps_old_committed_seal + root-FD-relative temp/sync/rename/root-sync/reread ordering"),
    "B-032": ("subprocess_fault", "stage7b_c_b032_sigkill_after_temp_sync_keeps_old_committed_seal"),
    "B-033": ("negative", "corrupt_recovery_seal_rejected_and_blocked_has_zero_effect"),
    "B-034": ("integration", "stage7b_c_b034_authenticated_checkpoint_ahead_of_file_journal_blocks"),
    "B-035": ("fs_integration", "seal_without_journal_rejected_without_creating_journal"),
    "B-036": ("fs_integration", "journal_without_seal_is_explicit_recovery_blocked"),
    "B-037": ("negative", "recovery_operational_identity_mismatch_is_blocked"),
    "B-038": ("negative", "recovery_hmac_digest_mismatch_is_blocked + domain-separated Stage7B seal HMAC verification"),
    "B-039": ("restart_integration", "stage7b_c_b039_finalized_file_journal_ahead_restarts_ready"),
    "B-040": ("restart_integration", "stage7b_c_b040_unbound_nonfinal_file_journal_blocks_without_effect"),
    "B-041": ("restart_integration", "stage7b_c_b041_cross_bound_active_file_journal_preserves_dispatch_safety"),
    "B-042": ("integration", "corrupt_recovery_seal_rejected_and_blocked_has_zero_effect + RecoveryBlocked false capabilities"),
    "B-071": ("governance_gate", "stage7b-entry-descriptor.json + stage7b-c-entry-descriptor.json current and cross-checked; exactly-once claim false"),
    "B-075": ("inherited+current_gate", "scripts/stage7b_c_r1_gate.sh + inherited scripts/stage7b_b_gate.sh"),
    "B-076": ("negative_harness", "scripts/stage7b_c_negative_harness.py cases=34 descriptor-pinned"),
    "B-079": ("closed_surface", "scripts/stage7b_c_closed_surface_check.py"),
}


def build() -> dict:
    with MATRIX.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 80 or [row["ID"] for row in rows] != [f"B-{i:03d}" for i in range(1, 81)]:
        raise SystemExit("stage7b-proof-map: frozen matrix IDs/count drift")
    proofs = []
    for row in rows:
        implemented = row["ID"] in FOUNDATION_WITNESSES
        proof_type, witness = FOUNDATION_WITNESSES.get(
            row["ID"],
            (row["Proof Type"], f"pending Stage 7B follow-up: {row['Required Witness']}"),
        )
        proofs.append(
            {
                "row_id": row["ID"],
                "requirement": row["Scenario / Requirement"],
                "proof_type": proof_type,
                "rationale": (
                    "Stage 7B accepted foundation or exact Stage 7B-b/7B-c witness"
                    if implemented
                    else "Frozen requirement retained pending its designated Stage 7B slice"
                ),
                "artifact": (
                    "Stage 7B-b-R2 inherited gate + Stage 7B-c gate"
                    if implemented
                    else "pending"
                ),
                "exact_witness": witness,
                "status": "implemented" if implemented else "pending",
            }
        )
    return {
        "schema_version": 1,
        "stage": "7B",
        "slice": "7B-c-R1",
        "accepted_predecessor": "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64",
        "accepted_slice_predecessor": "ff3fa2e8908440863b40b838991d4716b33caad4",
        "row_count": len(proofs),
        "implemented_count": sum(p["status"] == "implemented" for p in proofs),
        "pending_count": sum(p["status"] == "pending" for p in proofs),
        "stage7b_accepted": False,
        "proofs": proofs,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    expected = build()
    if args.emit:
        print(json.dumps(expected, ensure_ascii=False, indent=2) + "\n", end="")
        return
    if args.write:
        OUTPUT.write_text(
            json.dumps(expected, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(f"stage7b-proof-map: WROTE {OUTPUT}")
        return
    actual = json.loads(OUTPUT.read_text(encoding="utf-8"))
    if actual != expected:
        raise SystemExit("stage7b-proof-map: committed map differs from generator")
    print(
        "stage7b-proof-map: PASS "
        f"rows={expected['row_count']} implemented={expected['implemented_count']} "
        f"pending={expected['pending_count']} accepted=false"
    )


if __name__ == "__main__":
    main()
