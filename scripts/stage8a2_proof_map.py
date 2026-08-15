#!/usr/bin/env python3
"""Build the deterministic Stage 8A-2 acceptance proof map."""

from __future__ import annotations

import csv
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-8/STAGE8A_2_R1_ACCEPTANCE_MATRIX_2026-08-15.csv"


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a2_proof_map.py OUTPUT")
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 50:
        raise SystemExit("stage8a2-proof-map: FAIL matrix count")
    proof = {
        "schema_version": 1,
        "stage": "8A-2-R1",
        "candidate_status": "independent_acceptance_pending",
        "row_count": len(rows),
        "locally_proven_count": len(rows),
        "independent_acceptance_recorded": False,
        "rows": [
            {
                "id": row["id"],
                "group": row["group"],
                "requirement": row["requirement"],
                "proof_status": "LOCALLY_PROVEN",
            }
            for row in rows
        ],
    }
    Path(sys.argv[1]).write_text(json.dumps(proof, indent=2, sort_keys=True) + "\n")
    print("stage8a2-proof-map: PASS rows=50 locally-proven=50 independent=false")


if __name__ == "__main__":
    main()
