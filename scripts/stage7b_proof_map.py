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
    "B-001": ("git_gate", "scripts/stage7b_check.py::check_lineage"),
    "B-002": ("governance_gate", "scripts/stage7b_check.py::check_governance"),
    "B-003": ("static_gate", "scripts/stage7b_closed_surface_check.py"),
    "B-008": ("unit", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-010": ("unit", "stage6d_first_boot_requires_explicit_create_authority"),
    "B-011": ("fs_integration", "stage7b_open_existing_never_creates_missing_journal"),
    "B-012": ("fs_integration", "stage7b_create_new_and_open_existing_are_explicit_and_disjoint"),
    "B-014": ("fs_integration", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-015": ("unit", "stage6b_torn_write_failpoints_leave_reopen_fail_closed"),
    "B-016": ("unit", "stage6b_external_file_length_mutation_blocks_append"),
    "B-017": ("unit", "stage6b_sync_failure_returns_durability_uncertain_without_receipt"),
    "B-018": ("unit", "stage6b_file_receipt_is_returned_after_sync_path"),
    "B-019": ("unit", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-020": ("fs_integration", "stage7b_owned_backend_preserves_memory_file_and_reopen_parity"),
    "B-071": ("governance_gate", "docs/stage-7/stage7b-entry-descriptor.json"),
    "B-075": ("inherited_gate", "scripts/stage7b_a_gate.sh"),
    "B-076": ("negative_harness", "scripts/stage7b_negative_harness.py cases=10"),
    "B-079": ("closed_surface", "scripts/stage7b_closed_surface_check.py"),
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
                    "Stage 7B-a exact foundation witness"
                    if implemented
                    else "Frozen requirement retained pending its designated Stage 7B slice"
                ),
                "artifact": (
                    "strategy-runtime-core + Stage 7B-a gate"
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
        "slice": "7B-a",
        "accepted_predecessor": "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64",
        "row_count": len(proofs),
        "implemented_count": sum(p["status"] == "implemented" for p in proofs),
        "pending_count": sum(p["status"] == "pending" for p in proofs),
        "stage7b_accepted": False,
        "proofs": proofs,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit", action="store_true")
    args = parser.parse_args()
    expected = build()
    if args.emit:
        print(json.dumps(expected, ensure_ascii=False, indent=2) + "\n", end="")
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
