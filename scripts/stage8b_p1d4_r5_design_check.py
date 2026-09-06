#!/usr/bin/env python3
"""Fail-closed checker for the P1-d4 R5 retained-source design correction."""

from __future__ import annotations

import csv
import hashlib
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
P1D3_REF = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R3_REF = "e1ce6d3baec3974d8dfd05c2f3de00110e0605bf"
R4_REF = "ebede1d804f5eff50d6b4b9455edb08735e1be2c"
GENERAL_SHA256 = "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"
R4_MATRIX_SHA256 = "74fc128b06d188942008449f05977d8eb46630c3ccfc364659a75d77d0e5810f"
R5_DESIGN_SHA256 = "842e3a36ea348cb4c523adcdc3fd96e1f538599a5dd15f461bf55ae38ebf8ce9"
R5_DISCOVERY_SHA256 = "0c16ac5c2d6226e1d57a927e90c46a04ca7d44f535c9fe82a57c99b4dbd31d50"
R5_MATRIX_SHA256 = "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6"
GM_MATRIX_SHA256 = "87168c9089def33072cae2561b5ad1c182c8d0d538b2abd6c1170e8ea577e3bd"
AMENDMENT_SHA256 = "f6fcb398ce40ec96d21c87302806714429280d6737a7c8eb4e159dbb3e9437cb"

GENERAL = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
R4_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv"
R5_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
GM_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v1.csv"
AMENDMENT = ROOT / "docs/stage-8/stage8b-p1d4-r5-acceptance-amendment.csv"
DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r5.md"
DISCOVERY = ROOT / "docs/stage-8/stage8b-p1d4-source-discovery-r5.md"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r5.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r5.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r5.json",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v1.csv",
    "docs/stage-8/stage8b-p1d4-r5-acceptance-amendment.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r5.md",
    "scripts/make_stage8b_p1d4_r5_design_handoff.py",
    "scripts/stage8b_p1d4_r5_design_check.py",
    "scripts/stage8b_p1d4_r5_design_gate.sh",
    "scripts/stage8b_p1d4_r5_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r5_design_negative_harness.py",
}

