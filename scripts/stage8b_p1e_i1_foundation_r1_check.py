#!/usr/bin/env python3
"""Static contract gate for the Stage 8B-P1-e I1 foundation R1 fixes."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
LIB = ROOT / "crates/runtime-durable-service/src/lib.rs"
POLICY = ROOT / "docs/stage-8/stage8b-p1e-redis-runtime-policy-v2.json"
SCRIPT = ROOT / "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua"
BOUNDARY = ROOT / "docs/stage-8/stage8b-p1e-i1-supervisor-foundation-review-boundary.md"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p1e-i1-foundation-r1-check: FAIL: {message}")


def canonical_json_sha256(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def rust_constant(source: str, name: str) -> str:
    match = re.search(rf'pub const {re.escape(name)}: &str =\s*"([^"]+)";', source)
    require(match is not None, f"missing Rust constant {name}")
    return match.group(1)


def main() -> None:
    source = SOURCE.read_text()
    lib = LIB.read_text()
    policy = json.loads(POLICY.read_text())
    script = SCRIPT.read_bytes()
    boundary = BOUNDARY.read_text()

    require(policy["schema_version"] == 2, "runtime policy schema is not v2")
    require(
        policy["domain"] == "moex.stage8b.p1e.redis-runtime-policy.v2",
        "runtime policy domain drift",
    )
    require(
        policy["policy_id"] == "imoexf-hybrid-paper-db15-runtime-v2",
        "runtime policy id drift",
    )
    require(
        policy["supersedes"]["canonical_json_sha256"]
        == "a3657dfbd10743f93478c1727e118b996c3ad9db5e71986e4378912ab3fdc6f7",
        "accepted v1 lineage drift",
    )
    hygiene = policy["stale_consumer_hygiene"]
    require(hygiene["maximum_inventory"] == 64, "inventory bound drift")
    require(hygiene["examined_per_boot"] == 16, "per-boot prefix drift")
    require(hygiene["delete_minimum_idle_ms"] == 86_400_000, "idle threshold drift")
    require(hygiene["delete_requires_pending_count"] == 0, "zero-pending rule drift")
    require(hygiene["nonzero_pending_delete_allowed"] is False, "pending deletion opened")
    require(
        hygiene["discovery_snapshot_is_authoritative_for_delete"] is False,
        "discovery snapshot became deletion authority",
    )
    atomic = hygiene["atomic_delete"]
    require(atomic["command"].startswith("EVAL <pinned-script> 1 "), "atomic EVAL drift")
    require(
        atomic["fallback_when_atomic_guarantee_unavailable"] == "retain-consumer",
        "unsafe cleanup fallback",
    )
    require(hashlib.sha256(script).hexdigest() == atomic["script_sha256"], "Lua hash drift")

    policy_hash = canonical_json_sha256(policy)
    require(
        rust_constant(source, "STAGE8B_P1E_REDIS_RUNTIME_POLICY_ID") == policy["policy_id"],
        "Rust policy id is not pinned to v2",
    )
    require(
        rust_constant(source, "STAGE8B_P1E_REDIS_RUNTIME_POLICY_SHA256") == policy_hash,
        "Rust policy hash drift",
    )
    for fragment in (
        'include_str!("../../../docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua")',
        'redis::cmd("EVAL")',
        "clean_stale_zero_pending_consumers_inner",
        "after_discovery().await?",
        "Stage8bP1eCoordinatorV1",
        "Stage8bP1eCoordinatorActionV1::CompleteUsingRetainedIntent",
        "intent.bounded_exit_class(now_utc_ms)",
        "cleanup_atomically_retains_consumer_that_gains_pending_after_discovery",
        "coordinator_preserves_error_cause_through_authenticated_boundary",
        "coordinator_preserves_external_signal_and_first_request",
        "coordinator_grace_expiry_keeps_initiating_diagnostics",
        "authenticated_boundary_without_shutdown_keeps_owner_running",
    ):
        require(fragment in source, f"source contract missing: {fragment}")
    require("stage8b_p1e_coordinate_event_v1" not in source, "stateless coordinator restored")
    require("stage8b_p1e_coordinate_event_v1" not in lib, "stateless coordinator re-exported")
    require("Stage8bP1eCoordinatorV1" in lib, "stateful coordinator is not exported")
    require(
        'redis::cmd("XGROUP")\n                    .arg("DELCONSUMER")' not in source,
        "non-atomic DELCONSUMER restored",
    )
    require("I1 foundation R1 review corrections" in boundary, "boundary amendment missing")
    require("Redis DB0/VPS" in boundary, "closed operational surface statement missing")

    print(
        "stage8b-p1e-i1-foundation-r1-check: ok "
        f"policy={policy_hash} lua={hashlib.sha256(script).hexdigest()}"
    )


if __name__ == "__main__":
    main()
