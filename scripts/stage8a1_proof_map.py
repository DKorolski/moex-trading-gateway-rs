#!/usr/bin/env python3
"""Emit the Stage 8A-1 local acceptance proof map."""

from __future__ import annotations

import csv
import json

import stage8a1_check as checker


def main() -> None:
    checker.check()
    with (checker.ROOT / checker.MATRIX).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    print(
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8A-1-R1",
                "candidate_status": "independent_acceptance_pending",
                "accepted_stage8a0_ref": checker.BASE,
                "row_count": len(rows),
                "all_mandatory_rows_locally_proven": len(rows) == 58,
                "independent_acceptance_recorded": False,
                "rows": rows,
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
