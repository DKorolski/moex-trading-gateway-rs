#!/usr/bin/env python3
"""Ensure the Stage 7B-a delta stays inside paper durability boundaries."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage7b_check

ALLOWED_PREFIXES = (
    "crates/strategy-runtime-core/",
    "docs/stage-7/",
    "scripts/stage7b_",
)
ALLOWED_EXACT = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "scripts/make_stage7b_a_handoff_archive.py",
}
FORBIDDEN_PREFIXES = (
    "crates/broker-finam/",
    "crates/finam-gateway/",
)


def main() -> None:
    root = Path.cwd().resolve()
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", stage7b_check.BASE], cwd=root, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
    ).splitlines()
    touched = sorted(set(changed + untracked))
    bad = [p for p in touched if p.startswith(FORBIDDEN_PREFIXES)]
    outside = [p for p in touched if p not in ALLOWED_EXACT and not p.startswith(ALLOWED_PREFIXES)]
    if bad or outside:
        raise SystemExit(f"stage7b-closed-surface: FAIL bad={bad} outside={outside}")
    production = "\n".join(
        (root / path).read_text(errors="ignore")
        for path in touched
        if path.endswith((".rs", ".toml")) and (root / path).is_file()
    )
    for token in ("Method::POST", "Method::DELETE", ".post(", ".delete(", "runtime_live_enabled = true"):
        if token in production:
            raise SystemExit(f"stage7b-closed-surface: FAIL forbidden token={token}")
    print(
        "stage7b-closed-surface: PASS "
        f"changed={len(touched)} finam_post_delete=false broker_network=false "
        "runtime_live=false real_orders=false protective=false"
    )


if __name__ == "__main__":
    main()
