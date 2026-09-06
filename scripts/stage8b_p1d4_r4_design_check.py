#!/usr/bin/env python3
"""Fail-closed checker for the narrow P1-d4 R4 design correction."""

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
GENERAL_MATRIX_SHA256 = "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de"
R3_DESIGN_SHA256 = "d4c844498b47b53a09fc916e7a110ce856725ad421e4dee1c4b641e85dc70e7a"
R3_EVIDENCE_SHA256 = "f5c4caf8420d3aed9c929c4a4becf230425c439af3649e40fc2531233e48c901"
R3_MATRIX_SHA256 = "8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc"
R4_DESIGN_SHA256 = "7e6c5a65eab957b92b61a58a4b6180c050f95305c8f35568e8e35140780b9b39"
R4_DISCOVERY_SHA256 = "37c0ca38f0098880beabbb343725d5010da9459b2e21041c26e0196a2c1c21a6"
R4_MATRIX_SHA256 = "74fc128b06d188942008449f05977d8eb46630c3ccfc364659a75d77d0e5810f"

R3_DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md"
R4_DESIGN = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-design-r4.md"
DISCOVERY = ROOT / "docs/stage-8/stage8b-p1d4-source-discovery-r4.md"
GENERAL_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv"
R3_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv"
R4_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv"
R3_EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"
R4_EVIDENCE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r4.json"
STATUS = ROOT / "docs/current-status.md"
ROADMAP = ROOT / "docs/roadmap.md"

EXPECTED_CHANGED = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r4.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r4.json",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r4.md",
    "scripts/make_stage8b_p1d4_r4_design_handoff.py",
    "scripts/stage8b_p1d4_r4_design_check.py",
    "scripts/stage8b_p1d4_r4_design_gate.sh",
    "scripts/stage8b_p1d4_r4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r4_design_negative_harness.py",
}

