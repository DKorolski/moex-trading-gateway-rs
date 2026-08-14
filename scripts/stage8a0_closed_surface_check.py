#!/usr/bin/env python3
"""Stage 8A-0-specific closure authority for a docs/checker-only slice."""

from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "630bef3fb9aa07bbc377498fc052f085155a043c"
ALLOWED_PREFIXES = ("docs/stage-8/", "scripts/stage8a0_", "scripts/make_stage8a0_")
ALLOWED_EXACT = {"docs/current-status.md", "docs/roadmap.md"}
FORBIDDEN_CHANGED_PREFIXES = ("crates/", ".github/", ".cargo/")
FORBIDDEN_CHANGED_EXACT = {"Cargo.toml", "Cargo.lock"}


class ClosedSurfaceFailure(RuntimeError):
    pass


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def check_git_scope() -> list[str]:
    if git("merge-base", "HEAD", BASE) != BASE:
        raise ClosedSurfaceFailure("accepted Gate R3 is not an ancestor")
    changed = [line for line in git("diff", "--name-only", BASE).splitlines() if line]
    untracked = [line for line in git("ls-files", "--others", "--exclude-standard").splitlines() if line]
    paths = sorted(set(changed + untracked))
    for path in paths:
        if path in FORBIDDEN_CHANGED_EXACT or path.startswith(FORBIDDEN_CHANGED_PREFIXES):
            raise ClosedSurfaceFailure(f"production/Cargo/workflow delta: {path}")
        if path not in ALLOWED_EXACT and not path.startswith(ALLOWED_PREFIXES):
            raise ClosedSurfaceFailure(f"path outside Stage 8A-0 scope: {path}")
    return paths


def main() -> None:
    paths = check_git_scope()
    print(
        "stage8a0-closed-surface: PASS "
        f"changed_paths={len(paths)} production_rust=false cargo=false github=false "
        "finam_post_delete=false broker_dispatch=false runtime_live=false real_orders=false"
    )


if __name__ == "__main__":
    main()
