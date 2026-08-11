#!/usr/bin/env python3
"""Changed-path and closed execution-surface gate for Stage 6D."""
from __future__ import annotations

import subprocess
from pathlib import Path

import stage6d_check as checker

EXACT = {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs",
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs",
    "crates/strategy-runtime-core/src/stage5g_protective_completion.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/stage-6/stage6d-live-core-paper-mvp.md",
    "docs/stage-6/stage6d-integration-descriptor.json",
    "scripts/stage6d_check.py",
    "scripts/stage6d_negative_harness.py",
    "scripts/stage6d_closed_surface_check.py",
    "scripts/stage6d_preseal_check.py",
    "scripts/stage6d_gate.sh",
    "scripts/make_stage6d_handoff_archive.py",
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
    for path in sorted(set(changed + untracked)):
        if path not in EXACT:
            raise SystemExit(f"stage6d-closed-surface: FAIL: disallowed changed path: {path}")
    production = (root / checker.CORE).read_text().split("#[cfg(test)]", 1)[0]
    for token in FORBIDDEN_PRODUCTION:
        if token in production:
            raise SystemExit(f"stage6d-closed-surface: FAIL: forbidden production token: {token}")
    checker.check(root)
    print(
        "stage6d-closed-surface: PASS "
        f"changed={len(set(changed + untracked))} redis=false finam=false "
        "network_dispatch=false runtime_live=false real_orders=false protective=false"
    )


if __name__ == "__main__":
    main()
