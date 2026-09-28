#!/usr/bin/env python3
"""Fail-closed source/scope checker for Stage 8B-P1-d4."""

from __future__ import annotations

import csv
import hashlib
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
    "docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv",
    "docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv",
    "docs/stage-8/stage8b-p1d4-marker-witness-canonical-fixtures-v1.json",
    "docs/stage-8/stage8b-p1d4-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-source-evidence.json",
    "scripts/make_stage8b_p1d4_source_handoff.py",
    "scripts/stage8b_p1d4_crash_evidence_check.py",
    "scripts/stage8b_p1d4_crash_evidence_negative_harness.py",
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
        "base_registry": "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv",
        "matrix": "docs/stage-8/stage8b-p1d4-source-acceptance-matrix.csv",
        "base_oracle": "docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv",
        "base_operational_oracle": "docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv",
        "marker_witness_fixtures": "docs/stage-8/stage8b-p1d4-marker-witness-canonical-fixtures-v1.json",
        "evidence": "docs/stage-8/stage8b-p1d4-source-evidence.json",
        "status": "docs/current-status.md",
        "roadmap": "docs/roadmap.md",
        "crash_evidence_checker": "scripts/stage8b_p1d4_crash_evidence_check.py",
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
    crash_evidence_checker = content["crash_evidence_checker"]

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
        "stage8b_p1d4_test_observe_xack_reply",
        "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1",
        ".p1d4-xack-witness-",
        "create_new(true)",
        ".open(&witness)",
        "write_all(&mut file, bytes.as_bytes())",
        "file.sync_all()",
        "std::fs::rename(&temporary, &witness)",
        "let reread = std::fs::read(&witness)",
        "moex.stage8b.p1d4.crash-marker.v1",
    ):
        require(token in service, f"service routing invariant missing: {token}")
    require(
        "moex.stage8b.p1d4.crash-marker.v2" not in service,
        "service retained unaccepted CrashMarkerV2",
    )
    require(
        service.find("if let Some(package_state) = p1d4_package_state")
        < service.find("let stage8b_p1d3_phase"),
        "P1-d4 direct routing no longer precedes ordinary P1-d3/P1-d2 routing",
    )
    witness_emitter = section(
        service,
        "pub(crate) fn stage8b_p1d4_test_observe_xack_reply(",
        "pub(crate) fn stage8b_p1_test_crash_barrier(",
    )
    witness_steps = (
        'assert_eq!(reply, 1, "P1-d4 F16 witness requires parsed XACK reply 1")',
        ".open(&witness)",
        ".open(&temporary)",
        "std::io::Write::write_all(&mut file, bytes.as_bytes())",
        "file.sync_all()",
        "std::fs::rename(&temporary, &witness)",
        ".and_then(|directory| directory.sync_all())",
        "let reread = std::fs::read(&witness)",
    )
    witness_positions = [witness_emitter.find(step) for step in witness_steps]
    require(
        all(position >= 0 for position in witness_positions)
        and witness_positions == sorted(witness_positions)
        and len(set(witness_positions)) == len(witness_positions),
        "F16 witness create-once/fsync/rename/reread order drifted",
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

    acknowledge = section(
        redis_source,
        "async fn acknowledge_exact(",
        "async fn publish_exact_command(",
    )
    require(
        acknowledge.find(".query_async(&mut self.connection)")
        < acknowledge.find("stage8b_p1d4_test_observe_xack_reply"),
        "F16 witness hook must run after the parsed Redis XACK reply",
    )

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
        "struct P1d4RegistryCell",
        "struct P1d4GeneratedMarketRegistryCell",
        "p1d4_exhaustive_crash_replay_evidence_two_clean_runs",
        "p1d4_collect_evidence_run(1)",
        "p1d4_collect_evidence_run(2)",
        "P1D4_EVIDENCE_DIGEST",
        '"GM08" | "GM09"',
        "STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER",
        "assert_eq!(continuation.sequence_after, Some(pair)",
        'cell.frontier_id == "GM07"',
        "immediate_xack_attempts",
        "source_disposition_before_continuation",
        "P1d4ObservedEffectEvent",
        "p1d4_generated_expected_effect_events",
        "p1d4_generated_effect_scope_terminal",
        '"observed_effect_events".into()',
        '"package_commit_history".into()',
        '"truth_replacement_commits".into()',
        '"pre_kill_audit_payload".into()',
        '"post_restart_audit_payload".into()',
        '"final_audit_payload".into()',
        "p1d4_assert_base_evidence_oracle",
        "p1d4_assert_base_operational_evidence_oracle",
        "p1d4_redis_group_frontier_v1",
        "stage8b-p1d4-base-operational-evidence-oracle-v1.csv",
        '"group_frontier_v1"',
        '"group_frontier_sha256"',
        '"pre_kill_xack_reply"',
        '"pre_kill_xack_reply_witness_v1"',
        '"pre_kill_xack_reply_witness_sha256"',
        '"pre_kill_xack_witness_order"',
        "stage8b_p1d4_test_observe_xack_reply",
        "moex.stage8b.p1d4.crash-marker.v1",
        "moex.stage8b.p1d4.crash-marker.normalized.v1",
        "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1",
        "moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v1",
        "after_integer_1_before_crash_marker",
        "p1d4_assert_package_commit_history",
        "stage8b-p1d4-base-evidence-oracle-v1.csv",
        "journal_record_index",
        "second_clean_run_same_final_audit",
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

    for token in (
        "FIXED_CELL_FIELDS",
        "duplicate JSON key",
        "floating-point JSON value is forbidden",
        "len(cells) == 105",
        "registry field drift",
        'process["exit_signal"] == "signal:9"',
        'process["reaped"] is True',
        "semantic_digest(run)",
        "validate_sequence_audit",
        "FINAL_ALLOCATION_KINDS",
        "GENERATED_EFFECT_EVENTS",
        "GENERATED_EFFECT_SCOPE_TERMINAL",
        'cell["observed_effect_events"]',
        'cell["package_after"]["write_generation"]',
        'cell["truth_bearing_outcomes"] == sum',
        'cell["provider_attempts"] == expected_effect',
        'cell["schedule_issue_attempts"]',
        'cell["s_ack_commits"]',
        'cell["s_truth_commits"]',
        'cell["sequence_before"] == cell["sequence_after"]',
        'expected["frontier_id"] == "GM07"',
        'expected["xack_delta"]',
        'expected["final_source_disposition"]',
        'first["final_audit_sha256"] == second["final_audit_sha256"]',
        "BASE_ORACLE",
        "load_base_oracle",
        "validate_base_oracle",
        "BASE_OPERATIONAL_ORACLE",
        "load_base_operational_oracle",
        "validate_base_operational_oracle",
        "validate_group_frontier",
        'redis["pre_kill_xack_reply"] == oracle["pre_kill_xack_reply"]',
        "validate_marker_and_witness",
        "MARKER_NORMALIZED_DOMAIN",
        "WITNESS_DIGEST_DOMAIN",
        'hashlib.sha256(raw_marker).hexdigest() == filesystem["raw_marker_sha256"]',
        'redis["xack_reply"] == "integer:0"',
        "validate_package_history",
        "validate_audit_payload",
        'cell["truth_replacement_commits"]',
        'hashlib.sha256(canonical_bytes(payload)).hexdigest()',
    ):
        require(token in crash_evidence_checker, f"crash evidence checker invariant missing: {token}")
    require(
        "moex.stage8b.p1d4.crash-marker.v2" not in stage6
        and "moex.stage8b.p1d4.crash-marker.v2" not in redis_source,
        "source retained unaccepted CrashMarkerV2",
    )

    oracle_rows = list(csv.DictReader(content["base_oracle"].splitlines()))
    base_registry_rows = list(csv.DictReader(content["base_registry"].splitlines()))
    require(len(oracle_rows) == 92, "base evidence oracle must contain 92 rows")
    require(
        len({row["cell_id"] for row in oracle_rows}) == 92,
        "base evidence oracle cell identities must be unique",
    )
    require(
        {row["cell_id"] for row in oracle_rows}
        == {row["cell_id"] for row in base_registry_rows},
        "base evidence oracle identities drifted from the accepted registry",
    )
    operational_oracle_rows = list(
        csv.DictReader(content["base_operational_oracle"].splitlines())
    )
    operational_oracle_fields = [
        "cell_id",
        "callback_before",
        "callback_after",
        "command_publications",
        "immediate_xack_attempts",
        "xack_reply",
        "xack_disposition",
        "source_disposition_before_continuation",
        "source_stream",
        "source_group",
        "source_m10_redis_id",
        "before_last_delivered_id",
        "before_pending",
        "post_restart_last_delivered_id",
        "post_restart_pending",
        "final_last_delivered_id",
        "final_pending",
        "pre_kill_xack_reply",
    ]
    require(
        list(operational_oracle_rows[0]) == operational_oracle_fields,
        "base operational evidence oracle schema drifted",
    )
    require(
        len(operational_oracle_rows) == 92,
        "base operational evidence oracle must contain 92 rows",
    )
    require(
        len({row["cell_id"] for row in operational_oracle_rows}) == 92,
        "base operational evidence oracle cell identities must be unique",
    )
    require(
        {row["cell_id"] for row in operational_oracle_rows}
        == {row["cell_id"] for row in base_registry_rows},
        "base operational evidence oracle identities drifted from accepted registry",
    )
    registry_by_cell = {row["cell_id"]: row for row in base_registry_rows}
    for row in operational_oracle_rows:
        expected = (
            "integer:1"
            if registry_by_cell[row["cell_id"]]["frontier_id"] == "F16"
            else "not_observed"
        )
        require(
            row["pre_kill_xack_reply"] == expected,
            f"{row['cell_id']}: pre-kill XACK oracle drifted from accepted frontier",
        )

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

    fixtures = json.loads(content["marker_witness_fixtures"])
    require(fixtures["schema_version"] == 1, "marker/witness fixture schema")
    marker_fixture = fixtures["marker"]
    marker_bytes = marker_fixture["canonical_json"].encode()
    require(marker_fixture["contract"] == "Stage8bP1d4CrashMarkerV1", "marker contract")
    require(marker_fixture["field_count"] == 8, "marker field count")
    require(
        hashlib.sha256(marker_bytes).hexdigest() == marker_fixture["raw_sha256"],
        "marker fixture raw hash",
    )
    normalized_bytes = marker_fixture["normalized_canonical_json"].encode()
    require(
        hashlib.sha256(
            b"moex.stage8b.p1d4.crash-marker.normalized.v1\0"
            + len(normalized_bytes).to_bytes(8, "big")
            + normalized_bytes
        ).hexdigest()
        == marker_fixture["normalized_sha256"],
        "marker fixture normalized hash",
    )
    witness_fixture = fixtures["witness"]
    witness_bytes = witness_fixture["canonical_json"].encode()
    require(
        witness_fixture["contract"] == "Stage8bP1d4PreKillXackReplyWitnessV1",
        "witness contract",
    )
    require(witness_fixture["field_count"] == 12, "witness field count")
    require(
        hashlib.sha256(
            b"moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v1\0"
            + len(witness_bytes).to_bytes(8, "big")
            + witness_bytes
        ).hexdigest()
        == witness_fixture["sha256"],
        "witness fixture hash",
    )


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
