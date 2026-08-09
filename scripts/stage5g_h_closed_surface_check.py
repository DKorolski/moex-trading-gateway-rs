#!/usr/bin/env python3
"""Scoped closed-surface and changed-path check for Stage 5G-h."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

BASE = "ee0505dfee71f043f3185c16cbdd563e3b36a6c1"
ALLOWED_PREFIXES = ("docs/stage-5/", "scripts/stage5g_h", "scripts/make_stage5g_h")
ALLOWED_EXACT = {
    "crates/strategy-runtime-core/src/bin/stage5g_g_lifecycle_artifact.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_lifecycle_freeze.rs",
    "docs/current-status.md",
}
FORBIDDEN_RUST = (
    "reqwest::", "redis::", "Method::POST", "Method::DELETE", ".post(", ".delete(",
    "XREADGROUP", "XGROUP", "runtime_live_enabled = true", "real_order_endpoint_enabled = true",
)


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-h-closed-surface: FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--skip-git", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if not args.skip_git:
        output = subprocess.check_output(["git", "diff", "--name-only", BASE], cwd=root, text=True)
        for path in output.splitlines():
            if path not in ALLOWED_EXACT and not path.startswith(ALLOWED_PREFIXES):
                fail(f"path outside Stage H allowlist: {path}")
    source = (root / "crates/strategy-runtime-core/src/stage5g_lifecycle_freeze.rs").read_text()
    for token in FORBIDDEN_RUST:
        if token in source:
            fail(f"forbidden runtime surface: {token}")
    print("stage5g-h-closed-surface: PASS")


if __name__ == "__main__":
    main()
