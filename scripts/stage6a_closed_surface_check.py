#!/usr/bin/env python3
"""Changed-path and closed execution-surface check for Stage 6A."""
from __future__ import annotations
import subprocess
from pathlib import Path
import stage6a_check as checker

EXACT = {"crates/strategy-runtime-core/src/lib.rs", "crates/strategy-runtime-core/src/stage6_durable_identity.rs"}
PREFIXES = ("fixtures/stage6a/", "docs/stage-6/stage6a-", "scripts/stage6a_", "scripts/make_stage6a_")

def main() -> None:
    root = Path.cwd().resolve()
    changed = subprocess.check_output(["git", "diff", "--name-only", checker.BASE], cwd=root, text=True).splitlines()
    for path in changed:
        if path not in EXACT and not path.startswith(PREFIXES):
            raise SystemExit(f"stage6a-closed-surface: FAIL: disallowed changed path: {path}")
    checker.validate_source((root / checker.MODULE).read_text())
    print(f"stage6a-closed-surface: PASS changed={len(changed)} execution=false")

if __name__ == "__main__": main()