EXPECTED_CORRECTIONS = {
    ("P1D4C-039", "precondition"): "callback_and_exact_publication_durable_before_generated_market_continuation",
    ("P1D4C-039", "expected_restart_disposition"): "P1d4GeneratedMarketPrepublicationPending",
    ("P1D4C-039", "only_legal_continuation"): "reclaim_exact_source_validate_existing_publication_then_continue_generated_market_without_callback_or_xadd_replay",
    ("P1D4C-039", "sequence_expectation"): "no_order_sequence_before_generated_market_wal",
    ("P1D4C-039", "provider_delta"): "0_before_generated_market_continuation",
    ("P1D4C-039", "schedule_authority_delta"): "0_before_generated_market_continuation",
    ("P1D4C-039", "xack_expectation"): "forbidden_until_generated_market_s_truth",
    ("P1D4C-040", "precondition"): "redis_xack_reply_1_parsed_after_generated_market_s_truth_before_client_completion",
    ("P1D4C-040", "expected_restart_disposition"): "P1d4GeneratedMarketTruthCommitted",
    ("P1D4C-040", "only_legal_continuation"): "prove_pel_absent_and_group_frontier_then_AlreadyAcknowledged",
    ("P1D4C-040", "sequence_expectation"): "generated_market_exact_reserved_ack_truth_pair_unchanged",
    ("P1D4C-040", "provider_delta"): "0_replay_total_exactly_1",
    ("P1D4C-040", "schedule_authority_delta"): "0_reissue_total_exactly_1",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def committed_changed_files() -> set[str]:
    parent = subprocess.check_output(["git", "rev-parse", "HEAD^"], cwd=ROOT, text=True).strip()
    require(parent == R4_REF, f"R5 parent drifted: {parent}")
    output = subprocess.check_output(
        ["git", "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
        cwd=ROOT,
        text=True,
    )
    return set(output.splitlines())


def read_rows(value: str) -> list[dict[str, str]]:
    return list(csv.DictReader(value.splitlines()))


def validate(
    general: str,
    r4_matrix: str,
    r5_matrix: str,
    gm_matrix: str,
    amendment: str,
    design: str,
    discovery: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
    *,
    verify_hashes: bool = True,
) -> None:
    if verify_hashes:
        require(sha256_text(general) == GENERAL_SHA256, "accepted general matrix drifted")
        require(sha256_text(r4_matrix) == R4_MATRIX_SHA256, "R4 matrix drifted")
        require(sha256_text(r5_matrix) == R5_MATRIX_SHA256, "R5 base matrix drifted")
        require(sha256_text(gm_matrix) == GM_MATRIX_SHA256, "generated-Market matrix drifted")
        require(sha256_text(amendment) == AMENDMENT_SHA256, "R5 amendment drifted")
        require(sha256_text(design) == R5_DESIGN_SHA256, "R5 design drifted")
        require(sha256_text(discovery) == R5_DISCOVERY_SHA256, "R5 discovery drifted")

    for token in (
        "Status: R5 design-only review candidate",
        R4_REF,
        R3_REF,
        P1D3_REF,
        "Option A",
        "There is one later-bar M10 source",
        "remains pending",
        "XACK is last",
        "Stage8bP1d4GeneratedMarketCompositionV1",
        "peer of, not a field inside",
        "contains no raw",
        "Prepublication | AckCommitted | TruthCommitted",
        "P1d4GeneratedMarketPrepublicationPending",
        "P1d4GeneratedMarketPreAckPending",
        "P1d4GeneratedMarketAckCommitted",
        "P1d4GeneratedMarketTruthCommitted",
        "active total: 102 exact positive cells",
        "source missing before combined `S_truth`",
        "source implementation remains",
        "paused until independent R5 acceptance",
    ):
        require(token in design, f"design invariant missing: {token}")
    require(
        design.count("P1d4GeneratedMarketTruthCommitted") == 2,
        "design terminal owner inventory drifted",
    )
    for token in (
        "retained delivery end to end",
        "does not create a second M10 source",
        "Durable(Runtime(RestartRuntimeRequired))",
        "peer composition",
        "No new Stage6 journal record is required",
        "saved uncommitted source work remains excluded",
    ):
        require(token in discovery, f"discovery invariant missing: {token}")

    general_rows = read_rows(general)
    require(len(general_rows) == 88, "general row count drifted")
    old_rows = read_rows(r4_matrix)
    new_rows = read_rows(r5_matrix)
    require(len(old_rows) == len(new_rows) == 92, "base proof-cell count drifted")
    require(list(old_rows[0]) == list(new_rows[0]), "base proof-cell columns drifted")
    require(
        [row["cell_id"] for row in new_rows] == [f"P1D4C-{index:03d}" for index in range(1, 93)],
        "base proof-cell IDs/order drifted",
    )
    actual: dict[tuple[str, str], str] = {}
    for old, new in zip(old_rows, new_rows):
        require(old["cell_id"] == new["cell_id"], "R4/R5 cell alignment drifted")
        for field in old:
            if old[field] != new[field]:
                actual[(new["cell_id"], field)] = new[field]
    require(actual == EXPECTED_CORRECTIONS, f"R5 correction set drifted: {actual}")
    by_cell = {row["cell_id"]: row for row in new_rows}
    require(by_cell["P1D4C-064"]["expected_restart_disposition"] == "P1d3PreAckPending", "C1 regressed")
    require(by_cell["P1D4C-092"]["expected_restart_disposition"] == "P1d3PreAckPending", "C1 equivalent owner regressed")
    require(by_cell["P1D4C-039"]["pel_after"] == "exact_source_pending_1", "S05 source released early")
    require(by_cell["P1D4C-039"]["xack_expectation"] == "forbidden_until_generated_market_s_truth", "early XACK reopened")
    require(by_cell["P1D4C-040"]["pel_before"] == "exact_source_absent_after_parsed_xack_1", "terminal response-loss proof weakened")

    gm_rows = read_rows(gm_matrix)
    require(len(gm_rows) == 10, "generated-Market proof-cell count drifted")
    require([row["cell_id"] for row in gm_rows] == [f"P1D4GM-{index:03d}" for index in range(1, 11)], "generated-Market IDs/order drifted")
    require([row["frontier_id"] for row in gm_rows] == [f"GM{index:02d}" for index in range(10)], "generated-Market frontiers drifted")
    require(all(row["parent_scenario_id"] == "S05" for row in gm_rows), "generated-Market parent scope drifted")
    require(all(row["source_pel_before"] == "exact_source_pending_1" and row["source_pel_after"] == "exact_source_pending_1" for row in gm_rows), "source retention weakened")
    require(all(row["xack_delta"] == "0" for row in gm_rows[:-1]), "early generated-Market XACK opened")
    require(gm_rows[-1]["xack_delta"] == "+1_exact", "S_truth XACK authority missing")
    require(gm_rows[-1]["expected_restart_disposition"] == "P1d4GeneratedMarketTruthCommitted", "terminal owner drifted")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in old_rows + gm_rows), "duplicate/conflict coverage weakened")

    amendment_rows = read_rows(amendment)
    require(len(amendment_rows) == 24, "R5 amendment row count drifted")
    require([row["id"] for row in amendment_rows] == [f"P1D4R5-{index:03d}" for index in range(1, 25)], "R5 amendment IDs/order drifted")
    require(all(row["status"] == "REQUIRED" for row in amendment_rows), "R5 requirement weakened")

    require(evidence.get("stage") == "Stage 8B-P1-d4 crash/replay design R5 correction", "evidence stage drifted")
    require(evidence.get("status") == "DESIGN_R5_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("design_parent_ref") == R4_REF, "evidence parent drifted")
    require(evidence.get("accepted_r3_design_ref") == R3_REF, "evidence R3 lineage drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == P1D3_REF, "evidence P1-d3 lineage drifted")
    for key, expected in (
        ("design_r5_sha256", R5_DESIGN_SHA256),
        ("source_discovery_r5_sha256", R5_DISCOVERY_SHA256),
        ("historical_r4_matrix_sha256", R4_MATRIX_SHA256),
        ("scenario_frontier_matrix_v5_sha256", R5_MATRIX_SHA256),
        ("generated_market_matrix_sha256", GM_MATRIX_SHA256),
        ("acceptance_amendment_sha256", AMENDMENT_SHA256),
    ):
        require(evidence.get(key) == expected, f"evidence hash drifted: {key}")
    require(evidence.get("base_matrix_rows") == 92 and evidence.get("generated_market_matrix_rows") == 10 and evidence.get("active_positive_cells") == 102, "evidence proof inventory drifted")
    require(evidence.get("acceptance_amendment_rows") == 24, "evidence amendment inventory drifted")
    require(evidence.get("r5_base_matrix_changed_fields") == 13, "evidence delta inventory drifted")
    require(evidence.get("targeted_negative_cases") == 32 and evidence.get("total_contract_negative_cases") == 160, "evidence negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False and evidence.get("source_wip_included") is False, "design/source boundary opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")
    composition = evidence.get("composition_contract")
    require(isinstance(composition, dict), "composition contract missing")
    require(composition.get("option") == "A_RETAIN_SINGLE_SOURCE", "Option A binding drifted")
    require(composition.get("source_m10_count") == 1 and composition.get("source_retained_through") == "combined_s_truth", "source retention drifted")
    require(composition.get("source_absent_before_s_truth_allowed") is False, "source-absent path opened")
    require(composition.get("raw_stage5c_capability_embedded") is False, "raw Stage5C authority opened")
    require(composition.get("new_journal_record_authorized") is False, "new journal record opened")
    require(composition.get("owners") == [
        "P1d4GeneratedMarketPrepublicationPending",
        "P1d4GeneratedMarketPreAckPending",
        "P1d4GeneratedMarketAckCommitted",
        "P1d4GeneratedMarketTruthCommitted",
    ], "owner inventory drifted")

    for value, label in ((status, "status"), (roadmap, "roadmap")):
        for token in (R4_REF, "P1-d4 R5 design-only correction", "one later-bar M10", "102", "paused pending independent R5 acceptance", "Redis DB0/VPS"):
            require(token in value, f"{label} missing token: {token}")


def read_inputs() -> tuple[str, str, str, str, str, str, str, dict[str, object], str, str]:
    return (
        GENERAL.read_text(encoding="utf-8"),
        R4_MATRIX.read_text(encoding="utf-8"),
        R5_MATRIX.read_text(encoding="utf-8"),
        GM_MATRIX.read_text(encoding="utf-8"),
        AMENDMENT.read_text(encoding="utf-8"),
        DESIGN.read_text(encoding="utf-8"),
        DISCOVERY.read_text(encoding="utf-8"),
        json.loads(EVIDENCE.read_text(encoding="utf-8")),
        STATUS.read_text(encoding="utf-8"),
        ROADMAP.read_text(encoding="utf-8"),
    )


def main() -> None:
    try:
        content_only = sys.argv[1:] == ["--content-only"]
        require(not sys.argv[1:] or content_only, "usage: stage8b_p1d4_r5_design_check.py [--content-only]")
        if not content_only:
            require(committed_changed_files() == EXPECTED_CHANGED, "R5 committed path scope drifted")
        validate(*read_inputs())
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r5-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r5-design-scope files=13 base_cells=92 generated_market_cells=10 active_cells=102 amendments=24 design_only=true")


if __name__ == "__main__":
    main()
