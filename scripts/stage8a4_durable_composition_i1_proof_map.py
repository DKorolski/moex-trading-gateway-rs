#!/usr/bin/env python3
"""Check that every I1 acceptance row has non-empty evidence."""

import csv
from pathlib import Path

import stage8a4_durable_composition_i1_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=True)
    with (checker.ROOT / checker.MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 40 or any(not row["requirement"].strip() or not row["evidence"].strip() for row in rows):
        raise SystemExit("stage8a4-durable-composition-i1-proof-map: FAIL")
    print("stage8a4-durable-composition-i1-proof-map: PASS 40/40")


if __name__ == "__main__":
    main()
