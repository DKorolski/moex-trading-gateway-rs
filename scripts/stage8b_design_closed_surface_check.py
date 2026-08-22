#!/usr/bin/env python3
"""Prove Stage 8B-D R2 has no production, workflow or execution delta."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "7bc9fdab190e011111b15ebdf2f35ff2263a8e34"
AUTHORITY = ROOT / "docs/stage-8/stage8b-design-authority.json"


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-design-closed-surface: FAIL {message}")


def defaults_are_empty(path: Path) -> bool:
    text = path.read_text(encoding="utf-8")
    return re.search(r"(?m)^default\s*=\s*\[\s*\]\s*$", text) is not None


def require_false_accessor(path: Path, name: str) -> None:
    text = path.read_text(encoding="utf-8")
    require(
        re.search(rf"pub fn {re.escape(name)}\(&self\) -> bool \{{\s*false\s*\}}", text) is not None,
        f"closed accessor opened: {name}",
    )


def main() -> None:
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
    ).splitlines()
    forbidden = [
        path
        for path in changed
        if path.startswith(("crates/", ".github/workflows/"))
        or path in ("Cargo.toml", "Cargo.lock")
    ]
    require(not forbidden, f"production/workflow delta: {forbidden}")

    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    closed = authority.get("closed", {})
    require(len(closed) == 15 and all(value is True for value in closed.values()), "closed authority drift")
    require(defaults_are_empty(ROOT / "crates/broker-cli/Cargo.toml"), "broker-cli default feature opened")
    require(defaults_are_empty(ROOT / "crates/finam-gateway/Cargo.toml"), "finam-gateway default feature opened")

    stage6 = ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs"
    for name in (
        "redis_command_consumer_attached",
        "finam_transport_attached",
        "broker_network_dispatch_attached",
        "runtime_live_attached",
        "real_orders_enabled",
    ):
        require_false_accessor(stage6, name)

    print(
        "stage8b-design-closed-surface: PASS production=false workflow=false "
        "finam=false redis=false dispatch=false live=false real_orders=false stage8b_s=false stage12=false"
    )


if __name__ == "__main__":
    main()
