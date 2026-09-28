#!/usr/bin/env python3
"""Fail-closed source/scope checker for the Stage 8B-P1-c R1 candidate."""

from __future__ import annotations

import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE = "a85ef845f86f99bcfd45654792cc688240457d3d"
REDIS = ROOT / "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"
SEMANTIC = ROOT / "crates/runtime-durable-service/src/stage8b_p1_semantic.rs"
RECOVERY = ROOT / "crates/runtime-durable-service/src/recovery.rs"
LIVE_CORE = ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs"


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> list[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", BASE],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return sorted(set(result.stdout.splitlines()) | set(untracked.stdout.splitlines()))


def validate(redis_source: str, semantic: str, recovery: str, live_core: str) -> None:
    production = redis_source.split("#[cfg(test)]\nmod tests", maxsplit=1)[0]
    for token in (
        "reqwest::",
        "finam_gateway",
        "Stage7aPaperOutcomeProvider",
        "Stage7bRedisService<",
        "paper_outcome(",
        "runtime_live = true",
        "real_orders = true",
    ):
        require(token not in production, f"forbidden execution surface {token!r}")

    for token in (
        'redis::cmd("XREADGROUP")',
        'redis::cmd("XAUTOCLAIM")',
        'redis::cmd("XPENDING")',
        'redis::cmd("XACK")',
        'redis::cmd("XRANGE")',
        'redis::cmd("EVAL")',
        "NAMESPACE_INITIALIZATION_LUA",
        "NAMESPACE_VERIFY_LUA",
        "initialize_stage8b_p1_redis_namespace",
        "attach_stage8b_p1_redis",
        "Stage8bP1ReadySourceAcquisition",
        "PendingNotClaimable",
        "AmbiguousReadyPendingEntries",
        "COMMAND_PUBLICATION_LUA",
        "M10_PUBLICATION_LUA",
        "canonical_command_sha256",
        "canonical_envelope_sha256",
        "covering_seal_generation",
        "covering_seal_commitment_sha256",
        "marker['command_group']",
        "IdempotentExisting",
    ):
        require(token in production, f"missing P1-c invariant {token}")

    require('redis::cmd("XTRIM")' not in production, "P1-c may not trim retained M10")
    require('redis::cmd("XDEL")' not in production, "P1-c may not delete retained M10")
    initialization_lua = production.split("const NAMESPACE_INITIALIZATION_LUA", 1)[1].split('"#;', 1)[0]
    require(
        "redis.call('XGROUP', 'CREATE'" in initialization_lua
        and "'0-0', 'MKSTREAM'" in initialization_lua
        and "exact_initial_group" in initialization_lua
        and initialization_lua.count("redis.call('XLEN'") == 2,
        "one-shot initialization no longer proves an exact fresh namespace",
    )
    verify_lua = production.split("const NAMESPACE_VERIFY_LUA", 1)[1].split('"#;', 1)[0]
    require(
        "XGROUP" not in verify_lua and "exact_group_frontier" in verify_lua,
        "verify-only namespace attachment can create or guess a group",
    )
    attach_body = production.split("pub async fn attach_stage8b_p1_redis", 1)[1].split(
        "async fn open_backend", 1
    )[0]
    require(
        "verify_groups" in attach_body
        and "initialize_fresh_namespace" not in attach_body
        and "XGROUP" not in attach_body,
        "normal Redis attachment is not verify-only",
    )
    ready_acquisition = production.split("async fn acquire_ready_delivery", 1)[1].split(
        "async fn read_next_fresh", 1
    )[0]
    require(
        ready_acquisition.index("pending_entries")
        < ready_acquisition.index("match pending.ids.as_slice()")
        < ready_acquisition.index("read_next_fresh"),
        "Ready acquisition no longer checks PEL before fresh input",
    )
    for token in (
        'pending_entries("-", "+", 2)',
        "[entry]",
        "try_reclaim_exact_id",
        "PendingNotClaimable",
        "AmbiguousReadyPendingEntries",
    ):
        require(token in ready_acquisition, f"Ready PEL invariant missing: {token}")
    acknowledge = production.split("async fn acknowledge_exact", 1)[1].split(
        "async fn publish_exact_command", 1
    )[0]
    require(
        acknowledge.index("verify_groups") < acknowledge.index("pending_entries")
        and "0 if redis_id_at_least(&m10_group_frontier, delivery.redis_id())?" in acknowledge,
        "AlreadyAcknowledged no longer requires a verified continuous group frontier",
    )
    command_lua = production.split("const COMMAND_PUBLICATION_LUA", 1)[1].split('"#;', 1)[0]
    require("XACK" not in command_lua, "command publication must not XACK source M10")
    require(
        command_lua.index("XPENDING") < command_lua.index("XADD") < command_lua.index("SET"),
        "command publication ordering drifted",
    )
    require(
        "has_group(command_stream, command_group)" in command_lua,
        "Stage 7 command group is not checked atomically",
    )
    m10_lua = production.split("const M10_PUBLICATION_LUA", 1)[1].split('"#;', 1)[0]
    require(
        "has_group(stream, group)" in m10_lua and "XADD" in m10_lua,
        "M10 group-before-publication check drifted",
    )
    require(
        "pub(crate) fn into_p1c_parts" in recovery,
        "exact S1 publication material escaped crate ownership",
    )
    require(
        "pub fn into_p1c_parts" not in recovery,
        "S1 owner extraction became public",
    )
    require(
        "stage8b_p1_prepublication_material" in live_core
        and "projection.canonical_command.as_ref()?.clone()" in live_core,
        "restart no longer reconstructs the exact durable command",
    )
    require(
        "mod redis;" in semantic and "resume_stage8b_p1_prepublication_with_redis" in semantic,
        "P1-c composition is not sealed under the semantic owner module",
    )
    for test_name in (
        "p1c_real_redis_creates_groups_before_exact_m10_and_rejects_collision",
        "p1c_zero_intent_xacks_last_and_restart_is_ack_only",
        "p1c_command_response_loss_republishes_exactly_once_and_retains_m10",
        "p1c_journal_ahead_reclaims_real_pel_before_reconstructing_s1",
        "p1c_command_publication_rejects_source_xacked_before_command",
        "p1c_command_publication_rejects_missing_stage7_group_atomically",
        "p1c_tampered_publication_marker_cannot_duplicate_command",
        "p1c_s1_restart_rejects_ambiguous_multi_entry_pel",
        "p1c_ready_restart_reclaims_stale_a_before_fresh_b",
        "p1c_ready_with_unclaimable_stale_pel_never_reads_fresh",
        "p1c_ready_restart_rejects_ambiguous_pel_without_callback",
        "p1c_zero_intent_restart_rejects_deleted_m10_group_without_recreation",
        "p1c_zero_intent_rejects_externally_recreated_group_frontier",
        "p1c_restart_attach_rejects_deleted_command_group_without_recreation",
        "p1c_initializer_rejects_historical_stream_with_missing_group",
    ):
        require(test_name in redis_source, f"missing real-Redis witness {test_name}")


def validate_scope(paths: list[str]) -> None:
    allowed_rust = {
        "crates/runtime-durable-service/src/lib.rs",
        "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
        "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    }
    for path in paths:
        if path.endswith(".rs"):
            require(path in allowed_rust, f"unexpected Rust scope {path}")
        require(not path.startswith(".github/"), f"CI workflow changed in P1-c: {path}")
        require(not path.startswith("deploy/"), f"deployment changed in P1-c: {path}")
        require(not path.startswith("systemd/"), f"systemd changed in P1-c: {path}")
        require(pathlib.PurePosixPath(path).name != ".env", "secret-bearing .env changed")


def main() -> int:
    try:
        validate(
            REDIS.read_text(encoding="utf-8"),
            SEMANTIC.read_text(encoding="utf-8"),
            RECOVERY.read_text(encoding="utf-8"),
            LIVE_CORE.read_text(encoding="utf-8"),
        )
        validate_scope(changed_files())
    except (CheckFailure, OSError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1c: {error}", file=sys.stderr)
        return 1
    print("PASS stage8b-p1c-r1-source-scope")
    return 0


if __name__ == "__main__":
    sys.exit(main())
