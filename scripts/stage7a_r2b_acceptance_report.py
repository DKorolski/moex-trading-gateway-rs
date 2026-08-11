#!/usr/bin/env python3
"""Evaluate all frozen Stage 7A rows from the reviewable R2b proof map."""
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path

MATRIX = Path("docs/stage-7/STAGE7A_ACCEPTANCE_MATRIX_2026-08-11.csv")
PROOF_MAP = Path("docs/stage-7/stage7a-r2b-acceptance-proof-map.json")
PINNED_PROOF_TYPES = {
    "A-025": "strict_behavioral_tests",
    "A-032": "task_supervision_test",
    "A-033": "source_outage_test",
    "A-040": "closed_surface_na",
    "A-041": "closed_surface_na",
    "A-045": "fault_matrix_gate",
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    ids = [row["ID"] for row in rows]
    expected_ids = [f"A-{index:03d}" for index in range(1, 53)]
    if ids != expected_ids or not all(row["Blocking"] == "YES" for row in rows):
        raise SystemExit("stage7a-r2b-acceptance: FAIL: frozen matrix identity drift")

    proof_document = json.loads(PROOF_MAP.read_text())
    proofs = proof_document.get("proofs", [])
    proof_ids = [proof.get("id") for proof in proofs]
    if proof_ids != expected_ids or len(set(proof_ids)) != 52:
        raise SystemExit("stage7a-r2b-acceptance: FAIL: proof map incomplete or unordered")
    proof_by_id = {proof["id"]: proof for proof in proofs}

    evaluated = []
    for row in rows:
        proof = proof_by_id[row["ID"]]
        required_fields = ("requirement", "proof_type", "rationale", "witnesses")
        mapping_complete = all(proof.get(field) for field in required_fields)
        if row["ID"] in PINNED_PROOF_TYPES:
            mapping_complete &= proof.get("proof_type") == PINNED_PROOF_TYPES[row["ID"]]
        references = []
        passed = mapping_complete and isinstance(proof.get("witnesses"), list)
        for witness in proof.get("witnesses", []):
            filename = witness.get("artifact", "")
            tokens = witness.get("tokens", [])
            safe_name = bool(filename) and Path(filename).name == filename
            path = args.artifact_dir / filename if safe_name else Path("/__invalid__")
            text = path.read_text(errors="replace") if path.is_file() else ""
            matched = [token for token in tokens if isinstance(token, str) and token in text]
            witness_passed = safe_name and bool(tokens) and len(matched) == len(tokens)
            passed &= witness_passed
            references.append({
                "artifact": filename,
                "required_tokens": tokens,
                "matched_tokens": matched,
                "status": "PASS" if witness_passed else "FAIL",
            })
        evaluated.append({
            "id": row["ID"],
            "area": row["Area"],
            "frozen_scenario": row["Scenario"],
            "frozen_expected_result": row["Expected Result"],
            "blocking": True,
            "proof_requirement": proof.get("requirement"),
            "proof_type": proof.get("proof_type"),
            "why_witness_proves_row": proof.get("rationale"),
            "mapping_complete": mapping_complete,
            "status": "PASS" if passed else "FAIL",
            "witnesses": references,
        })

    pass_count = sum(item["status"] == "PASS" for item in evaluated)
    report = {
        "schema_version": 2,
        "stage": "7A-R2b",
        "proof_map": str(PROOF_MAP),
        "acceptance_row_count": len(rows),
        "acceptance_evaluated_count": len(evaluated),
        "acceptance_pass_count": pass_count,
        "all_blocking_rows_passed": pass_count == len(rows),
        "rows": evaluated,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if pass_count != 52:
        failed = [item["id"] for item in evaluated if item["status"] != "PASS"]
        raise SystemExit(f"stage7a-r2b-acceptance: FAIL rows={failed}")
    print("stage7a-r2b-acceptance: PASS evaluated=52 passed=52 semantic_map=complete")


if __name__ == "__main__":
    main()
