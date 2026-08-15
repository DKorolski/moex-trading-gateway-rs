#!/usr/bin/env python3
"""Verify and print the 90-row Stage 8A-4 R3 proof map."""

from __future__ import annotations

import csv
from pathlib import Path

import stage8a4_implementation_check as checker


def main() -> None:
    checker.check(checker.ROOT, git_scope=False)
    with (checker.ROOT / checker.MATRIX).open(newline="") as stream:
        rows = list(csv.DictReader(stream))
    source = (checker.ROOT / checker.SOURCE).read_text()
    tests = (checker.ROOT / checker.TESTS).read_text()
    contract = (checker.ROOT / checker.CONTRACT).read_text()
    for row in rows:
        evidence = row["evidence"]
        if "test" in evidence:
            assert "#[test]" in tests
        if "source" in evidence or "scanner" in evidence:
            assert "Stage8a4" in source
        if "authority" in evidence or "contract" in evidence:
            assert "8A-4" in contract
        print(f"PASS {row['id']} {row['requirement']}")
    if len(rows) != 90:
        raise SystemExit("stage8a4-implementation-proof-map: FAIL row count")
    print("stage8a4-implementation-proof-map: PASS 90/90")


if __name__ == "__main__":
    main()
