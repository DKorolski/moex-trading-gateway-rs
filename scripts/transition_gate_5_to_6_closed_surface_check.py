#!/usr/bin/env python3
"""Ensure Transition Gate 5->6 remains documentation/tooling-only."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

BASE = "013e63bbee57c4f2d00a0587e9343ab623efba0d"


def fail(message: str) -> None:
    raise SystemExit(f"transition-gate-5-to-6-closed-surface: FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if not args.skip_git:
        changed = subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=root, text=True).splitlines()
        for path in changed:
            allowed = path in {"docs/current-status.md", "docs/reviewer-onboarding-and-roadmap.md", "docs/roadmap.md"} or path.startswith("docs/stage-6/") or path.startswith("scripts/transition_gate_5_to_6") or path == "scripts/make_transition_gate_5_to_6_handoff_archive.py"
            if not allowed:
                fail(f"non-planning path changed: {path}")
            if path.startswith("crates/") or path.startswith("source-oracles/"):
                fail(f"accepted functional authority changed: {path}")
    print("transition-gate-5-to-6-closed-surface: PASS planning_only=true")


if __name__ == "__main__":
    main()
