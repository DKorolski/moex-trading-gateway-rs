#!/usr/bin/env python3
"""Stage 8 specific closed-surface scanner; independent of legacy Stage 5 pins."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

PREDECESSOR = "4a11688c941ee240e377b384042c4bca837b040f"
FORBIDDEN = {
    "reqwest transport": re.compile(r"\breqwest\b"),
    "HTTP post": re.compile(r"\.post\s*\("),
    "HTTP delete": re.compile(r"\.delete\s*\("),
    "POST method": re.compile(r"Method\s*::\s*POST"),
    "DELETE method": re.compile(r"Method\s*::\s*DELETE"),
    "Redis XADD": re.compile(r"\bXADD\b|\.xadd\s*\(", re.I),
    "Redis XACK": re.compile(r"\bXACK\b|\.xack\s*\(", re.I),
    "Redis command consumer": re.compile(r"redis_live_consumer_enabled\s*[:=]\s*true", re.I),
    "broker dispatch": re.compile(r"broker_dispatch_enabled\s*[:=]\s*true", re.I),
    "runtime live": re.compile(r"runtime_live_enabled\s*[:=]\s*true", re.I),
}


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    files = sorted((root / "crates/finam-gateway/src").glob("stage8a*.rs"))
    files += sorted((root / "crates/finam-gateway/src").glob("stage8a*/**/*.rs"))
    require(files, "no Stage 8 production source found")
    for path in files:
        text = path.read_text(encoding="utf-8")
        for label, pattern in FORBIDDEN.items():
            require(not pattern.search(text), f"{label} found in {path.relative_to(root)}")

    authority_path = root / "docs/stage-8/stage8a5-aggregate-acceptance-authority.json"
    authority = json.loads(authority_path.read_text(encoding="utf-8"))
    for key in (
        "ack_publication_enabled", "readiness_publication_enabled", "redis_xadd_xack_enabled",
        "redis_live_consumer_enabled", "finam_post_delete_enabled", "broker_dispatch_enabled",
        "retry_resend_rearm_enabled", "runtime_live_enabled", "real_orders_enabled",
    ):
        require(authority.get(key) is False, f"authority opened closed surface: {key}")

    if not args.no_git:
        changed = subprocess.check_output(
            ["git", "diff", "--name-only", PREDECESSOR, "--", "crates", "Cargo.toml", "Cargo.lock", ".github"],
            cwd=root,
            text=True,
        ).strip()
        require(not changed, f"aggregate changed production/Cargo/workflow paths: {changed}")
    print(f"stage8a5-forbidden-surface-check: PASS files={len(files)} redis=false finam=false live=false")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"stage8a5-forbidden-surface-check: FAIL {error}") from error
