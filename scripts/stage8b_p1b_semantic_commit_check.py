#!/usr/bin/env python3
"""Fail-closed source/scope checker for the Stage 8B-P1-b review candidate."""

from __future__ import annotations

import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "073ae9f38acd06b7a5febdbfc1e75a7b460adf03"
SEMANTIC = ROOT / "crates/runtime-durable-service/src/stage8b_p1_semantic.rs"
RECOVERY = ROOT / "crates/runtime-durable-service/src/recovery.rs"
CORE = ROOT / "crates/strategy-runtime-core/src/stage5g_p1_semantic.rs"
LIVE_CORE = ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"FAIL stage8b-p1b-semantic-commit: {message}")


def git_changed_files() -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", BASE],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return [line for line in result.stdout.splitlines() if line]


def main() -> int:
    semantic = SEMANTIC.read_text(encoding="utf-8")
    recovery = RECOVERY.read_text(encoding="utf-8")
    core = CORE.read_text(encoding="utf-8")
    live_core = LIVE_CORE.read_text(encoding="utf-8")
    production_semantic = semantic.split("#[cfg(test)]\nmod tests", maxsplit=1)[0]

    for token in (
        "redis::",
        "reqwest::",
        "std::net::",
        "tokio::net::",
        "Stage7bRedisService",
        "finam_gateway",
    ):
        require(token not in production_semantic, f"forbidden I/O surface {token!r}")

    for token in (
        "Stage8bP1SemanticCompositionOwner",
        "Stage8bP1PendingM10Delivery",
        "acknowledge_after_durable_commit",
        "resume_stage8b_p1_journal_ahead_with_local_m10",
        "STAGE8B_P1_LOCAL_M10_MIN_RETENTION",
    ):
        require(token in semantic, f"missing semantic invariant {token}")

    for token in (
        'stage8b_p1_test_crash_barrier("before-request-accepted-append")',
        'stage8b_p1_test_crash_barrier("after-request-accepted-fsync")',
        'stage8b_p1_test_crash_barrier("before-s1-temp-fsync")',
        'stage8b_p1_test_crash_barrier("after-s1-temp-fsync-before-rename")',
        'stage8b_p1_test_crash_barrier("after-s1-rename-before-directory-fsync")',
        'stage8b_p1_test_crash_barrier("after-s1-directory-fsync-before-reread")',
        'stage8b_p1_test_crash_barrier("after-s1-reread-before-command-xadd")',
        "records.len() != prefix_len.saturating_add(1)",
    ):
        require(
            token in recovery or token in core or token in live_core,
            f"missing crash/recovery invariant {token}",
        )

    require("p1_multi_intent_boundary(&semantic_batch_id_sha256" in core, "multi-intent gate absent")
    require("request_ids.len() > 1" in core, "multi-intent threshold drift")

    forbidden_changed_prefixes = (
        ".github/",
        "deploy/",
        "systemd/",
        "crates/finam-gateway/",
    )
    forbidden_changed_names = {
        "docker-compose.yml",
        "docker-compose.yaml",
        ".env",
    }
    for changed in git_changed_files():
        require(
            not changed.startswith(forbidden_changed_prefixes)
            and pathlib.PurePosixPath(changed).name not in forbidden_changed_names,
            f"forbidden operational surface changed: {changed}",
        )

    print("PASS stage8b-p1b-semantic-commit-source-scope")
    return 0


if __name__ == "__main__":
    sys.exit(main())
