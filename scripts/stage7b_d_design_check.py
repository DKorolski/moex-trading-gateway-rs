#!/usr/bin/env python3
"""Validate the Stage 7B-d design/entry authority without opening runtime I/O."""
from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "c57ae8d5f98bbb11df0a81f78262d3916b276d81"
STAGE7A = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
BRANCH = "stage7b-production-durability"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
MATRIX_SHA256 = "083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage7b-d-design-check: FAIL {message}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    require(git("merge-base", "HEAD", BASE) == BASE, "wrong accepted Stage 7B-c base")
    require(git("branch", "--show-current") == BRANCH, "branch drift")

    production_paths = [
        "Cargo.toml",
        "Cargo.lock",
        "crates",
        ".cargo",
        ".github/workflows",
    ]
    changed_production = git("diff", "--name-only", BASE, "--", *production_paths)
    require(not changed_production, f"design candidate changes production: {changed_production}")

    descriptor = json.loads(
        (ROOT / "docs/stage-7/stage7b-d-entry-descriptor.json").read_text()
    )
    expected = {
        "schema_version": 1,
        "stage": "7B",
        "slice": "7B-d-design",
        "status": "design_entry_candidate",
        "accepted_stage7a_predecessor": STAGE7A,
        "accepted_stage7b_c_predecessor": BASE,
        "branch": BRANCH,
        "blocking_acceptance_rows": 80,
        "semantic_proof_map_count": 80,
        "implemented_count": 42,
        "pending_count": 38,
        "design_target_first_row": "B-043",
        "design_target_last_row": "B-070",
        "implementation_slices": ["7B-d-a", "7B-d-b", "7B-d-c"],
        "stage6_single_lifecycle_authority": True,
        "mutable_recovered_extractor_allowed": False,
        "seal_before_ack_xack_required": True,
        "on_disk_seal_revalidation_required": True,
        "atomic_ack_xack_required": True,
        "atomic_dlq_xack_required": True,
        "settlement_marker_transport_only": True,
        "process_memory_ack_restart_authority": False,
        "source_claim_freshness_independent": True,
        "explicit_task_abort_clears_readiness": True,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "normative_tz_sha256": TZ_SHA256,
        "normative_matrix_sha256": MATRIX_SHA256,
    }
    require(descriptor == expected, "design descriptor drift")

    aggregate = json.loads(
        (ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text()
    )
    for key, value in {
        "schema_version": 2,
        "stage": "7B",
        "slice": "7B-d-design",
        "status": "design_entry_candidate",
        "accepted_stage7b_c_predecessor": BASE,
        "implemented_count": 42,
        "pending_count": 38,
        "stage7b_d_design_frozen": True,
        "stage7b_d_implementation_started": False,
        "seal_before_ack_xack_required": True,
        "atomic_ack_dlq_xack_required": True,
        "settlement_marker_transport_only": True,
        "process_memory_ack_restart_authority": False,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }.items():
        require(aggregate.get(key) == value, f"aggregate descriptor drift: {key}")

    c_descriptor = json.loads(
        (ROOT / "docs/stage-7/stage7b-c-entry-descriptor.json").read_text()
    )
    require(c_descriptor.get("status") == "accepted_closed", "Stage 7B-c not closed")
    require(c_descriptor.get("accepted_commit") == BASE, "Stage 7B-c acceptance ref drift")

    design = (ROOT / "docs/stage-7/stage7b-d-design.md").read_text()
    required_design = (
        "Stage 6 remains the only command/order lifecycle authority",
        "no mutable extractor",
        "seal-before-settlement",
        "Redis atomic settlement primitive",
        "process-memory ACK maps as restart authority",
        "transport-only",
        "temp sync_all",
        "root-directory sync_all",
        "committed bytes reread",
        "one opaque SettlementAuthorized capability",
        "one reviewed Lua primitive",
        "response loss",
        "Source poll and claim scan",
        "explicit abort/cancel",
        "Legacy SQLite/M3",
        "external exactly-once execution claim: false",
    )
    for token in required_design:
        require(token.lower() in design.lower(), f"design invariant absent: {token}")
    for slice_name in ("7B-d-a", "7B-d-b", "7B-d-c", "7B-e"):
        require(slice_name in design, f"implementation sequence absent: {slice_name}")

    require(
        sha256(ROOT / "docs/stage-7/STAGE7B_ACCEPTANCE_MATRIX_2026-08-12.csv")
        == MATRIX_SHA256,
        "frozen acceptance matrix drift",
    )
    require(
        sha256(ROOT / "docs/stage-7/TZ_STAGE7B_PRODUCTION_DURABILITY_COMPOSITION_2026-08-12.md")
        == TZ_SHA256,
        "normative Stage 7B TZ drift",
    )
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    print("stage7b-d-design-check: PASS rows=80 implemented=42 pending=38 production_diff=false")


if __name__ == "__main__":
    main()