EXPECTED_CORRECTIONS = {
    ("P1D4C-039", "only_legal_continuation"): (
        "confirm_exact_existing_publication_then_exact_bar_xack_"
        "generated_command_lifecycle_independent"
    ),
    ("P1D4C-040", "expected_restart_disposition"): "P1SemanticPrepublicationReady",
    ("P1D4C-040", "only_legal_continuation"): (
        "prove_bar_pel_absent_and_group_frontier_then_continue_exact_existing_"
        "command_without_callback_replay"
    ),
    ("P1D4C-064", "expected_restart_disposition"): "P1d3PreAckPending",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", R3_REF], cwd=ROOT, check=True,
        text=True, capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
        check=True, text=True, capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def validate(
    r3_design: str,
    r4_design: str,
    discovery: str,
    general_matrix: str,
    r3_matrix: str,
    r4_matrix: str,
    r3_evidence: str,
    evidence: dict[str, object],
    status: str,
    roadmap: str,
) -> None:
    require(sha256_text(r3_design) == R3_DESIGN_SHA256, "accepted R3 design drifted")
    require(sha256_text(r3_evidence) == R3_EVIDENCE_SHA256, "accepted R3 evidence drifted")
    require(sha256_text(general_matrix) == GENERAL_MATRIX_SHA256, "general matrix drifted")
    require(sha256_text(r3_matrix) == R3_MATRIX_SHA256, "accepted R3 registry drifted")
    require(sha256_text(r4_design) == R4_DESIGN_SHA256, "R4 design byte hash drifted")
    require(sha256_text(discovery) == R4_DISCOVERY_SHA256, "R4 discovery byte hash drifted")
    require(sha256_text(r4_matrix) == R4_MATRIX_SHA256, "R4 registry byte hash drifted")

    for token in (
        "Status: R4 design-only review candidate",
        R3_REF,
        P1D3_REF,
        "changes exactly three",
        "proof cells and four fields",
        "P1D4C-064 S09/F09 and P1D4C-092 S09/F04",
        "have the same authenticated durable",
        "An ACK that existed only in memory cannot participate in",
        "P1D4C-064.expected_restart_disposition",
        "P1d3CancelContinuationPending -> P1d3PreAckPending",
        "belongs to its own command M10 source",
        "P1D4C-040 expected_restart_disposition",
        "P1d3TruthCommitted -> P1SemanticPrepublicationReady",
        "reading a crash marker or test environment from production recovery",
        "introducing a combined P1-d3-plus-Market schema or owner",
        "P1-e remains unauthorized",
    ):
        require(token in r4_design, f"missing R4 invariant: {token}")

    for token in (
        "S09/F04 and S09/F09 are durably indistinguishable",
        "The marker is test evidence",
        "incorrectly couple two source lifecycles",
        "Durable(Runtime(RestartRuntimeRequired))",
        "implementation worktree itself is deliberately excluded",
    ):
        require(token in discovery, f"missing discovery evidence: {token}")

    general_rows = list(csv.DictReader(general_matrix.splitlines()))
    require(len(general_rows) == 88, "general row count drifted")
    require(
        [row["id"] for row in general_rows] == [f"P1D4D-{index:03d}" for index in range(1, 89)],
        "general row IDs/order drifted",
    )

    old_rows = list(csv.DictReader(r3_matrix.splitlines()))
    new_rows = list(csv.DictReader(r4_matrix.splitlines()))
    require(len(old_rows) == 92 and len(new_rows) == 92, "proof-cell count drifted")
    require(list(old_rows[0]) == list(new_rows[0]), "proof-cell fields drifted")
    require(
        [row["cell_id"] for row in new_rows] == [f"P1D4C-{index:03d}" for index in range(1, 93)],
        "proof-cell IDs/order drifted",
    )
    require({row["scenario_id"] for row in new_rows} == {f"S{index:02d}" for index in range(1, 12)}, "scenario set drifted")
    require({row["frontier_id"] for row in new_rows} == {f"F{index:02d}" for index in range(21)}, "frontier set drifted")

    actual_changes: dict[tuple[str, str], str] = {}
    for old, new in zip(old_rows, new_rows):
        require(old["cell_id"] == new["cell_id"], "v3/v4 cell alignment drifted")
        for field in old:
            if old[field] != new[field]:
                actual_changes[(new["cell_id"], field)] = new[field]
    require(actual_changes == EXPECTED_CORRECTIONS, f"R4 correction set drifted: {actual_changes}")

    by_cell = {row["cell_id"]: row for row in new_rows}
    require(by_cell["P1D4C-063"]["expected_restart_disposition"] == "P1d3CancelContinuationPending", "S09 target-terminal owner collapsed")
    require(by_cell["P1D4C-064"]["expected_restart_disposition"] == by_cell["P1D4C-092"]["expected_restart_disposition"] == "P1d3PreAckPending", "durable-equivalent S09 owners diverged")
    require(by_cell["P1D4C-040"]["pel_before"] == "exact_source_absent_after_parsed_xack_1", "S05 F16 PEL proof weakened")
    require(by_cell["P1D4C-040"]["xack_expectation"] == "parsed_reply_1_then_AlreadyAcknowledged", "S05 F16 XACK proof weakened")
    require(all(row["duplicate_variant_required"] == "true" and row["conflict_variant_required"] == "true" for row in new_rows), "duplicate/conflict coverage weakened")

    require(evidence.get("stage") == "Stage 8B-P1-d4 crash/replay design R4 correction", "evidence stage drifted")
    require(evidence.get("status") == "DESIGN_R4_REVIEW_CANDIDATE", "evidence status drifted")
    require(evidence.get("accepted_r3_design_ref") == R3_REF, "R3 lineage drifted")
    require(evidence.get("accepted_p1d3_closure_ref") == P1D3_REF, "P1-d3 lineage drifted")
    require(evidence.get("acceptance_rows") == 88 and evidence.get("scenario_frontier_matrix_rows") == 92, "evidence inventory drifted")
    require(evidence.get("design_r3_sha256") == R3_DESIGN_SHA256, "evidence R3 hash drifted")
    require(evidence.get("design_r4_sha256") == R4_DESIGN_SHA256, "evidence R4 hash drifted")
    require(evidence.get("discovery_r4_sha256") == R4_DISCOVERY_SHA256, "evidence discovery hash drifted")
    require(evidence.get("historical_r3_matrix_sha256") == R3_MATRIX_SHA256, "evidence R3 matrix hash drifted")
    require(evidence.get("scenario_frontier_matrix_v4_sha256") == R4_MATRIX_SHA256, "evidence R4 matrix hash drifted")
    require(evidence.get("inherited_negative_cases") == 128, "inherited negative inventory drifted")
    require(evidence.get("targeted_negative_cases") == 18, "targeted negative inventory drifted")
    require(evidence.get("total_negative_contract_cases") == 146, "aggregate negative inventory drifted")
    require(evidence.get("design_only") is True and evidence.get("implementation_authorized") is False, "design boundary opened")
    require(evidence.get("source_wip_included") is False, "source WIP entered design commit")
    require(evidence.get("new_persistence_schema_authorized") is False, "new persistence schema opened")
    require(all(value is False for value in evidence.get("closed_surfaces", {}).values()), "closed surface opened")

    correction = evidence.get("correction_contract")
    require(isinstance(correction, dict), "correction contract missing")
    require(correction.get("exact_changed_fields") == [f"{cell}.{field}" for cell, field in EXPECTED_CORRECTIONS], "evidence correction set drifted")
    durable = correction.get("durable_equivalence")
    require(isinstance(durable, dict), "durable equivalence contract missing")
    require(durable.get("cells") == ["P1D4C-064", "P1D4C-092"], "durable equivalence cells drifted")
    require(durable.get("restart_disposition") == "P1d3PreAckPending", "durable equivalence owner drifted")
    require(durable.get("forbidden_restart_inputs") == ["crash_marker", "test_environment", "volatile_ack_state"], "forbidden restart input set drifted")
    independent = correction.get("source_independence")
    require(isinstance(independent, dict), "source independence contract missing")
    require(independent.get("generated_command_disposition") == "P1SemanticPrepublicationReady", "generated command owner drifted")
    require(independent.get("generated_command_lifecycle") == "independent_accepted_p1d2_market_lifecycle", "generated command lifecycle coupled")
    require(independent.get("callback_replay_allowed") is False and independent.get("second_publication_allowed") is False, "callback/publication replay opened")
    require(independent.get("new_combined_schema_allowed") is False, "combined schema opened")

    for text, label in ((status, "status"), (roadmap, "roadmap")):
        for token in (R3_REF, "P1-d4 R4 design-only correction", "implementation is paused", "Redis DB0/VPS"):
            require(token in text, f"{label} missing R4 token: {token}")


def read_inputs() -> tuple[str, str, str, str, str, str, str, dict[str, object], str, str]:
    return (
        R3_DESIGN.read_text(encoding="utf-8"),
        R4_DESIGN.read_text(encoding="utf-8"),
        DISCOVERY.read_text(encoding="utf-8"),
        GENERAL_MATRIX.read_text(encoding="utf-8"),
        R3_MATRIX.read_text(encoding="utf-8"),
        R4_MATRIX.read_text(encoding="utf-8"),
        R3_EVIDENCE.read_text(encoding="utf-8"),
        json.loads(R4_EVIDENCE.read_text(encoding="utf-8")),
        STATUS.read_text(encoding="utf-8"),
        ROADMAP.read_text(encoding="utf-8"),
    )


def main() -> None:
    try:
        content_only = sys.argv[1:] == ["--content-only"]
        require(not sys.argv[1:] or content_only, "usage: stage8b_p1d4_r4_design_check.py [--content-only]")
        if not content_only:
            require(changed_files() == EXPECTED_CHANGED, f"R4 design changed path drift: {sorted(changed_files())}")
        validate(*read_inputs())
    except (OSError, json.JSONDecodeError, subprocess.CalledProcessError, CheckFailure) as error:
        print(f"stage8b-p1d4-r4-design-check: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-r4-design-scope files=11 general_rows=88 cells=92 corrections=4 design_only=true")


if __name__ == "__main__":
    main()
