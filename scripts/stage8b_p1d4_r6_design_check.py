#!/usr/bin/env python3
"""Fail-closed checker for the P1-d4 R6 source-shaped design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import subprocess
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
P1D3_REF = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R3_REF = "e1ce6d3baec3974d8dfd05c2f3de00110e0605bf"
R5_REF = "b377c0275f1ce5f01cfe9b223724bf1542f985e2"

GENERAL_SHA256 = "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"
BASE_SHA256 = "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6"
DESIGN_SHA256 = "99d5696ac3346f5c12bd4c5189a5c318ac2aaeadd329d392ae440014a3e1d7e2"
DISCOVERY_SHA256 = "22e70bb86a5e4b228ed4f731b1aea2d0f196fcb611abeed2fe7d03f0edb7f77e"
GM_SHA256 = "e6a284372cc99fab3bfeadd9afa0d8e1ddec0070966f79505a5cb09dbb627561"
AMENDMENT_SHA256 = "ca381b8e6891840303d9c602ca27e379a072a3cf344526f14ee1ce170e5f7f53"
SOURCE_SHAPE_SHA256 = "08890b48633918b52623c28222d9e02445e1820e7b4343f9b208bdffd6df1917"

GENERAL = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
BASE = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
GM = ROOT / "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v2.csv"
AMENDMENT = ROOT / "docs/stage-8/stage8b-p1d4-r6-acceptance-amendment.csv"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r6.md"
DISCOVERY = ROOT / "docs/stage-8/stage8b-p1d4-source-discovery-r6.md"
SOURCE_SHAPE = ROOT / "docs/stage-8/stage8b-p1d4-source-shape-r6.json"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r6.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"

SOURCE_HASHES = {
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs":
        "17edbe4c41315aef97faa8574eedd884e75ad90cb70e9aa01812bc6e197a7295",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs":
        "ef113596c8f9835669987853ea1ce4bdb976c73d507cf8014dabcff23a923ef8",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs":
        "619a6f7e4aff4d2d6ba2e0c86036da7df96383e3de020afbb6eaa3f907cb8478",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs":
        "2e9630533aa0470ad73294fb0afff6e155bf0a738eba2dae51eca9d0f7387a2b",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs":
        "376b0af099cdb95a31c48a4ab1b23272aa34dc436b19dd61898ddc078bde6480",
}

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r6.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r6.json",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v2.csv",
    "docs/stage-8/stage8b-p1d4-r6-acceptance-amendment.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r6.md",
    "docs/stage-8/stage8b-p1d4-source-shape-r6.json",
    "scripts/make_stage8b_p1d4_r6_design_handoff.py",
    "scripts/stage8b_p1d4_r6_design_check.py",
    "scripts/stage8b_p1d4_r6_design_gate.sh",
    "scripts/stage8b_p1d4_r6_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r6_design_negative_harness.py",
}

OWNERS = [
    "P1d4GeneratedMarketPrepublicationPending",
    "P1d4GeneratedMarketDispatchPending",
    "P1d4GeneratedMarketOrderPending",
    "P1d4GeneratedMarketPreFinalizationPending",
    "P1d4GeneratedMarketPreAckPending",
    "P1d4GeneratedMarketAckCommitted",
    "P1d4GeneratedMarketTruthCommitted",
]

PUBLICATION_FIELDS = [
    "schema_version",
    "domain",
    "source_stream",
    "source_group",
    "source_m10_redis_id",
    "semantic_batch_id_sha256",
    "strategy_request_id",
    "canonical_command_sha256",
    "canonical_envelope_sha256",
    "command_stream",
    "command_group",
    "command_entry_id",
    "prepublication_package_generation",
    "prepublication_seal_generation",
    "prepublication_seal_commitment_sha256",
    "publication_binding_sha256",
]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def read_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(value.splitlines()))


def committed_changed_files() -> set[str]:
    parent = subprocess.check_output(
        ["git", "rev-parse", "HEAD^"], cwd=ROOT, text=True
    ).strip()
    require(parent == R5_REF, f"R6 parent drifted: {parent}")
    output = subprocess.check_output(
        ["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
        cwd=ROOT,
        text=True,
    )
    return set(output.splitlines())


def source_at_baseline(path: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "show", f"{R5_REF}:{path}"], cwd=ROOT, text=True
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return (ROOT / path).read_text(encoding="utf-8")


def require_order(value: str, tokens: list[str], label: str) -> None:
    cursor = -1
    for token in tokens:
        position = value.find(token, cursor + 1)
        require(position >= 0, f"{label} missing token: {token}")
        require(position > cursor, f"{label} order drifted: {token}")
        cursor = position


def validate_source_shape(shape: dict[str, Any], sources: dict[str, str]) -> None:
    require(shape.get("baseline_ref") == R5_REF, "source-shape baseline drifted")
    require(shape.get("status") == "R6_SOURCE_SHAPE_PINNED", "source-shape status drifted")
    require(shape.get("source_sha256") == SOURCE_HASHES, "source-shape hash inventory drifted")
    facts = shape.get("facts", {})
    require(facts.get("existing_p1d2_classifier_suffix_lengths") == [3, 4], "classifier suffix fact drifted")
    require(facts.get("generated_market_journal_version") == "V1", "Market journal version drifted")
    require(facts.get("generated_market_v3_authorized") is False, "Market V3 was authorized")
    require(facts.get("market_chain") == [
        "DispatchAttemptRecorded", "BrokerOrderObserved", "BrokerTradeObserved", "RequestFinalized"
    ], "Market chain fact drifted")
    require(facts.get("order_and_trade_appended_separately") is True, "separate append fact drifted")
    require(facts.get("sequence_allocator_after_request_finalized") is True, "allocator timing fact drifted")
    require(facts.get("sequence_pair_crash_hook") == "p1d2-after-sequence-pair-before-ack", "pair hook fact drifted")

    for path, digest in SOURCE_HASHES.items():
        require(path in sources, f"source input missing: {path}")
        require(sha256_text(sources[path]) == digest, f"committed source shape drifted: {path}")

    live = sources["crates/strategy-runtime-core/src/stage6d_live_core.rs"]
    for token in (
        "if !matches!(suffix_len, 3 | 4)",
        "let [dispatch, order, trade, rest @ ..] = v1.as_slice()",
        "Stage6JournalPayloadV1::DispatchAttemptRecorded",
        "Stage6JournalPayloadV1::BrokerOrderObserved",
        "Stage6JournalPayloadV1::BrokerTradeObserved",
        "Stage6JournalPayloadV1::RequestFinalized",
        "for record in records",
        "recovered.journal_mut().append(&record)?",
        "recovered.refresh_after_append()?",
        "p1d2-after-stage6-before-request-finalized",
        "p1d2-after-request-finalized-before-ack",
    ):
        require(token in live, f"Stage6 Market source assertion missing: {token}")
    require_order(live, [
        "let report = execute_stage6d_paper_outcome",
        "p1d2-after-stage6-before-request-finalized",
        "let report = finalize_stage7a_paper_request",
        "p1d2-after-request-finalized-before-ack",
        "apply_stage8b_p1d2_ack_stage",
    ], "Stage6/7/ACK ordering")

    feedback = sources["crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs"]
    require_order(feedback, [
        ".stage8b_p1d2_allocate_sequence_pair()",
        "stage8b_p1d2_test_record_sequence_pair_before_crash",
        "p1d2-after-sequence-pair-before-ack",
        "apply_stage5g_mock_ack",
    ], "sequence allocator ordering")

    ack = sources["crates/strategy-runtime-core/src/stage5g_mock_ack.rs"]
    for token in ("P1-d2's sole sequence-pair allocator", "checked_add(1)?", "Some((self, seq_ack, seq_truth))"):
        require(token in ack, f"ACK allocator assertion missing: {token}")

    provider = sources["crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs"]
    for token in (
        "Post-dispatch provider capability",
        "bind_stage8b_p1d1_market_dispatch",
        "pub fn execute(self) -> Stage8bP1d1MarketOutcomeBundle",
        "reconstruct_stage8b_p1d1_market_outcome_evidence",
        "inventing or reacquiring calendar evidence",
    ):
        require(token in provider, f"provider source assertion missing: {token}")

    redis = sources["crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs"]
    for token in (
        "pub struct Stage8bP1RedisCommandPublicationReceipt",
        "pub source_m10_redis_id: String",
        "pub semantic_batch_id_sha256: String",
        "pub strategy_request_id: StrategyRequestId",
        "pub canonical_command_sha256: String",
        "pub canonical_envelope_sha256: String",
        "pub command_entry_id: String",
        "pub covering_seal_generation: u64",
        "pub covering_seal_commitment_sha256: String",
        "marker['source_stream']",
        "marker['source_group']",
        "marker['command_stream']",
        "marker['command_group']",
        "marker['command_entry_id']",
    ):
        require(token in redis, f"publication source assertion missing: {token}")


def validate(
    general: str,
    base: str,
    gm: str,
    amendment: str,
    design: str,
    discovery: str,
    shape_text: str,
    evidence: dict[str, Any],
    status: str,
    roadmap: str,
    sources: dict[str, str],
    *,
    verify_hashes: bool = True,
) -> None:
    shape = json.loads(shape_text)
    if verify_hashes:
        require(sha256_text(general) == GENERAL_SHA256, "accepted general matrix drifted")
        require(sha256_text(base) == BASE_SHA256, "inherited base matrix drifted")
        require(sha256_text(gm) == GM_SHA256, "R6 generated-Market matrix drifted")
        require(sha256_text(amendment) == AMENDMENT_SHA256, "R6 amendment drifted")
        require(sha256_text(design) == DESIGN_SHA256, "R6 design drifted")
        require(sha256_text(discovery) == DISCOVERY_SHA256, "R6 discovery drifted")
        require(sha256_text(shape_text) == SOURCE_SHAPE_SHA256, "R6 source-shape artifact drifted")

    validate_source_shape(shape, sources)

    for token in (
        "Status: R6 design-only review candidate",
        R5_REF,
        R3_REF,
        P1D3_REF,
        "Option A",
        "one retained M10",
        "XACK-last",
        "active total: 105 exact positive cells",
        "There is no generated-Market Stage6 V3 record",
        "DispatchAttemptRecorded V1",
        "BrokerOrderObserved V1",
        "BrokerTradeObserved V1",
        "RequestFinalized V1",
        "P1d4GeneratedMarketDispatchPending",
        "P1d4GeneratedMarketOrderPending",
        "P1d4GeneratedMarketPreFinalizationPending",
        "pair_not_allocated_but_deterministically_reconstructible",
        "exact_pair_allocated_in_memory_and_reconstructed_after_restart",
        "Stage8bP1d4CommandPublicationBindingV1",
        "canonical_envelope_sha256",
        "command_entry_id",
        "prepublication_seal_commitment_sha256",
        "byte-identical duplicate entry is not equivalent",
        "source implementation remains",
        "paused until independent R6 acceptance",
    ):
        require(token in design, f"design invariant missing: {token}")
    for owner in OWNERS:
        require(owner in design, f"owner missing from design: {owner}")
    for field in PUBLICATION_FIELDS:
        require(field in design, f"publication field missing from design: {field}")
    for token in (
        "committed R5 parent",
        "Stage6 V1 records",
        "suffix lengths 3 or 4",
        "dispatch+order",
        "RequestFinalized-before-allocator",
        "Stage8bP1d4CommandPublicationBindingV1",
        "saved uncommitted source work remains excluded",
    ):
        require(token in discovery, f"discovery invariant missing: {token}")

    general_rows = read_rows(general)
    base_rows = read_rows(base)
    gm_rows = read_rows(gm)
    amendment_rows = read_rows(amendment)
    require(len(general_rows) == 88, "general row count drifted")
    require(len(base_rows) == 92, "base row count drifted")
    require(len(gm_rows) == 13, "generated-Market row count drifted")
    require(len(amendment_rows) == 32, "R6 amendment row count drifted")
    require([row["cell_id"] for row in gm_rows] == [f"P1D4GM-{index:03d}" for index in range(1, 14)], "GM IDs/order drifted")
    require([row["frontier_id"] for row in gm_rows] == [f"GM{index:02d}" for index in range(13)], "GM frontiers drifted")
    require(all(row["parent_scenario_id"] == "S05" for row in gm_rows), "GM parent drifted")
    require([row["stage6_durable_frontier"] for row in gm_rows] == [
        "none", "none", "none", "dispatch_only", "dispatch_only",
        "dispatch_plus_order", "dispatch_plus_order_plus_trade",
        "complete_v1_chain", "complete_v1_chain", "complete_v1_chain",
        "complete_v1_chain", "complete_v1_chain", "complete_v1_chain",
    ], "V1 durable frontier graph drifted")
    require([row["expected_restart_disposition"] for row in gm_rows] == [
        OWNERS[0], OWNERS[0], OWNERS[0], OWNERS[1], OWNERS[1], OWNERS[2],
        OWNERS[3], OWNERS[4], OWNERS[4], OWNERS[4], OWNERS[5], OWNERS[5], OWNERS[6],
    ], "owner graph drifted")
    require(all(row["source_pel_before"] == "exact_source_pending_1" and row["source_pel_after"] == "exact_source_pending_1" for row in gm_rows), "source retention weakened")
    require(gm_rows[0]["publication_binding_expectation"] == "absent_before_xadd", "pre-XADD binding rule drifted")
    require(all("exact_external_reconstruction_required" in row["publication_binding_expectation"] or "reconstruct_and_persist_before_s_ack" in row["publication_binding_expectation"] for row in gm_rows[1:10]), "pre-S_ack publication reconstruction drifted")
    require(all(row["publication_binding_expectation"] == "mandatory_hmac_covered_exact_binding" for row in gm_rows[10:]), "post-S_ack binding requirement drifted")
    require(all(row["sequence_expectation"] == "pair_not_allocated" for row in gm_rows[:7]), "pair allocated before finalization")
    require(gm_rows[7]["sequence_expectation"] == "pair_not_allocated_but_deterministically_reconstructible", "pre-allocator frontier drifted")
    require(gm_rows[8]["sequence_expectation"] == "exact_pair_allocated_in_memory_and_reconstructed_after_restart", "post-allocation frontier drifted")
    require("pre_kill_marker" in gm_rows[9]["sequence_expectation"], "pre-kill equality frontier missing")
    require(gm_rows[3]["dispatch_v1_total"] == "0_replay_total_exactly_1" and gm_rows[4]["dispatch_v1_total"] == "0_replay_total_exactly_1", "second dispatch was permitted")
    require(gm_rows[5]["only_legal_continuation"] == "validate_reconstructed_evidence_and_append_only_missing_trade", "order-only continuation broadened")
    require(gm_rows[5]["trade_v1_total"] == "+1_exact", "order-only missing-trade append lost")
    require(gm_rows[6]["request_finalized_v1_total"] == "+1_exact", "pre-finalization append lost")
    require(all(row["xack_delta"] == "0" for row in gm_rows[:-1]), "early XACK opened")
    require(gm_rows[-1]["xack_delta"] == "+1_exact", "terminal XACK authority missing")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in base_rows + gm_rows), "duplicate/conflict coverage weakened")
    require([row["id"] for row in amendment_rows] == [f"P1D4R6-{index:03d}" for index in range(1, 33)], "R6 amendment IDs/order drifted")
    require(all(row["status"] == "REQUIRED" for row in amendment_rows), "R6 requirement weakened")

    require(evidence.get("stage") == "Stage 8B-P1-d4 crash/replay design R6 correction", "evidence stage drifted")
    require(evidence.get("status") == "DESIGN_R6_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("design_parent_ref") == R5_REF, "evidence parent drifted")
    require(evidence.get("accepted_r3_design_ref") == R3_REF, "evidence R3 lineage drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == P1D3_REF, "evidence business lineage drifted")
    expected_hashes = {
        "design_r6_sha256": DESIGN_SHA256,
        "source_discovery_r6_sha256": DISCOVERY_SHA256,
        "source_shape_r6_sha256": SOURCE_SHAPE_SHA256,
        "inherited_base_matrix_sha256": BASE_SHA256,
        "generated_market_matrix_sha256": GM_SHA256,
        "acceptance_amendment_sha256": AMENDMENT_SHA256,
    }
    for key, expected in expected_hashes.items():
        require(evidence.get(key) == expected, f"evidence hash drifted: {key}")
    require(evidence.get("base_matrix_rows") == 92 and evidence.get("generated_market_matrix_rows") == 13 and evidence.get("active_positive_cells") == 105, "evidence proof inventory drifted")
    require(evidence.get("acceptance_amendment_rows") == 32, "evidence amendment inventory drifted")
    require(evidence.get("targeted_negative_cases") == 48 and evidence.get("total_contract_negative_cases") == 176, "negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False and evidence.get("source_wip_included") is False, "design/source boundary opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    composition = evidence.get("composition_contract", {})
    require(composition.get("option") == "A_RETAIN_SINGLE_SOURCE", "Option A drifted")
    require(composition.get("source_m10_count") == 1 and composition.get("source_retained_through") == "combined_s_truth", "source retention drifted")
    require(composition.get("generated_market_journal_version") == "V1", "journal version evidence drifted")
    require(composition.get("partial_v1_suffix_lengths") == [1, 2, 3, 4], "partial suffix inventory drifted")
    require(composition.get("owners") == OWNERS, "evidence owner inventory drifted")
    require(composition.get("command_publication_binding") == "Stage8bP1d4CommandPublicationBindingV1", "publication binding evidence missing")
    require(evidence.get("publication_binding_fields") == PUBLICATION_FIELDS, "publication field inventory drifted")

    for value, label in ((status, "status"), (roadmap, "roadmap")):
        for token in (R5_REF, "P1-d4 R6", "Stage6 V1", "105", "implementation", "paused", "independent R6 acceptance", "Redis DB0/VPS"):
            require(token in value, f"{label} missing token: {token}")


def read_inputs() -> tuple[Any, ...]:
    return (
        GENERAL.read_text(encoding="utf-8"),
        BASE.read_text(encoding="utf-8"),
        GM.read_text(encoding="utf-8"),
        AMENDMENT.read_text(encoding="utf-8"),
        DESIGN.read_text(encoding="utf-8"),
        DISCOVERY.read_text(encoding="utf-8"),
        SOURCE_SHAPE.read_text(encoding="utf-8"),
        json.loads(EVIDENCE.read_text(encoding="utf-8")),
        STATUS.read_text(encoding="utf-8"),
        ROADMAP.read_text(encoding="utf-8"),
        {path: source_at_baseline(path) for path in SOURCE_HASHES},
    )


def main() -> None:
    try:
        content_only = sys.argv[1:] == ["--content-only"]
        require(not sys.argv[1:] or content_only, "usage: stage8b_p1d4_r6_design_check.py [--content-only]")
        if not content_only:
            require(committed_changed_files() == EXPECTED_CHANGED, "R6 committed path scope drifted")
        validate(*read_inputs())
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r6-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r6-design-scope files=13 base_cells=92 generated_market_cells=13 active_cells=105 amendments=32 source_shape=exact design_only=true")


if __name__ == "__main__":
    main()
