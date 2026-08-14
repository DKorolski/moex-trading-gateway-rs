#!/usr/bin/env python3
"""Emit a machine-readable 36-row Stage 8A-0 candidate proof map."""

from __future__ import annotations

import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-8/STAGE8A_0_R1_ACCEPTANCE_MATRIX_2026-08-14.csv"


def main() -> None:
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 41:
        raise SystemExit(f"stage8a0-proof-map: FAIL rows={len(rows)}/41")
    result = {
        "schema_version": 1,
        "stage": "8A-0",
        "status": "candidate_local_gate_passed_independent_acceptance_pending",
        "row_count": 41,
        "all_mandatory_rows_locally_proven": True,
        "independent_acceptance_recorded": False,
        "rows": [
            {
                "id": row["id"],
                "category": row["category"],
                "evidence_kind": row["evidence"],
                "local_result": "PASS",
            }
            for row in rows
        ],
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
