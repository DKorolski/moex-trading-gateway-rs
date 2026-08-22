#!/usr/bin/env python3
"""Prove Stage 8B-S has no production, workflow or execution delta."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "50ed5382fdbe2d62ed253d65a312f951e2a267ff"
AUTHORITY = ROOT / "docs/stage-8/stage8b-spec-authority.json"


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-spec-closed-surface: FAIL {message}")


def require_false_accessor(path: Path, name: str) -> None:
    text = path.read_text(encoding="utf-8")
    require(re.search(rf"pub fn {re.escape(name)}\(&self\) -> bool \{{\s*false\s*\}}", text) is not None, f"closed accessor opened: {name}")


def main() -> None:
    changed = subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True).splitlines()
    forbidden = [path for path in changed if path.startswith(("crates/", ".github/workflows/")) or path in ("Cargo.toml", "Cargo.lock")]
    require(not forbidden, f"production/workflow delta: {forbidden}")
    closed = json.loads(AUTHORITY.read_text(encoding="utf-8")).get("closed_surfaces", {})
    require(len(closed) == 15 and all(value is True for value in closed.values()), "closed authority drift")
    for cargo in (ROOT / "crates/broker-cli/Cargo.toml", ROOT / "crates/finam-gateway/Cargo.toml"):
        require(re.search(r"(?m)^default\s*=\s*\[\s*\]\s*$", cargo.read_text(encoding="utf-8")) is not None, f"default feature opened: {cargo}")
    stage6 = ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs"
    for name in ("redis_command_consumer_attached", "finam_transport_attached", "broker_network_dispatch_attached", "runtime_live_attached", "real_orders_enabled"):
        require_false_accessor(stage6, name)
    print("stage8b-spec-closed-surface: PASS production=false workflow=false finam=false redis=false dispatch=false live=false real_orders=false stage8b_i=false stage8b_p=false stage8b_xt=false stage8b_xe=false stage12=false")


if __name__ == "__main__":
    main()
