#!/usr/bin/env python3
"""Fail-closed verifier for Stage 8B-P1-d4 two-run crash evidence."""

from __future__ import annotations

import copy
import csv
import hashlib
import json
import pathlib
import re
import sys
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[1]
BASE_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv"
GENERATED_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv"
RUN_FILES = (
    "stage8b-p1d4-crash-replay-run-1.json",
    "stage8b-p1d4-crash-replay-run-2.json",
)
DIGEST_FILE = "stage8b-p1d4-crash-replay-semantic-digest.txt"
DOMAIN = b"moex.stage8b.p1d4.crash-replay.semantic-evidence.v1\0"
ROOT_FIELDS = {
    "schema_version",
    "domain",
    "accepted_predecessor_ref",
    "source_ref",
    "source_tree",
    "matrix_sha256",
    "run_ordinal",
    "cells",
    "aggregate",
}
FIXED_CELL_FIELDS = {
    "passed",
    "process",
    "filesystem",
    "redis",
    "pre_kill_audit_sha256",
    "post_restart_audit_sha256",
    "final_audit_sha256",
    "sequence_before",
    "sequence_after",
    "sequence_audit_before",
    "sequence_audit_after",
    "package_before",
    "package_after",
    "callback_before",
    "callback_after",
    "provider_attempts",
    "schedule_issue_attempts",
    "durable_outcomes",
    "durable_truths",
    "s_ack_commits",
    "s_truth_commits",
    "s_ack_generations",
    "s_truth_generations",
    "observed_effect_events",
    "command_publications",
    "restart_disposition",
    "continuation_disposition",
    "final_disposition",
    "final_restart_disposition",
    "immediate_xack_attempts",
    "source_disposition_before_continuation",
    "duplicate_result",
    "conflict_result",
}
PROCESS_FIELDS = {"child_pid", "exit_code", "exit_signal", "reaped", "wall_duration_ms"}
FILESYSTEM_FIELDS = {"scratch_root", "raw_marker_sha256", "normalized_marker_sha256"}
REDIS_FIELDS = {"port", "pel_before", "pel_after", "group_frontier", "xack_reply", "xack_disposition"}
AGGREGATE = {
    "passed": True,
    "base_cells": 92,
    "generated_market_cells": 13,
    "positive_cells": 105,
    "duplicate_variants": 105,
    "conflict_variants": 105,
    "final_pel_zero_cells": 105,
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
SEQUENCE_AUDIT_FIELDS = {
    "lifecycle_sequence",
    "journal_lifecycle_sequences",
    "durable_sequence_pair",
    "pre_kill_sequence_pair",
    "allocations",
}
SEQUENCE_ALLOCATION_FIELDS = {
    "outcome_kind",
    "journal_record_index",
    "stage6_lifecycle_sequence",
    "sequence_allocation_frontier",
    "seq_ack",
    "seq_truth",
}
PACKAGE_FIELDS = {"write_generation", "p1d3_phase", "generated_market_phase"}
FINAL_ALLOCATION_KINDS = {
    "S01": ["initial_working"],
    "S02": ["initial_filled"],
    "S03": ["initial_expired"],
    "S04": ["initial_working"],
    "S05": ["initial_working"],
    "S06": ["initial_working", "later_filled"],
    "S07": ["initial_working", "later_expired"],
    "S08": ["initial_working", "cancel_canceled"],
    "S09": ["initial_working", "later_filled", "cancel_execution_observed"],
    "S10": ["initial_filled", "cancel_execution_observed"],
    "S11": ["initial_expired", "cancel_already_terminal_non_execution"],
}
GENERATED_EFFECT_EVENTS = {
    "GM00": ["generated_publication", "generated_schedule", "generated_dispatch", "generated_provider", "generated_order", "generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM01": ["generated_publication", "generated_schedule", "generated_dispatch", "generated_provider", "generated_order", "generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM02": ["generated_publication", "generated_schedule", "generated_dispatch", "generated_provider", "generated_order", "generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM03": ["generated_provider", "generated_order", "generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM04": ["generated_provider", "generated_order", "generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM05": ["generated_trade", "generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM06": ["generated_request_finalized", "generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM07": ["generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM08": ["generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM09": ["generated_s_ack", "generated_s_truth", "generated_xack"],
    "GM10": ["generated_s_truth", "generated_xack"],
    "GM11": ["generated_s_truth", "generated_xack"],
    "GM12": ["generated_xack"],
}
GENERATED_EFFECT_SCOPE_TERMINAL = {
    "GM00": "generated_publication",
    "GM01": "generated_schedule",
    "GM02": "generated_dispatch",
    "GM03": "generated_provider",
    "GM04": "generated_order",
    "GM05": "generated_trade",
    "GM06": "generated_request_finalized",
    "GM07": "generated_s_ack",
    "GM08": "generated_s_ack",
    "GM09": "generated_s_ack",
    "GM10": "generated_s_truth",
    "GM11": "generated_s_truth",
    "GM12": "generated_xack",
}


class EvidenceFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise EvidenceFailure(message)


def reject_float(value: str) -> Any:
    raise EvidenceFailure(f"floating-point JSON value is forbidden: {value}")


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json(path: pathlib.Path) -> dict[str, Any]:
    value = json.loads(
        path.read_text(encoding="utf-8"),
        parse_float=reject_float,
        object_pairs_hook=unique_object,
    )
    require(isinstance(value, dict), f"{path.name}: root must be an object")
    return value


def load_registry(path: pathlib.Path) -> tuple[list[str], dict[str, dict[str, Any]]]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        require(reader.fieldnames is not None, f"{path.name}: header missing")
        fields = list(reader.fieldnames)
        rows: dict[str, dict[str, Any]] = {}
        for raw in reader:
            cell_id = raw["cell_id"]
            require(cell_id not in rows, f"{path.name}: duplicate cell {cell_id}")
            row: dict[str, Any] = dict(raw)
            for boolean_field in ("duplicate_variant_required", "conflict_variant_required"):
                require(row[boolean_field] in ("true", "false"), f"{cell_id}: invalid boolean")
                row[boolean_field] = row[boolean_field] == "true"
            rows[cell_id] = row
    return fields, rows


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def semantic_view(value: dict[str, Any]) -> dict[str, Any]:
    normalized = copy.deepcopy(value)
    normalized["run_ordinal"] = 0
    for cell in normalized["cells"]:
        cell["process"]["child_pid"] = 0
        cell["process"]["wall_duration_ms"] = 0
        cell["filesystem"]["scratch_root"] = "<VOLATILE_PATH>"
        cell["filesystem"]["raw_marker_sha256"] = "<VOLATILE_MARKER_SHA256>"
        cell["redis"]["port"] = 0
    return normalized


def semantic_digest(value: dict[str, Any]) -> str:
    payload = canonical_bytes(semantic_view(value))
    return hashlib.sha256(DOMAIN + len(payload).to_bytes(8, "big") + payload).hexdigest()


def expected_pel(label: str) -> int:
    if label == "exact_source_pending_1":
        return 1
    if label in {
        "exact_source_absent_after_parsed_xack_1",
        "exact_source_absent",
        "no_new_source",
        "unchanged_no_new_source",
    }:
        return 0
    raise EvidenceFailure(f"unknown PEL label: {label}")


def validate_sequence_audit(cell_id: str, audit: Any) -> None:
    require(isinstance(audit, dict) and set(audit) == SEQUENCE_AUDIT_FIELDS, f"{cell_id}: sequence audit schema")
    lifecycle = audit["lifecycle_sequence"]
    journal = audit["journal_lifecycle_sequences"]
    require(type(lifecycle) is int and lifecycle >= 0, f"{cell_id}: lifecycle sequence")
    require(isinstance(journal, list), f"{cell_id}: journal sequence vector")
    require(journal and journal[0] == 1, f"{cell_id}: journal sequence must start at one")
    require(
        all(current == 1 or current == previous + 1 for previous, current in zip(journal, journal[1:])),
        f"{cell_id}: journal segments are not gap-free",
    )
    require(journal[-1] == lifecycle, f"{cell_id}: current journal frontier")
    for pair_field in ("durable_sequence_pair", "pre_kill_sequence_pair"):
        pair = audit[pair_field]
        require(
            pair is None
            or (
                isinstance(pair, list)
                and len(pair) == 2
                and all(type(value) is int and value > 0 for value in pair)
                and pair[1] == pair[0] + 1
            ),
            f"{cell_id}: malformed {pair_field}",
        )
    allocations = audit["allocations"]
    require(isinstance(allocations, list), f"{cell_id}: allocations")
    prior_business_frontier = None
    prior_record_index = None
    for index, allocation in enumerate(allocations):
        require(isinstance(allocation, dict) and set(allocation) == SEQUENCE_ALLOCATION_FIELDS, f"{cell_id}: allocation schema")
        kind = allocation["outcome_kind"]
        journal_record_index = allocation["journal_record_index"]
        lifecycle_sequence = allocation["stage6_lifecycle_sequence"]
        frontier = allocation["sequence_allocation_frontier"]
        seq_ack = allocation["seq_ack"]
        seq_truth = allocation["seq_truth"]
        require(type(journal_record_index) is int and journal_record_index >= 0, f"{cell_id}: V3 record index")
        require(
            prior_record_index is None or journal_record_index > prior_record_index,
            f"{cell_id}: V3 journal record order",
        )
        require(journal_record_index < len(journal), f"{cell_id}: V3 record index bounds")
        require(
            journal[journal_record_index] == lifecycle_sequence,
            f"{cell_id}: V3 lifecycle not bound to journal record",
        )
        require(type(frontier) is int and frontier >= 0, f"{cell_id}: allocation frontier")
        require(frontier == (2 if index == 0 else prior_business_frontier), f"{cell_id}: business sequence frontier")
        if kind in {"initial_working", "initial_filled", "initial_expired", "cancel_canceled"}:
            require(type(seq_ack) is int and type(seq_truth) is int, f"{cell_id}: pair types")
            require(seq_ack == frontier + 1 and seq_truth == seq_ack + 1, f"{cell_id}: exact ACK/truth pair")
            terminal = seq_truth
        elif kind in {"later_filled", "later_expired"}:
            require(seq_ack is None and type(seq_truth) is int, f"{cell_id}: autonomous truth shape")
            require(seq_truth == frontier + 1, f"{cell_id}: exact autonomous truth")
            terminal = seq_truth
        elif kind in {"cancel_execution_observed", "cancel_already_terminal_non_execution"}:
            require(type(seq_ack) is int and seq_truth is None, f"{cell_id}: recovered ACK shape")
            require(seq_ack == frontier + 1, f"{cell_id}: exact recovered ACK")
            terminal = seq_ack
        else:
            raise EvidenceFailure(f"{cell_id}: unknown allocation kind {kind}")
        prior_record_index = journal_record_index
        prior_business_frontier = terminal


def sequence_label(audit: dict[str, Any], generated: bool) -> str:
    pair = audit["pre_kill_sequence_pair"] or audit["durable_sequence_pair"]
    if pair is not None:
        return f"seq_ack={pair[0]};seq_truth={pair[1]}"
    allocations = audit["allocations"]
    if allocations and not generated:
        allocation = allocations[-1]
        if allocation["seq_ack"] is not None and allocation["seq_truth"] is not None:
            return f"seq_ack={allocation['seq_ack']};seq_truth={allocation['seq_truth']}"
        if allocation["seq_ack"] is not None:
            return f"seq_ack={allocation['seq_ack']}"
        return f"seq_truth={allocation['seq_truth']}"
    return f"lifecycle_sequence={audit['lifecycle_sequence']}"


def expected_effect(label: str, base: bool) -> int:
    if label.startswith("+1") or (base and label == "0_before_reissue"):
        return 1
    return 0


def validate_cell(
    cell: dict[str, Any],
    registry_fields: list[str],
    expected: dict[str, Any],
    generated: bool,
) -> None:
    cell_id = expected["cell_id"]
    require(set(cell) == set(registry_fields) | FIXED_CELL_FIELDS, f"{cell_id}: cell field set drift")
    for field in registry_fields:
        require(cell[field] == expected[field], f"{cell_id}: registry field drift: {field}")
    require(cell["passed"] is True, f"{cell_id}: not passed")
    require(cell["restart_disposition"] == expected["expected_restart_disposition"], f"{cell_id}: restart disposition")

    process = cell["process"]
    require(isinstance(process, dict) and set(process) == PROCESS_FIELDS, f"{cell_id}: process schema")
    require(type(process["child_pid"]) is int and process["child_pid"] > 0, f"{cell_id}: child PID")
    require(process["exit_code"] == "none", f"{cell_id}: SIGKILL must have no exit code")
    require(process["exit_signal"] == "signal:9", f"{cell_id}: signal must be SIGKILL")
    require(process["reaped"] is True, f"{cell_id}: child was not reaped")
    require(type(process["wall_duration_ms"]) is int and process["wall_duration_ms"] >= 0, f"{cell_id}: duration")

    filesystem = cell["filesystem"]
    require(isinstance(filesystem, dict) and set(filesystem) == FILESYSTEM_FIELDS, f"{cell_id}: filesystem schema")
    require(isinstance(filesystem["scratch_root"], str) and filesystem["scratch_root"], f"{cell_id}: scratch root")
    for field in ("raw_marker_sha256", "normalized_marker_sha256"):
        require(isinstance(filesystem[field], str) and SHA256.fullmatch(filesystem[field]), f"{cell_id}: {field}")

    redis = cell["redis"]
    require(isinstance(redis, dict) and set(redis) == REDIS_FIELDS, f"{cell_id}: Redis schema")
    require(type(redis["port"]) is int and 0 < redis["port"] <= 65535, f"{cell_id}: Redis port")
    before_label = expected["source_pel_before" if generated else "pel_before"]
    after_label = expected["source_pel_after" if generated else "pel_after"]
    require(redis["pel_before"] == expected_pel(before_label), f"{cell_id}: PEL before")
    require(redis["pel_after"] == expected_pel(after_label), f"{cell_id}: PEL after")
    require(isinstance(redis["group_frontier"], str) and "pending=" in redis["group_frontier"], f"{cell_id}: group frontier")
    require(isinstance(redis["xack_reply"], str) and redis["xack_reply"], f"{cell_id}: XACK reply")
    require(isinstance(redis["xack_disposition"], str) and redis["xack_disposition"], f"{cell_id}: XACK disposition")

    for field in ("pre_kill_audit_sha256", "post_restart_audit_sha256", "final_audit_sha256"):
        require(isinstance(cell[field], str) and SHA256.fullmatch(cell[field]), f"{cell_id}: {field}")
    for field in (
        "callback_before",
        "callback_after",
        "provider_attempts",
        "schedule_issue_attempts",
        "durable_outcomes",
        "durable_truths",
        "s_ack_commits",
        "s_truth_commits",
        "command_publications",
        "immediate_xack_attempts",
    ):
        require(type(cell[field]) is int and cell[field] >= 0, f"{cell_id}: invalid counter {field}")
    for field in ("s_ack_generations", "s_truth_generations"):
        require(
            isinstance(cell[field], list)
            and all(type(generation) is int and generation > 0 for generation in cell[field]),
            f"{cell_id}: invalid authenticated generation vector {field}",
        )
    require(cell["s_ack_commits"] == len(cell["s_ack_generations"]), f"{cell_id}: S_ack generation count")
    require(cell["s_truth_commits"] == len(cell["s_truth_generations"]), f"{cell_id}: S_truth generation count")
    events = cell["observed_effect_events"]
    require(
        isinstance(events, list) and all(isinstance(event, str) and event for event in events),
        f"{cell_id}: observed effect event vector",
    )
    for field in ("package_before", "package_after"):
        package = cell[field]
        require(isinstance(package, dict) and set(package) == PACKAGE_FIELDS, f"{cell_id}: {field} schema")
        require(
            package["write_generation"] is None
            or (type(package["write_generation"]) is int and package["write_generation"] > 0),
            f"{cell_id}: {field} generation",
        )
        for phase in ("p1d3_phase", "generated_market_phase"):
            require(package[phase] is None or isinstance(package[phase], str), f"{cell_id}: {field} {phase}")
    validate_sequence_audit(cell_id, cell["sequence_audit_before"])
    validate_sequence_audit(cell_id, cell["sequence_audit_after"])
    before_allocations = cell["sequence_audit_before"]["allocations"]
    after_allocations = cell["sequence_audit_after"]["allocations"]
    require(after_allocations[: len(before_allocations)] == before_allocations, f"{cell_id}: sequence allocation replay drift")
    scenario_id = expected["parent_scenario_id" if generated else "scenario_id"]
    require(
        [allocation["outcome_kind"] for allocation in after_allocations]
        == FINAL_ALLOCATION_KINDS[scenario_id],
        f"{cell_id}: final authenticated outcome sequence",
    )
    require(cell["sequence_before"] == sequence_label(cell["sequence_audit_before"], generated), f"{cell_id}: legacy sequence_before drift")
    require(cell["sequence_after"] == sequence_label(cell["sequence_audit_after"], generated), f"{cell_id}: legacy sequence_after drift")
    require(cell["durable_outcomes"] == len(after_allocations), f"{cell_id}: durable outcome count")
    require(
        cell["durable_truths"] == sum(allocation["seq_truth"] is not None for allocation in after_allocations),
        f"{cell_id}: independently derived durable truth count",
    )
    require(
        cell["provider_attempts"] == expected_effect(expected["provider_delta"], not generated),
        f"{cell_id}: provider effect",
    )
    require(
        cell["schedule_issue_attempts"]
        == expected_effect(expected["schedule_authority_delta"], False),
        f"{cell_id}: schedule effect",
    )
    if not generated:
        require(
            all(event in {"p1d3_provider", "p1d3_schedule"} for event in events),
            f"{cell_id}: base effect family",
        )
        require(cell["provider_attempts"] == events.count("p1d3_provider"), f"{cell_id}: observed provider count")
        require(cell["schedule_issue_attempts"] == events.count("p1d3_schedule"), f"{cell_id}: observed schedule count")
    for field in (
        "sequence_before",
        "sequence_after",
        "continuation_disposition",
        "final_disposition",
        "final_restart_disposition",
        "source_disposition_before_continuation",
        "duplicate_result",
        "conflict_result",
    ):
        require(isinstance(cell[field], str) and cell[field], f"{cell_id}: missing {field}")
    require(
        cell["duplicate_result"]
        == "PASS:second_clean_run_same_final_audit_and_restart_audit",
        f"{cell_id}: duplicate variant",
    )
    require(
        cell["conflict_result"] == "PASS:runtime_config_binding_and_hmac_rejected",
        f"{cell_id}: conflict variant",
    )
    require(cell["final_disposition"] == cell["continuation_disposition"], f"{cell_id}: final continuation")
    require(
        cell["final_restart_disposition"] == cell["final_disposition"]
        or (
            cell["final_disposition"] == "Ready"
            and cell["final_restart_disposition"] == "P1SemanticZeroIntentAckPending"
            and ";pending=0" in redis["group_frontier"].rsplit("final:", 1)[-1]
        ),
        f"{cell_id}: final durable restart cannot converge",
    )
    require(";pending=0" in redis["group_frontier"].rsplit("final:", 1)[-1], f"{cell_id}: final PEL not zero")

    if generated and expected["frontier_id"] in {"GM08", "GM09"}:
        require(cell["sequence_before"].startswith("seq_ack="), f"{cell_id}: pre-kill pair absent")
        require(cell["sequence_before"] == cell["sequence_after"], f"{cell_id}: pair changed")
    if generated and expected["frontier_id"] == "GM07":
        require(
            cell["sequence_audit_before"]["pre_kill_sequence_pair"] is None
            and cell["sequence_audit_before"]["durable_sequence_pair"] is None
            and not cell["sequence_before"].startswith("seq_ack="),
            f"{cell_id}: GM07 allocated a generated-Market pair",
        )
    if generated:
        frontier_id = expected["frontier_id"]
        require(events == GENERATED_EFFECT_EVENTS[frontier_id], f"{cell_id}: exact operational effect event sequence")
        terminal = GENERATED_EFFECT_SCOPE_TERMINAL[frontier_id]
        require(terminal in events, f"{cell_id}: effect scope terminal")
        scoped_events = events[: events.index(terminal) + 1]
        require(cell["provider_attempts"] == scoped_events.count("generated_provider"), f"{cell_id}: scoped provider count")
        require(cell["schedule_issue_attempts"] == scoped_events.count("generated_schedule"), f"{cell_id}: scoped schedule count")
        require(cell["s_ack_commits"] == scoped_events.count("generated_s_ack"), f"{cell_id}: scoped S_ack count")
        require(cell["s_truth_commits"] == scoped_events.count("generated_s_truth"), f"{cell_id}: scoped S_truth count")
        require(
            cell["s_ack_commits"] == (1 if expected["s_ack_delta"] == "+1_exact" else 0),
            f"{cell_id}: S_ack effect",
        )
        require(
            cell["s_truth_commits"] == (1 if expected["s_truth_delta"] == "+1_exact" else 0),
            f"{cell_id}: S_truth effect",
        )
        expected_phase = {
            "P1d4GeneratedMarketPrepublicationPending": "Prepublication",
            "P1d4GeneratedMarketDispatchPending": "Prepublication",
            "P1d4GeneratedMarketOrderPending": "Prepublication",
            "P1d4GeneratedMarketPreFinalizationPending": "Prepublication",
            "P1d4GeneratedMarketPreAckPending": "Prepublication",
            "P1d4GeneratedMarketAckCommitted": "AckCommitted",
            "P1d4GeneratedMarketTruthCommitted": "TruthCommitted",
        }[expected["expected_restart_disposition"]]
        require(cell["package_before"]["generated_market_phase"] == expected_phase, f"{cell_id}: authenticated package phase before")
        require(cell["package_after"]["generated_market_phase"] == "TruthCommitted", f"{cell_id}: authenticated package phase after")
        before_generation = cell["package_before"]["write_generation"]
        after_generation = cell["package_after"]["write_generation"]
        require(type(before_generation) is int and type(after_generation) is int, f"{cell_id}: package generations")
        generation_advance = {"Prepublication": 2, "AckCommitted": 1, "TruthCommitted": 0}[expected_phase]
        require(after_generation == before_generation + generation_advance, f"{cell_id}: package generation transition")
        if cell["s_ack_generations"]:
            require(cell["s_ack_generations"] == [before_generation + 1], f"{cell_id}: S_ack generation binding")
        if cell["s_truth_generations"]:
            require(
                cell["s_truth_generations"] == [before_generation + 1]
                and cell["s_truth_generations"][0] == after_generation,
                f"{cell_id}: S_truth generation binding",
            )
        require(
            cell["immediate_xack_attempts"]
            == (1 if expected["xack_delta"] == "+1_exact" else 0),
            f"{cell_id}: XACK delta",
        )
        require(
            cell["source_disposition_before_continuation"]
            == expected["final_source_disposition"],
            f"{cell_id}: source disposition",
        )


def validate_run(
    value: dict[str, Any],
    ordinal: int,
    base_fields: list[str],
    base_rows: dict[str, dict[str, Any]],
    generated_fields: list[str],
    generated_rows: dict[str, dict[str, Any]],
) -> None:
    require(set(value) == ROOT_FIELDS, f"run {ordinal}: root field set drift")
    require(value["schema_version"] == 1, f"run {ordinal}: schema version")
    require(value["domain"] == "moex.stage8b.p1d4.crash-replay.evidence.v1", f"run {ordinal}: domain")
    require(value["accepted_predecessor_ref"] == "1a1ea05775f1d15b86fcc3495ad6863b851e9212", f"run {ordinal}: predecessor")
    require(value["run_ordinal"] == ordinal, f"run {ordinal}: ordinal")
    require(isinstance(value["source_ref"], str) and value["source_ref"], f"run {ordinal}: source ref")
    require(isinstance(value["source_tree"], str) and value["source_tree"], f"run {ordinal}: source tree")
    require(value["matrix_sha256"] == hashlib.sha256(BASE_MATRIX.read_bytes()).hexdigest(), f"run {ordinal}: matrix hash")
    require(value["aggregate"] == AGGREGATE, f"run {ordinal}: aggregate")
    cells = value["cells"]
    require(isinstance(cells, list) and len(cells) == 105, f"run {ordinal}: cell count")
    ids = [cell.get("cell_id") for cell in cells if isinstance(cell, dict)]
    expected_ids = sorted([*base_rows, *generated_rows])
    require(ids == expected_ids, f"run {ordinal}: missing, extra, duplicate or unsorted cells")
    for cell in cells:
        cell_id = cell["cell_id"]
        if cell_id in base_rows:
            validate_cell(cell, base_fields, base_rows[cell_id], False)
        else:
            validate_cell(cell, generated_fields, generated_rows[cell_id], True)


def check(directory: pathlib.Path) -> dict[str, Any]:
    base_fields, base_rows = load_registry(BASE_MATRIX)
    generated_fields, generated_rows = load_registry(GENERATED_MATRIX)
    require(len(base_rows) == 92 and len(generated_rows) == 13, "registry cardinality drift")
    runs = [load_json(directory / name) for name in RUN_FILES]
    for ordinal, run in enumerate(runs, 1):
        validate_run(run, ordinal, base_fields, base_rows, generated_fields, generated_rows)
    require(runs[0]["source_ref"] == runs[1]["source_ref"], "source ref differs across runs")
    require(runs[0]["source_tree"] == runs[1]["source_tree"], "source tree differs across runs")
    digests = [semantic_digest(run) for run in runs]
    require(digests[0] == digests[1], "two clean-run semantic digests differ")
    # Both runs were already required to contain exactly the same 105 sorted
    # cell IDs, so ordinary zip is exact here and keeps the checker compatible
    # with the repository's supported Python 3.9 environment.
    for first, second in zip(runs[0]["cells"], runs[1]["cells"]):
        cell_id = first["cell_id"]
        require(first["cell_id"] == second["cell_id"], f"{cell_id}: duplicate identity")
        require(
            first["final_audit_sha256"] == second["final_audit_sha256"],
            f"{cell_id}: duplicate final audit",
        )
        require(
            first["final_disposition"] == second["final_disposition"],
            f"{cell_id}: duplicate final disposition",
        )
    retained = (directory / DIGEST_FILE).read_text(encoding="utf-8").strip()
    require(SHA256.fullmatch(retained) is not None, "retained digest malformed")
    require(retained == digests[0], "retained digest differs from independent digest")
    return {"cells": 105, "runs": 2, "semantic_digest": retained}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_crash_evidence_check.py EVIDENCE_DIRECTORY")
    try:
        result = check(pathlib.Path(sys.argv[1]).resolve())
    except (EvidenceFailure, OSError, KeyError, TypeError, ValueError) as error:
        print(f"FAIL stage8b-p1d4-crash-evidence-check: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "PASS stage8b-p1d4-crash-evidence-check "
        f"cells={result['cells']} runs={result['runs']} semantic_digest={result['semantic_digest']}"
    )


if __name__ == "__main__":
    main()
