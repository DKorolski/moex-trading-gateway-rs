#!/usr/bin/env python3
"""Prove Stage 8B Design R1 has no production or execution delta."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "0ce76a334f12bf7b13e682ca976c9a4cde6be137"
AUTHORITY = ROOT / "docs/stage-8/stage8b-design-authority.json"


def main() -> None:
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    forbidden = [
        path for path in changed
        if path.startswith(("crates/", ".github/workflows/"))
        or path in ("Cargo.toml", "Cargo.lock")
    ]
    if forbidden:
        raise SystemExit(f"stage8b-design-closed-surface: FAIL production delta: {forbidden}")
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    closed = authority.get("closed", {})
    if len(closed) != 12 or not all(value is True for value in closed.values()):
        raise SystemExit("stage8b-design-closed-surface: FAIL closed authority drift")
    print("stage8b-design-closed-surface: PASS production=false finam=false redis=false dispatch=false live=false")


if __name__ == "__main__":
    main()
