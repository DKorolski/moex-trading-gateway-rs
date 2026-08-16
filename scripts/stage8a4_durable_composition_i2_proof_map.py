#!/usr/bin/env python3
"""Verify every I2 acceptance row has evidence."""

import csv
import stage8a4_durable_composition_i2_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=True)
    with (checker.ROOT / checker.MATRIX).open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    if len(rows) != 42 or any(not row["requirement"].strip() or not row["evidence"].strip() for row in rows):
        raise SystemExit("stage8a4-durable-composition-i2-proof-map: FAIL")
    print("stage8a4-durable-composition-i2-proof-map: PASS 42/42")


if __name__ == "__main__":
    main()
