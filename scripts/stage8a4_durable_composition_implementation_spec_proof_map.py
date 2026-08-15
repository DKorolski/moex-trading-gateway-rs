#!/usr/bin/env python3
"""Print the 84-row implementation-spec proof map."""

import csv

import stage8a4_durable_composition_implementation_spec_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=False)
    with (checker.ROOT / checker.MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    for row in rows:
        print(f"PASS {row['id']} {row['requirement']}")
    if len(rows) != 84:
        raise SystemExit("stage8a4-durable-composition-implementation-spec-proof-map: FAIL")
    print("stage8a4-durable-composition-implementation-spec-proof-map: PASS 84/84")


if __name__ == "__main__":
    main()
