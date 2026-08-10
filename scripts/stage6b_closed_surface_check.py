#!/usr/bin/env python3
"""Changed-path and closed execution-surface gate for Stage 6B."""
from __future__ import annotations
import subprocess
from pathlib import Path
import stage6b_check as checker

EXACT={
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6_journal_backend.rs",
}
PREFIXES=("fixtures/stage6b/","docs/stage-6/stage6b-","scripts/stage6b_","scripts/make_stage6b_")

def main():
    root=Path.cwd().resolve()
    changed=subprocess.check_output(["git","diff","--name-only",checker.BASE],cwd=root,text=True).splitlines()
    for path in changed:
        if path not in EXACT and not path.startswith(PREFIXES):
            raise SystemExit(f"stage6b-closed-surface: FAIL: disallowed changed path: {path}")
    checker.validate_source((root/checker.MODULE).read_text())
    checker.validate_authority(root,__import__("json").loads((root/checker.AUTHORITY).read_text()))
    print(f"stage6b-closed-surface: PASS changed={len(changed)} runtime=false execution=false")
if __name__ == "__main__": main()
