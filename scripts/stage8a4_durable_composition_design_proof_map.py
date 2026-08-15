#!/usr/bin/env python3
"""Print the Stage 8A-4 durable-composition Design R2 proof map."""

import csv

import stage8a4_durable_composition_design_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=False)
    with (checker.ROOT / checker.MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    for row in rows:
        print(f"PASS {row['id']} {row['requirement']}")
    if len(rows) != 76:
        raise SystemExit("stage8a4-durable-composition-design-proof-map: FAIL")
    print("stage8a4-durable-composition-design-proof-map: PASS 76/76")


if __name__ == "__main__":
    main()
