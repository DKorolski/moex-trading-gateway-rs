#!/usr/bin/env python3
"""Validate the Stage 7B-d design/entry authority without opening runtime I/O."""
from __future__ import annotations

import hashlib
import csv
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = "c57ae8d5f98bbb11df0a81f78262d3916b276d81"
STAGE7A = "2b6d6e90f2350b77fc1d79aa7381e6d9c6566c64"
BRANCH = "stage7b-production-durability"
TZ_SHA256 = "200e42acef2bb30cf24e3d2a5bc38df99ed853d70d6310653f315e76d1f4c1e0"
MATRIX_SHA256 = "083cc6e1e0925f11efa4bc093fd7c2d3d4cbeb05fd275f68ed71be3bdac1931d"
R1_CONTRACT_SHA256 = "fea4a7ce11d6eba22aa7700ab558b2a84122aa65316adf1a9cd3ee9ab4a5c65a"
R1_MATRIX_SHA256 = "2611667dc030ee6bd97943e92a2ef15d59799236f7ca1af7988cf798cdb8dcfb"


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
        "slice": "7B-d-design-R1",
        "status": "design_authority_clarification_candidate",
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
        "production_diff_from_accepted_stage7b_c": False,
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
        "settlement_authorization_linear": True,
        "settlement_authorization_serializable": False,
        "settlement_authorization_reconstructible_from_input": False,
        "settlement_authorization_exact_request_bound": True,
        "settlement_authorization_seal_generation_bound": True,
        "settlement_authorization_checkpoint_bound": True,
        "settlement_authorization_payload_fingerprint_bound": True,
        "transport_plan_entry_bound": True,
        "ack_requires_finalized_and_sealed": True,
        "separate_ack_and_poison_capabilities": True,
        "poison_requires_zero_stage6_mutation": True,
        "poison_no_stage6_seal_advance": True,
        "holds_never_dlq_or_xack": True,
        "stable_settlement_key_excludes_payload_fingerprint": True,
        "marker_value_contains_payload_fingerprint": True,
        "same_key_same_fingerprint_idempotent": True,
        "same_key_different_fingerprint_conflict": True,
        "request_canonical_ack_marker": True,
        "post_publication_duplicate_semantics": True,
        "conflicting_duplicate_never_settles": True,
        "new_settlement_requires_expected_pel": True,
        "marker_retry_does_not_require_pel": True,
        "lua_validates_before_first_write": True,
        "single_hash_slot_required": True,
        "ambiguous_seal_commit_requires_reread": True,
        "redis_response_loss_scope_explicit": True,
        "d_a_rows": ["B-043..B-051", "B-054..B-056"],
        "d_a_rows_exclude_b052_b053": True,
        "d_b_rows": "B-057..B-063",
        "d_c_rows": "B-064..B-070",
        "d_c_closes_b052_b053": True,
        "implementation_open_after_design_acceptance": True,
        "design_negative_case_count": 44,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "cross_process_exactly_once_claimed": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "normative_stage7b_tz_sha256": TZ_SHA256,
        "normative_stage7b_matrix_sha256": MATRIX_SHA256,
        "normative_r1_contract_sha256": R1_CONTRACT_SHA256,
        "normative_r1_matrix_sha256": R1_MATRIX_SHA256,
    }
    require(descriptor == expected, "design descriptor drift")

    aggregate = json.loads(
        (ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text()
    )
    for key, value in {
        "schema_version": 2,
        "stage": "7B",
        "slice": "7B-d-design-R1",
        "status": "design_authority_clarification_candidate",
        "accepted_stage7b_c_predecessor": BASE,
        "implemented_count": 42,
        "pending_count": 38,
        "stage7b_d_design_frozen": False,
        "stage7b_d_design_r1_acceptance_pending": True,
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

    row_ownership = json.loads(
        (ROOT / "docs/stage-7/stage7b-d-row-ownership.json").read_text()
    )
    require(
        row_ownership
        == {
            "schema_version": 1,
            "stage": "7B-d-design-R1",
            "accepted_stage7b_c_predecessor": BASE,
            "slices": {
                "7B-d-a": {
                    "owned_rows": ["B-043..B-051", "B-054..B-056"],
                    "b052_b053_status": "pending_real_redis_restart",
                },
                "7B-d-b": {"owned_rows": ["B-057..B-063"]},
                "7B-d-c": {
                    "owned_rows": ["B-064..B-070"],
                    "also_closes_with_real_redis_restart": ["B-052", "B-053"],
                },
            },
            "implemented_rows": 42,
            "pending_rows": 38,
            "b052_b053_implemented": False,
        },
        "row ownership governance drift",
    )

    design = (ROOT / "docs/stage-7/stage7b-d-design.md").read_text()
    normalized_design = " ".join(design.lower().split())
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
        "opaque linear DurableAckAuthorized capability",
        "opaque linear RedisAckSettlementPlan",
        "opaque linear PoisonDlqAuthorized",
        "non-Clone, non-Copy",
        "cannot settle request B",
        "key never includes the proposed ACK/DLQ payload fingerprint",
        "same key with a different fingerprint fails before `XADD` or `XACK`",
        "source entry is pending in the expected consumer group before",
        "does not require PEL membership",
        "No expected semantic/type/conflict error is reachable after",
        "does not advance or fabricate a Stage6 recovery seal",
        "cached `generation + 1` retry is forbidden",
        "Redis storage rollback",
        "B-052/B-053 remain pending",
        "real-Redis restart closure of B-052/B-053",
        "one reviewed Lua primitive",
        "response loss",
        "Source poll and claim scan",
        "explicit abort/cancel",
        "Legacy SQLite/M3",
        "external exactly-once execution claim: false",
    )
    for token in required_design:
        normalized_token = " ".join(token.lower().split())
        require(normalized_token in normalized_design, f"design invariant absent: {token}")
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
    require(
        sha256(ROOT / "docs/stage-7/TZ_STAGE7B_D_DESIGN_R1_IMPLEMENTATION_CONTRACT_2026-08-13.md")
        == R1_CONTRACT_SHA256,
        "normative Stage 7B-d Design R1 contract drift",
    )
    r1_matrix = ROOT / "docs/stage-7/STAGE7B_D_DESIGN_R1_ACCEPTANCE_MATRIX_2026-08-13.csv"
    require(sha256(r1_matrix) == R1_MATRIX_SHA256, "normative Design R1 matrix drift")
    with r1_matrix.open(newline="") as handle:
        r1_rows = list(csv.DictReader(handle))
    require(len(r1_rows) == 30, "Design R1 matrix row count drift")
    require(
        [row["ID"] for row in r1_rows]
        == [f"D-R1-{index:03d}" for index in range(1, 31)],
        "Design R1 matrix ID/order drift",
    )
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    print(
        "stage7b-d-design-check: PASS r1_rows=30 rows=80 "
        "implemented=42 pending=38 production_diff=false"
    )


if __name__ == "__main__":
    main()
