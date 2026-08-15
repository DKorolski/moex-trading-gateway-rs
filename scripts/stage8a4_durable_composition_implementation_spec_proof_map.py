#!/usr/bin/env python3
"""Print the 105-row implementation-spec R2 proof map."""

import csv

import stage8a4_durable_composition_implementation_spec_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=False)
    with (checker.ROOT / checker.MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    for row in rows:
        print(f"PASS {row['id']} {row['requirement']}")
    if len(rows) != 105:
        raise SystemExit("stage8a4-durable-composition-implementation-spec-proof-map: FAIL")
    print("stage8a4-durable-composition-implementation-spec-proof-map: PASS 105/105")


if __name__ == "__main__":
    main()
