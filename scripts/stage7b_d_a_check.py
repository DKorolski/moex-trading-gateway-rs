#!/usr/bin/env python3
"""Stage 7B-d-a Redis-free lifecycle/seal authority acceptance checker."""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DESIGN_BASE = "00cead2989493b44e0d86ead29b95d57a7fbcbe2"
STAGE7B_C_BASE = "c57ae8d5f98bbb11df0a81f78262d3916b276d81"
BRANCH = "stage7b-production-durability"
OWNED = {f"B-{value:03d}" for value in range(43, 52)} | {
    "B-054",
    "B-055",
    "B-056",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def source_block(source: str, needle: str) -> str:
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
    raise CheckFailure(f"unterminated source block: {needle}")


def check_lineage() -> None:
    merge_base = subprocess.check_output(
        ["git", "merge-base", "HEAD", DESIGN_BASE], cwd=ROOT, text=True
    ).strip()
    require(merge_base == DESIGN_BASE, "candidate is not based on accepted Design R1")
    branch = subprocess.check_output(
        ["git", "branch", "--show-current"], cwd=ROOT, text=True
    ).strip()
    require(branch == BRANCH, "Stage 7B-d-a branch drift")


def check_production_scope() -> None:
    changed = set(
        subprocess.check_output(
            ["git", "diff", "--name-only", DESIGN_BASE, "--"], cwd=ROOT, text=True
        ).splitlines()
    )
    allowed_production = {
        "Cargo.lock",
        "crates/runtime-durable-service/Cargo.toml",
        "crates/runtime-durable-service/src/lib.rs",
        "crates/runtime-durable-service/src/recovery.rs",
        "crates/strategy-runtime-core/src/lib.rs",
        "crates/strategy-runtime-core/src/stage5g_order_position.rs",
        "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    }
    production = {
        path
        for path in changed
        if path == "Cargo.lock" or path == "Cargo.toml" or path.startswith("crates/")
    }
    require(production <= allowed_production, f"d-a production scope expanded: {sorted(production - allowed_production)}")
    for forbidden_prefix in (
        "crates/finam-client/",
        "crates/finam-gateway/",
        "crates/runtime-command-bridge/",
        ".github/workflows/",
    ):
        require(not any(path.startswith(forbidden_prefix) for path in changed), f"closed surface changed: {forbidden_prefix}")


def check_descriptors() -> None:
    descriptor = json.loads(
        (ROOT / "docs/stage-7/stage7b-d-entry-descriptor.json").read_text()
    )
    aggregate = json.loads(
        (ROOT / "docs/stage-7/stage7b-entry-descriptor.json").read_text()
    )
    ownership = json.loads(
        (ROOT / "docs/stage-7/stage7b-d-row-ownership.json").read_text()
    )
    expected = {
        "slice": "7B-d-a",
        "status": "implementation_candidate",
        "accepted_design_r1_ref": DESIGN_BASE,
        "implemented_count": 54,
        "pending_count": 26,
        "stage7b_d_design_frozen": True,
        "stage7b_d_a_implementation_started": True,
        "stage7b_d_a_acceptance_pending": True,
        "stage7b_d_b_open": False,
        "stage7b_d_c_open": False,
        "d_a_owned_rows_implemented": True,
        "d_a_rows_exclude_b052_b053": True,
        "b052_b053_implemented": False,
        "d_a_negative_case_count": 29,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    for key, value in expected.items():
        require(descriptor.get(key) == value, f"d-a descriptor drift: {key}")
    aggregate_expected = {
        "slice": "7B-d-a",
        "status": "implementation_candidate",
        "implemented_count": 54,
        "pending_count": 26,
        "stage7b_d_design_frozen": True,
        "stage7b_d_design_r1_acceptance_pending": False,
        "accepted_stage7b_d_design_r1_ref": DESIGN_BASE,
        "stage7b_d_implementation_started": True,
        "stage7b_d_a_acceptance_pending": True,
        "stage7b_d_b_open": False,
        "stage7b_d_c_open": False,
        "redis_consumer_attached": False,
        "redis_settlement_enabled": False,
        "xack_enabled": False,
        "finam_post_delete": False,
        "broker_network_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }
    for key, value in aggregate_expected.items():
        require(aggregate.get(key) == value, f"aggregate descriptor drift: {key}")
    require(ownership.get("accepted_design_r1_ref") == DESIGN_BASE, "row ownership design ref drift")
    require(ownership.get("implemented_rows") == 54, "row ownership implemented count drift")
    require(ownership.get("pending_rows") == 26, "row ownership pending count drift")
    require(ownership.get("b052_b053_implemented") is False, "B-052/B-053 closed early")


def check_proof_map() -> None:
    subprocess.run(["python3", "scripts/stage7b_proof_map.py"], cwd=ROOT, check=True)
    proof_map = json.loads(
        (ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text()
    )
    require(proof_map["slice"] == "7B-d-a", "proof-map slice drift")
    require(proof_map["implemented_count"] == 54, "proof-map implemented count drift")
    require(proof_map["pending_count"] == 26, "proof-map pending count drift")
    rows = {row["row_id"]: row for row in proof_map["proofs"]}
    for row_id in OWNED:
        require(rows[row_id]["status"] == "implemented", f"d-a row pending: {row_id}")
        require(rows[row_id]["exact_witness"] != "pending", f"d-a witness absent: {row_id}")
    for row_id in ("B-052", "B-053"):
        require(rows[row_id]["status"] == "pending", f"{row_id} closed before real Redis restart")
        require(rows[row_id]["proof_type"] == "real_redis_restart", f"{row_id} proof type drift")


def check_source() -> None:
    recovery = (ROOT / "crates/runtime-durable-service/src/recovery.rs").read_text()
    service_lib = (ROOT / "crates/runtime-durable-service/src/lib.rs").read_text()
    live_core = (ROOT / "crates/strategy-runtime-core/src/stage6d_live_core.rs").read_text()
    manifest = (ROOT / "crates/runtime-durable-service/Cargo.toml").read_text()
    for forbidden in ("redis.workspace", "redis =", "reqwest", "broker-finam", "finam-gateway"):
        require(forbidden not in manifest, f"d-a forbidden dependency: {forbidden}")
    for token in (
        "pub(crate) struct Stage7bFinalizedPaperRequest",
        "pub(crate) struct Stage7bDurableAckAuthorized",
        "seal_commit_uncertain: bool",
        "pub fn admit_paper_command(",
        "pub fn record_paper_outcome(",
        "pub(crate) fn finalize_paper_request(",
        "pub(crate) fn finalize_replayed_paper_request(",
        "pub(crate) fn authorize_finalized_ack(",
        "refresh_stage7b_durable_frontier(&mut self.recovered)?",
        "self.advance_recovery_seal(commitment_key)?",
        "durable_ack_authority(&self.committed_seal, current)",
        "self.seal_commit_uncertain = true",
        "Stage7bAckPublicationDecision::Canonical",
        "Stage7bAckPublicationDecision::Duplicate",
        "Stage7bAckPublicationDecision::Conflict",
        "stage7b_d_a_b044_sigkill_after_accepted_recovers_dispatch_once",
        "stage7b_d_a_b045_sigkill_after_dispatch_never_blind_redispatches",
        "stage7b_d_a_b046_sigkill_during_unknown_effect_requires_reconciliation",
        "stage7b_d_a_b047_sigkill_after_outcome_reconstructs_finalization_and_ack",
        "stage7b_d_a_b048_sigkill_after_finalization_reconstructs_canonical_ack",
        "stage7b_d_a_b050_seal_failure_blocks_authorization_and_readiness",
        "stage7b_d_a_b051_sigkill_after_seal_reconstructs_without_provider",
        "stage7b_d_a_b054_sequential_cancel_survives_restart_and_reseals",
    ):
        require(token in recovery, f"d-a source invariant absent: {token}")
    authority_token = "pub(crate) struct Stage7bDurableAckAuthorized"
    authority_prefix = recovery[max(0, recovery.index(authority_token) - 100):recovery.index(authority_token)]
    require("#[derive" not in authority_prefix, "ACK authority became derivable")
    require(
        "pub struct Stage7bDurableAckAuthorized" not in recovery,
        "ACK authority escaped crate-private boundary",
    )
    for forbidden in (
        "impl Clone for Stage7bDurableAckAuthorized",
        "impl Copy for Stage7bDurableAckAuthorized",
        "Serialize for Stage7bDurableAckAuthorized",
        "Deserialize for Stage7bDurableAckAuthorized",
        "pub fn new_stage7b_durable_ack",
    ):
        require(forbidden not in recovery, f"linear ACK authority escape: {forbidden}")
    classifier = source_block(recovery, "    pub(crate) fn classify_publication(")
    require(
        "None => Stage7bAckPublicationDecision::Canonical" in classifier,
        "first publication no longer canonical",
    )
    authorize = source_block(recovery, "    pub(crate) fn authorize_finalized_ack(")
    ordered = (
        "self.require_lifecycle_available()?",
        "stage7b_finalized_request_facts(",
        "refresh_stage7b_durable_frontier(&mut self.recovered)?",
        "self.advance_recovery_seal(commitment_key)?",
        "durable_ack_authority(&self.committed_seal, current)",
    )
    positions = [authorize.index(token) for token in ordered]
    require(positions == sorted(positions), "finalize/seal/ACK ordering drift")
    advance = source_block(recovery, "    fn advance_recovery_seal(")
    ordered = (
        "advance_stage6d_restart_package(",
        "checked_add(1)",
        "Stage7bRecoverySealV1::new(",
        "commit_recovery_seal(&next)",
        "read_committed_recovery_seal()",
        "Stage7bRecoverySealV1::decode_canonical(",
        "validate_recovered_binding(&self.recovered, &committed, &identity)",
        "self.committed_seal = committed",
    )
    positions = [advance.index(token) for token in ordered]
    require(positions == sorted(positions), "seal commit/reread/update ordering drift")
    require("use runtime_durable_service::Stage7bDurableAckAuthorized;" in service_lib, "compile-fail ACK privacy witness absent")
    for token in (
        "pub fn advance_stage6d_restart_package(",
        "pub fn refresh_stage7b_durable_frontier(",
        "pub fn stage7b_finalized_request_facts(",
        "stage7b_test_authenticated_cancel_restart_fixture",
    ):
        require(token in live_core, f"Stage 6 d-a bridge absent: {token}")


def check_governance() -> None:
    texts = [
        (ROOT / "docs/current-status.md").read_text(),
        (ROOT / "docs/roadmap.md").read_text(),
        (ROOT / "docs/reviewer-onboarding-and-roadmap.md").read_text(),
        (ROOT / "docs/stage-7/stage7b-d-a-implementation.md").read_text(),
    ]
    for text in texts:
        compact = " ".join(text.split())
        require(DESIGN_BASE in compact, "accepted Design R1 ref absent from governance")
        require("B-052/B-053" in compact or "B-052" in compact, "deferred real-Redis rows absent")
    implementation = texts[-1]
    for forbidden_claim in ("real FINAM enabled", "runtime-live enabled", "real orders enabled"):
        require(forbidden_claim not in implementation, f"forbidden governance claim: {forbidden_claim}")


def main() -> None:
    check_lineage()
    check_production_scope()
    check_descriptors()
    check_proof_map()
    check_source()
    check_governance()
    print("stage7b-d-a-check: PASS rows=12 implemented=54 pending=26 redis=false")


if __name__ == "__main__":
    try:
        main()
    except (CheckFailure, subprocess.CalledProcessError, ValueError, KeyError) as error:
        raise SystemExit(f"stage7b-d-a-check: FAIL: {error}")
