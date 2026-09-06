#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d4."""

from __future__ import annotations

import csv
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
ACCEPTED_DESIGN = "1a1ea05775f1d15b86fcc3495ad6863b851e9212"
ACCEPTED_P1D3 = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d4_generated_market.rs",
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-generated-market-source.md",
    "docs/stage-8/stage8b-p1d4-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-source-evidence.json",
    "scripts/make_stage8b_p1d4_source_handoff.py",
    "scripts/stage8b_p1d4_source_check.py",
    "scripts/stage8b_p1d4_source_gate.sh",
    "scripts/stage8b_p1d4_source_handoff_safety_check.py",
    "scripts/stage8b_p1d4_source_negative_harness.py",
}


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def changed_files() -> set[str]:
    tracked = subprocess.run(
        ["git", "diff", "--name-only", ACCEPTED_DESIGN],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    untracked = subprocess.run(
        ["git", "ls-files", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.splitlines()
    return set(tracked) | set(untracked)


def load_content(root: pathlib.Path = ROOT) -> dict[str, str]:
    paths = {
        "domain": "crates/strategy-runtime-core/src/stage8b_p1d4_generated_market.rs",
        "stage5g": "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
        "stage6": "crates/strategy-runtime-core/src/stage6d_live_core.rs",
        "service": "crates/runtime-durable-service/src/recovery.rs",
        "redis": "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
        "doc": "docs/stage-8/stage8b-p1d4-generated-market-source.md",
        "matrix": "docs/stage-8/stage8b-p1d4-source-acceptance-matrix.csv",
        "evidence": "docs/stage-8/stage8b-p1d4-source-evidence.json",
        "status": "docs/current-status.md",
        "roadmap": "docs/roadmap.md",
    }
    return {key: (root / path).read_text(encoding="utf-8") for key, path in paths.items()}


def section(text: str, start: str, end: str) -> str:
    begin = text.find(start)
    require(begin >= 0, f"section start missing: {start}")
    finish = text.find(end, begin + len(start))
    require(finish >= 0, f"section end missing: {end}")
    return text[begin:finish]


def validate_content(content: dict[str, str]) -> None:
    domain = content["domain"]
    stage5g = content["stage5g"]
    stage6 = content["stage6"]
    service = content["service"]
    redis_source = content["redis"]
    document = content["doc"]

    for token in (
        'RESERVATION_DOMAIN: &str = "moex.stage8b.p1d4.command-publication-reservation.v1"',
        'BINDING_DOMAIN: &str = "moex.stage8b.p1d4.command-publication-binding.v1"',
        'COMPOSITION_DOMAIN: &str = "moex.stage8b.p1d4.generated-market-composition.v1"',
        "pub enum Stage8bP1d4GeneratedMarketPackageState",
        "Prepublication {",
        "AckCommitted {",
        "TruthCommitted {",
        "Stage8bP1d4CommandPublicationReservationV1",
        "Stage8bP1d4CommandPublicationBindingV1",
        "immediate_successor(predecessor)? != reserved",
        "self.prepublication_package_generation.to_be_bytes()",
        "self.prepublication_seal_generation.to_be_bytes()",
        "seal_generation_above_json_safe_integer_remains_distinct",
    ):
        require(token in domain, f"domain invariant missing: {token}")

    for token in (
        "p1d4_generated_market:",
        "Stage5gCleanRestartSource::P1d4",
        "stage8b_p1d4_generated_market_package_state",
    ):
        require(token in stage5g, f"Stage5G composition invariant missing: {token}")

    for token in (
        "authenticate_stage8b_p1d4_restart_package_state",
        "apply_stage8b_p1d4_publication_reservation",
        "apply_stage8b_p1d4_ack_transition",
        "apply_stage8b_p1d4_truth_transition",
        "classify_stage8b_p1d4_generated_market_journal_ahead_candidate",
    ):
        require(token in stage6, f"Stage6 invariant missing: {token}")

    for token in (
        "Stage8bP1d4GeneratedMarketPrepublicationOwner",
        "Stage8bP1d4GeneratedMarketDispatchPendingOwner",
        "Stage8bP1d4GeneratedMarketOrderPendingOwner",
        "Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner",
        "Stage8bP1d4GeneratedMarketPreAckPendingOwner",
        "Stage8bP1d4GeneratedMarketAckCommittedOwner",
        "Stage8bP1d4GeneratedMarketTruthCommittedOwner",
        "if let Some(package_state) = p1d4_package_state",
        "restart_stage8b_p1d4_generated_market_direct",
        "P1d3PreAckPending(Box::new(pending))",
        "F04 and F09 are deliberately the same durable-equivalence class",
    ):
        require(token in service, f"service routing invariant missing: {token}")
    require(
        service.find("if let Some(package_state) = p1d4_package_state")
        < service.find("let stage8b_p1d3_phase"),
        "P1-d4 direct routing no longer precedes ordinary P1-d3/P1-d2 routing",
    )

    publication = section(
        redis_source,
        "const P1D4_COMMAND_PUBLICATION_LUA",
        "const P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA",
    )
    for token in (
        "last-generated-id",
        "local reserved_id = ARGV[6]",
        "local exact_reserved = redis.call('XRANGE', command_stream, reserved_id, reserved_id)",
        "if last_generated_id(command_stream) ~= predecessor_id",
        "redis.call('XADD', command_stream, reserved_id, 'payload', envelope_payload)",
        "redis.call('SET', marker_key, marker_payload)",
    ):
        require(token in publication, f"publication invariant missing: {token}")
    require("XADD', command_stream, '*" not in publication, "dynamic XADD ID reopened")
    require("tonumber" not in publication, "P1-d4 publication uses lossy Lua number")

    revalidate = section(
        redis_source,
        "const P1D4_COMMAND_PUBLICATION_REVALIDATE_LUA",
        "#[derive(Debug, Clone, PartialEq, Eq)]",
    )
    for token in (
        "redis.call('GET', marker_key) ~= marker_payload",
        "redis.call('XRANGE', command_stream, reserved_id, reserved_id)",
        "STAGE8B_P1D4_SOURCE_NOT_PENDING",
    ):
        require(token in revalidate, f"revalidation invariant missing: {token}")

    for token in (
        "prepublication_package_generation: String",
        "prepublication_seal_generation: String",
        "binding.prepublication_seal_generation().to_string()",
        "p1d4_generated_market_write_and_seal_generations_are_independent",
        "assert_eq!(binding.prepublication_seal_generation(), 100)",
        "p1d4_publication_marker_preserves_high_u64_generation_exactly",
        "9_007_199_254_740_993",
        "p1d4_exact_registry_cells_sigkill_with_duplicate_and_conflict_variants",
        "p1d4_generated_market_registry_extends_base_to_105_with_variants",
        "matches!(&conflict, Err(_) | Ok(Stage7bRestartOutcome::Blocked(_)))",
        "byte-identical duplicate restart drifted",
        "p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers",
        "p1d2_sequence_pair_allocation_crash_reconstructs_exact_ack_path",
        '"/../../docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"',
        '"/../../docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv"',
    ):
        require(token in redis_source, f"Redis/proof invariant missing: {token}")
    for frontier in range(13):
        require(f'"GM{frontier:02}"' in redis_source, f"GM{frontier:02} hook missing")

    for forbidden in ("reqwest::", "FINAM", "POST /", "DELETE /"):
        require(forbidden not in domain, f"domain opened forbidden transport: {forbidden}")
    for token in (
        "105 positive real subprocess/SIGKILL cells",
        "Operational Redis DB0/VPS",
        "FINAM POST/DELETE",
        "P1-e remain closed",
    ):
        require(token in document, f"source document invariant missing: {token}")

    matrix_rows = list(csv.DictReader(content["matrix"].splitlines()))
    require(len(matrix_rows) == 20, "source acceptance matrix must contain 20 rows")
    require(all(row["required"] == "true" for row in matrix_rows), "all source rows must be required")
    evidence = json.loads(content["evidence"])
    require(evidence["accepted_design_ref"] == ACCEPTED_DESIGN, "evidence design ref drift")
    require(evidence["accepted_business_source_ref"] == ACCEPTED_P1D3, "evidence P1-d3 ref drift")
    require(evidence["crash_proof"]["positive_sigkill_cells"] == 105, "evidence cell count drift")
    require(not evidence["next_stage_authorized"], "P1-e opened before review")
    require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
    require("R7" in content["status"] and "R7" in content["roadmap"], "status/roadmap R7 missing")


def main() -> None:
    try:
        if ROOT == pathlib.Path(__file__).resolve().parents[1]:
            actual = changed_files()
            require(actual == EXPECTED_CHANGED, f"changed file scope mismatch: {sorted(actual ^ EXPECTED_CHANGED)}")
        validate_content(load_content())
    except (CheckFailure, OSError, KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"FAIL stage8b-p1d4-source-check: {error}", file=sys.stderr)
        raise SystemExit(1)
    print("PASS stage8b-p1d4-source-check rows=20 cells=105 db0=false finam=false live=false p1e=false")


if __name__ == "__main__":
    main()
