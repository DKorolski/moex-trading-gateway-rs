#!/usr/bin/env python3
"""Changed-path and closed execution-surface gate for Stage 6C-R1."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

import stage6c_check as checker

EXACT = {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6_replay.rs",
}
PREFIXES = (
    "fixtures/stage6c/",
    "docs/stage-6/stage6c-",
    "scripts/stage6c_",
    "scripts/make_stage6c_",
)


def main():
    root = Path.cwd().resolve()
    changed = subprocess.check_output(["git", "diff", "--name-only", checker.BASE], cwd=root, text=True).splitlines()
    for path in changed:
        if path not in EXACT and not path.startswith(PREFIXES):
            raise SystemExit(f"stage6c-closed-surface: FAIL: disallowed changed path: {path}")
    checker.validate_identity((root / checker.IDENTITY).read_text())
    checker.validate_replay((root / checker.REPLAY).read_text())
    checker.validate_compatibility(root, json.loads((root / checker.COMPATIBILITY).read_text()))
    print(f"stage6c-r1-closed-surface: PASS changed={len(changed)} redis=false finam=false dispatch=false live=false")


if __name__ == "__main__":
    main()
