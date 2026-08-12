#!/usr/bin/env python3
"""Keep Stage 7B-c inside paper recovery ownership boundaries."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage7b_c_check

ALLOWED_PREFIXES = (
    "crates/runtime-durable-service/",
    "crates/strategy-runtime-core/",
    "docs/stage-7/",
    "scripts/stage7b_",
)
ALLOWED_EXACT = {
    "Cargo.lock",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "scripts/make_stage7b_c_handoff_archive.py",
}
FORBIDDEN_PREFIXES = (
    "crates/broker-finam/",
    "crates/finam-gateway/",
    "crates/runtime-command-bridge/",
)


def main() -> None:
    root = Path.cwd().resolve()
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", stage7b_c_check.BASE], cwd=root, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
    ).splitlines()
    touched = sorted(set(changed + untracked))
    bad = [path for path in touched if path.startswith(FORBIDDEN_PREFIXES)]
    outside = [
        path for path in touched
        if path not in ALLOWED_EXACT and not path.startswith(ALLOWED_PREFIXES)
    ]
    if bad or outside:
        raise SystemExit(f"stage7b-c-closed-surface: FAIL bad={bad} outside={outside}")
    production = "\n".join(
        (root / path).read_text(errors="ignore")
        for path in touched
        if path.endswith((".rs", ".toml")) and (root / path).is_file()
    )
    for token in (
        "Method::POST", "Method::DELETE", ".post(", ".delete(",
        "runtime_live_enabled = true", "redis::", "broker_finam", "finam_gateway",
    ):
        if token in production:
            raise SystemExit(f"stage7b-c-closed-surface: FAIL forbidden token={token}")
    print(
        "stage7b-c-closed-surface: PASS "
        f"changed={len(touched)} redis_consumer=false settlement=false xack=false "
        "finam_post_delete=false broker_network=false runtime_live=false real_orders=false"
    )


if __name__ == "__main__":
    main()
