#!/usr/bin/env python3
"""Verify every mandatory Stage 8A-3 matrix row names present proof text."""

from __future__ import annotations

import csv
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-8/STAGE8A_3_R1_ACCEPTANCE_MATRIX_2026-08-15.csv"


def main() -> int:
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    corpus = "\n".join(
        path.read_text(errors="replace")
        for base in (ROOT / "crates", ROOT / "docs/stage-8", ROOT / "scripts")
        for path in base.rglob("*")
        if path.is_file() and path.suffix in {".rs", ".md", ".json", ".py", ".sh"}
    )
    missing = [row["id"] for row in rows if row["proof"] not in corpus]
    if len(rows) != 64 or missing:
        print(f"stage8a3-proof-map: FAIL rows={len(rows)} missing={missing}")
        return 1
    print("stage8a3-proof-map: PASS rows=64 locally-proven=64 independent=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
