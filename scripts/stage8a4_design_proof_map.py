#!/usr/bin/env python3
"""Verify all mandatory Stage 8A-4 design rows have local proof locators."""

from __future__ import annotations

import csv
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/stage-8/STAGE8A_4_DESIGN_ACCEPTANCE_MATRIX_2026-08-15.csv"


def main() -> int:
    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    files = [
        path
        for base in (ROOT / "crates", ROOT / "docs/stage-8", ROOT / "scripts")
        for path in base.rglob("*")
        if path.is_file() and path != MATRIX
    ]
    corpus = "\n".join(path.read_text(errors="replace") for path in files)
    names = {path.name for path in files}
    missing = []
    for row in rows:
        proof = row["proof"]
        if proof.endswith((".json", ".md", ".py", ".sh", ".rs")):
            present = proof in names
        else:
            present = proof in corpus
        if not present:
            missing.append(row["id"])
    if len(rows) != 72 or missing:
        print(f"stage8a4-design-proof-map: FAIL rows={len(rows)} missing={missing}")
        return 1
    print("stage8a4-design-proof-map: PASS rows=72 locally-proven=72 independent=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
