#!/usr/bin/env python3
"""Changed-path and closed execution-surface gate for Stage 6E."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage6e_check as checker

EXACT = {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/stage-6/stage6-slice-plan.md",
    "docs/stage-6/stage6e-closure-descriptor.json",
    "docs/stage-6/stage6e-live-durable-chain-closure.md",
    "scripts/stage6e_check.py",
    "scripts/stage6e_negative_harness.py",
    "scripts/stage6e_closed_surface_check.py",
    "scripts/stage6e_preseal_check.py",
    "scripts/stage6e_gate.sh",
    "scripts/make_stage6e_handoff_archive.py",
}

FORBIDDEN_PRODUCTION = (
    "redis::",
    "XREADGROUP",
    "XAUTOCLAIM",
    "reqwest",
    "broker_finam",
    "finam_gateway",
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "Stage6FileJournalBackend",
    "std::fs",
    "OpenOptions",
    "TcpStream",
    "tokio::spawn",
    "std::thread::spawn",
    "NativeStopOrder",
    "ProtectiveOrderPayload",
)


def main() -> None:
    root = Path.cwd().resolve()
    changed = subprocess.check_output(
        ["git", "diff", "--name-only", checker.BASE], cwd=root, text=True
    ).splitlines()
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
    ).splitlines()
    touched = set(changed + untracked)
    for path in sorted(touched):
        if path not in EXACT:
            raise SystemExit(f"stage6e-closed-surface: FAIL: disallowed changed path: {path}")
    production = "\n".join(
        (root / path).read_text().split("#[cfg(test)]", 1)[0]
        for path in (checker.CORE, checker.STAGE5_ORDER_POSITION)
    )
    for token in FORBIDDEN_PRODUCTION:
        if token in production:
            raise SystemExit(f"stage6e-closed-surface: FAIL: forbidden production token: {token}")
    checker.check(root)
    print(
        "stage6e-closed-surface: PASS "
        f"changed={len(touched)} redis=false finam=false network_dispatch=false "
        "runtime_live=false real_orders=false protective=false"
    )


if __name__ == "__main__":
    main()
