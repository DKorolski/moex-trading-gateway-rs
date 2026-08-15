#!/usr/bin/env python3
"""Verify that Stage 8A-1 adds only the approved no-send Rust surface."""

from __future__ import annotations

import subprocess
import sys

import stage8a1_check as checker


def main() -> None:
    try:
        checker.check()
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", checker.BASE], cwd=checker.ROOT, text=True
            ).splitlines()
        )
        changed.update(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=checker.ROOT,
                text=True,
            ).splitlines()
        )
        rust = {path for path in changed if path.endswith(".rs")}
        allowed = {str(path) for path in checker.PINNED_RUST_SHA256}
        checker.require(rust <= allowed, f"unexpected Rust delta: {sorted(rust - allowed)}")
        checker.require(
            not any(path.startswith("crates/broker-finam/") for path in changed),
            "FINAM adapter/builder changed",
        )
        cargo_changes = {path for path in changed if path == "Cargo.lock" or path.endswith("Cargo.toml")}
        checker.require(
            cargo_changes <= {"Cargo.lock", str(checker.FINAM_CARGO)},
            f"unexpected Cargo surface: {sorted(cargo_changes)}",
        )
        checker.require(
            not any(path.startswith(".github/") for path in changed),
            "CI surface changed",
        )
    except (checker.CheckFailure, subprocess.CalledProcessError) as error:
        print(f"stage8a1-closed-surface: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "stage8a1-r1-closed-surface: PASS rust=exact-sha-pinned serializer=false "
        "http=false redis=false dispatch=false runtime_live=false"
    )


if __name__ == "__main__":
    main()
