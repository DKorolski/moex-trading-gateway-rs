#!/usr/bin/env python3
"""Fail closed if Stage 8B-I expands beyond its reviewed no-send delta."""

from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = "d1581962666aa82b993854d0642e67bd66624032"
AUTHORITY = ROOT / "docs/stage-8/stage8b-i-authority.json"
ALLOWED_PRODUCTION = {
    "Cargo.lock",
    "crates/broker-cli/src/lib.rs",
    "crates/broker-cli/tests/stage8b_i_no_send_facade.rs",
    "crates/finam-gateway/Cargo.toml",
    "crates/finam-gateway/src/lib.rs",
    "crates/finam-gateway/src/stage8b_no_send.rs",
}


def require(value: bool, message: str) -> None:
    if not value:
        raise SystemExit(f"stage8b-i-closed-surface: FAIL {message}")


def changed_paths() -> set[str]:
    changed = set(
        subprocess.check_output(
            ["git", "diff", "--name-only", BASE, "--"], cwd=ROOT, text=True
        ).splitlines()
    )
    changed.update(
        subprocess.check_output(
            ["git", "ls-files", "--others", "--exclude-standard"],
            cwd=ROOT,
            text=True,
        ).splitlines()
    )
    return changed


def require_false_accessor(path: Path, name: str) -> None:
    text = path.read_text(encoding="utf-8")
    require(
        re.search(
            rf"pub fn {re.escape(name)}\(&self\) -> bool \{{\s*false\s*\}}", text
        )
        is not None,
        f"closed accessor opened: {name}",
    )


def main() -> None:
    changed = changed_paths()
    production = {
        path
        for path in changed
        if path in {"Cargo.toml", "Cargo.lock"} or path.startswith("crates/")
    }
    require(
        production == ALLOWED_PRODUCTION,
        f"production delta drift: expected={sorted(ALLOWED_PRODUCTION)} actual={sorted(production)}",
    )
    workflows = sorted(path for path in changed if path.startswith(".github/workflows/"))
    require(not workflows, f"workflow delta forbidden: {workflows}")

    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    for key in (
        "real_adapter_present",
        "finam_post_delete_enabled",
        "network_send_enabled",
        "redis_execution_enabled",
        "ack_readiness_publication_enabled",
        "broker_dispatch_enabled",
        "runtime_live_enabled",
        "real_orders_enabled",
        "stage8b_it_enabled",
        "stage8b_p_enabled",
        "stage8b_xe_enabled",
        "stage12_enabled",
    ):
        require(authority.get(key) is False, f"closed authority opened: {key}")

    module = (ROOT / "crates/finam-gateway/src/stage8b_no_send.rs").read_text(
        encoding="utf-8"
    )
    for forbidden in (
        "reqwest::",
        "redis::",
        ".post(",
        ".delete(",
        ".request(",
        ".send(",
        "xadd(",
        "xack(",
        "TcpStream",
    ):
        require(forbidden not in module, f"network/effect token present: {forbidden}")

    for cargo in (
        ROOT / "crates/broker-cli/Cargo.toml",
        ROOT / "crates/finam-gateway/Cargo.toml",
    ):
        require(
            re.search(
                r"(?m)^default\s*=\s*\[\s*\]\s*$", cargo.read_text(encoding="utf-8")
            )
            is not None,
            f"default feature opened: {cargo}",
        )

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
        "stage8b-i-closed-surface: PASS "
        "production_delta=exact workflow=false adapter=false finam=false redis=false "
        "dispatch=false live=false real_orders=false stage8b_it=false stage8b_p=false "
        "stage8b_xe=false stage12=false"
    )


if __name__ == "__main__":
    main()
