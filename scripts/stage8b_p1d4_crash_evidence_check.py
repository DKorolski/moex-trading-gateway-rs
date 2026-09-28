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
BASE_ORACLE = ROOT / "docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv"
BASE_OPERATIONAL_ORACLE = ROOT / "docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv"
GENERATED_MATRIX = ROOT / "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv"
RUN_FILES = (
    "stage8b-p1d4-crash-replay-run-1.json",
    "stage8b-p1d4-crash-replay-run-2.json",
)
DIGEST_FILE = "stage8b-p1d4-crash-replay-semantic-digest.txt"
DOMAIN = b"moex.stage8b.p1d4.crash-replay.semantic-evidence.v4\0"
MARKER_DOMAIN = "moex.stage8b.p1d4.crash-marker.v1"
MARKER_NORMALIZED_DOMAIN = b"moex.stage8b.p1d4.crash-marker.normalized.v1\0"
WITNESS_DOMAIN = "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1"
WITNESS_DIGEST_DOMAIN = (
    b"moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v1\0"
)
ROOT_FIELDS = {
    "schema_version",
    "domain",
    "accepted_predecessor_ref",
    "source_ref",
    "source_tree",
    "matrix_sha256",
    "base_evidence_oracle_sha256",
    "base_operational_evidence_oracle_sha256",
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
    "pre_kill_audit_payload",
    "post_restart_audit_payload",
    "final_audit_payload",
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
    "truth_bearing_outcomes",
    "package_commit_history",
    "truth_replacement_commits",
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
FILESYSTEM_FIELDS = {
    "scratch_root",
    "raw_marker_sha256",
    "normalized_marker_sha256",
    "crash_marker_v1",
    "pre_kill_xack_reply_witness_v1",
    "pre_kill_xack_reply_witness_sha256",
    "pre_kill_snapshot_sha256",
    "post_restart_snapshot_sha256",
    "final_snapshot_sha256",
}
REDIS_FIELDS = {
    "port",
    "pel_before",
    "pel_after",
    "group_frontier",
    "group_frontier_v1",
    "group_frontier_sha256",
    "pre_kill_xack_reply",
    "pre_kill_xack_witness_order",
    "xack_reply",
    "xack_disposition",
}
GROUP_FRONTIER_FIELDS = {
    "schema_version",
    "domain",
    "source_stream",
    "source_group",
    "source_m10_redis_id",
    "before",
    "post_restart",
    "final",
}
GROUP_FRONTIER_POINT_FIELDS = {"last_delivered_id", "pending"}
MARKER_FIELDS = {
    "schema_version",
    "domain",
    "cell_id",
    "child_pid",
    "scenario_id",
    "frontier_id",
    "kill_hook_name",
    "pre_kill_audit_sha256",
}
WITNESS_FIELDS = MARKER_FIELDS | {
    "source_stream",
    "source_group",
    "source_m10_redis_id",
    "xack_reply",
}
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
PACKAGE_COMMIT_FIELDS = {
    "write_generation",
    "covering_seal_generation",
    "p1d3_phase",
    "generated_market_phase",
}
AUDIT_PAYLOAD_FIELDS = {
    "schema_version",
    "domain",
    "cell_id",
    "scenario_id",
    "frontier_id",
    "phase",
    "disposition",
    "filesystem_snapshot_sha256",
    "runtime_audit",
}
RUNTIME_AUDIT_FIELDS = {
    "lifecycle_sequence",
    "journal_lifecycle_sequences",
    "sequence_pair",
    "sequence_allocations",
    "package",
    "callback_count",
    "dispatch_v1_total",
    "order_v1_total",
    "trade_v1_total",
    "request_finalized_v1_total",
    "durable_outcomes",
    "truth_bearing_outcomes",
}
BASE_ORACLE_FIELDS = [
    "cell_id",
    "pre_kill_allocation_kinds",
    "final_allocation_kinds",
    "ordered_effect_events",
    "package_before_p1d3_phase",
    "package_before_generated_market_phase",
    "package_before_write_generation",
    "package_after_p1d3_phase",
    "package_after_generated_market_phase",
    "package_after_write_generation",
    "write_generation_advance",
    "truth_bearing_outcomes",
    "truth_replacement_commits",
]
BASE_OPERATIONAL_ORACLE_FIELDS = [
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


def load_base_oracle(path: pathlib.Path) -> dict[str, dict[str, Any]]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        require(reader.fieldnames == BASE_ORACLE_FIELDS, f"{path.name}: exact header drift")
        rows: dict[str, dict[str, Any]] = {}
        for raw in reader:
            cell_id = raw["cell_id"]
            require(cell_id not in rows, f"{path.name}: duplicate cell {cell_id}")
            row: dict[str, Any] = dict(raw)
            for field in (
                "package_before_write_generation",
                "package_after_write_generation",
                "write_generation_advance",
                "truth_bearing_outcomes",
                "truth_replacement_commits",
            ):
                value = row[field]
                require(value == str(int(value)) and int(value) >= 0, f"{cell_id}: invalid oracle {field}")
                row[field] = int(value)
            rows[cell_id] = row
    return rows


def load_base_operational_oracle(path: pathlib.Path) -> dict[str, dict[str, Any]]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        require(
            reader.fieldnames == BASE_OPERATIONAL_ORACLE_FIELDS,
            f"{path.name}: exact header drift",
        )
        rows: dict[str, dict[str, Any]] = {}
        for raw in reader:
            cell_id = raw["cell_id"]
            require(cell_id not in rows, f"{path.name}: duplicate cell {cell_id}")
            row: dict[str, Any] = dict(raw)
            for field in (
                "callback_before",
                "callback_after",
                "command_publications",
                "immediate_xack_attempts",
                "before_pending",
                "post_restart_pending",
                "final_pending",
            ):
                value = row[field]
                require(
                    value == str(int(value)) and int(value) >= 0,
                    f"{cell_id}: invalid operational oracle {field}",
                )
                row[field] = int(value)
            rows[cell_id] = row
    return rows


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def oracle_vector(value: str) -> list[str]:
    return [] if value == "none" else value.split("|")


def oracle_phase(value: str) -> Any:
    return None if value == "none" else value


def semantic_view(value: dict[str, Any]) -> dict[str, Any]:
    normalized = copy.deepcopy(value)
    normalized["run_ordinal"] = 0
    for cell in normalized["cells"]:
        cell["process"]["child_pid"] = 0
        cell["process"]["wall_duration_ms"] = 0
        cell["filesystem"]["scratch_root"] = "<VOLATILE_PATH>"
        cell["filesystem"]["raw_marker_sha256"] = "<VOLATILE_MARKER_SHA256>"
        cell["filesystem"]["crash_marker_v1"]["child_pid"] = 0
        witness = cell["filesystem"]["pre_kill_xack_reply_witness_v1"]
        if witness is not None:
            witness["child_pid"] = 0
            cell["filesystem"]["pre_kill_xack_reply_witness_sha256"] = (
                "<VOLATILE_WITNESS_SHA256>"
            )
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


def framed_sha256(domain: bytes, payload: bytes) -> str:
    return hashlib.sha256(domain + len(payload).to_bytes(8, "big") + payload).hexdigest()


def validate_marker_and_witness(
    cell_id: str,
    scenario_id: str,
    frontier_id: str,
    kill_hook_name: str,
    process: dict[str, Any],
    filesystem: dict[str, Any],
    redis: dict[str, Any],
    source_frontier: dict[str, Any],
) -> None:
    marker = filesystem["crash_marker_v1"]
    require(
        isinstance(marker, dict) and set(marker) == MARKER_FIELDS,
        f"{cell_id}: exact CrashMarkerV1 field set",
    )
    expected_marker = {
        "schema_version": 1,
        "domain": MARKER_DOMAIN,
        "cell_id": cell_id,
        "child_pid": process["child_pid"],
        "scenario_id": scenario_id,
        "frontier_id": frontier_id,
        "kill_hook_name": kill_hook_name,
        "pre_kill_audit_sha256": filesystem["pre_kill_snapshot_sha256"],
    }
    require(marker == expected_marker, f"{cell_id}: exact CrashMarkerV1 binding")
    raw_marker = canonical_bytes(marker)
    require(
        hashlib.sha256(raw_marker).hexdigest() == filesystem["raw_marker_sha256"],
        f"{cell_id}: raw CrashMarkerV1 hash",
    )
    normalized_marker = copy.deepcopy(marker)
    normalized_marker["child_pid"] = 0
    require(
        framed_sha256(MARKER_NORMALIZED_DOMAIN, canonical_bytes(normalized_marker))
        == filesystem["normalized_marker_sha256"],
        f"{cell_id}: normalized CrashMarkerV1 hash",
    )

    witness = filesystem["pre_kill_xack_reply_witness_v1"]
    witness_sha256 = filesystem["pre_kill_xack_reply_witness_sha256"]
    if frontier_id == "F16":
        require(
            isinstance(witness, dict) and set(witness) == WITNESS_FIELDS,
            f"{cell_id}: exact F16 witness field set",
        )
        expected_witness = {
            "schema_version": 1,
            "domain": WITNESS_DOMAIN,
            "cell_id": cell_id,
            "child_pid": process["child_pid"],
            "scenario_id": scenario_id,
            "frontier_id": frontier_id,
            "kill_hook_name": kill_hook_name,
            "pre_kill_audit_sha256": filesystem["pre_kill_snapshot_sha256"],
            "source_stream": source_frontier["source_stream"],
            "source_group": source_frontier["source_group"],
            "source_m10_redis_id": source_frontier["source_m10_redis_id"],
            "xack_reply": "integer:1",
        }
        require(witness == expected_witness, f"{cell_id}: exact F16 witness binding")
        require(
            isinstance(witness_sha256, str)
            and SHA256.fullmatch(witness_sha256)
            and framed_sha256(WITNESS_DIGEST_DOMAIN, canonical_bytes(witness))
            == witness_sha256,
            f"{cell_id}: F16 witness canonical hash",
        )
        require(
            redis["pre_kill_xack_reply"] == "integer:1"
            and redis["pre_kill_xack_witness_order"]
            == "after_integer_1_before_crash_marker",
            f"{cell_id}: exact F16 pre-kill protocol order",
        )
        require(
            redis["xack_reply"] == "integer:0"
            and redis["xack_disposition"] == "AlreadyAcknowledged",
            f"{cell_id}: F16 restart must resolve AlreadyAcknowledged",
        )
        require(
            source_frontier["before"]["pending"] == 0
            and source_frontier["post_restart"]["pending"] == 0,
            f"{cell_id}: F16 source must remain absent after parsed XACK 1",
        )
    else:
        require(witness is None, f"{cell_id}: non-F16 witness is forbidden")
        require(witness_sha256 is None, f"{cell_id}: non-F16 witness hash is forbidden")
        require(
            redis["pre_kill_xack_reply"] == "not_observed"
            and redis["pre_kill_xack_witness_order"] == "not_applicable",
            f"{cell_id}: non-F16 pre-kill witness evidence is forbidden",
        )


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


def validate_package(cell_id: str, label: str, package: Any) -> None:
    require(isinstance(package, dict) and set(package) == PACKAGE_FIELDS, f"{cell_id}: {label} schema")
    require(
        package["write_generation"] is None
        or (type(package["write_generation"]) is int and package["write_generation"] > 0),
        f"{cell_id}: {label} generation",
    )
    for phase in ("p1d3_phase", "generated_market_phase"):
        require(package[phase] is None or isinstance(package[phase], str), f"{cell_id}: {label} {phase}")


def validate_runtime_audit(
    cell_id: str,
    label: str,
    audit: Any,
    sequence_audit: dict[str, Any],
    package: dict[str, Any],
    callback_count: int,
    durable_outcomes: int,
    truth_bearing_outcomes: int,
) -> None:
    require(isinstance(audit, dict) and set(audit) == RUNTIME_AUDIT_FIELDS, f"{cell_id}: {label} runtime audit schema")
    for field in (
        "lifecycle_sequence",
        "callback_count",
        "dispatch_v1_total",
        "order_v1_total",
        "trade_v1_total",
        "request_finalized_v1_total",
        "durable_outcomes",
        "truth_bearing_outcomes",
    ):
        require(type(audit[field]) is int and audit[field] >= 0, f"{cell_id}: {label} {field}")
    require(audit["lifecycle_sequence"] == sequence_audit["lifecycle_sequence"], f"{cell_id}: {label} lifecycle binding")
    require(
        audit["journal_lifecycle_sequences"] == sequence_audit["journal_lifecycle_sequences"],
        f"{cell_id}: {label} journal binding",
    )
    require(audit["sequence_pair"] == sequence_audit["durable_sequence_pair"], f"{cell_id}: {label} pair binding")
    require(audit["sequence_allocations"] == sequence_audit["allocations"], f"{cell_id}: {label} allocation binding")
    require(audit["package"] == package, f"{cell_id}: {label} package binding")
    require(audit["callback_count"] == callback_count, f"{cell_id}: {label} callback binding")
    require(audit["durable_outcomes"] == durable_outcomes, f"{cell_id}: {label} durable outcome binding")
    require(
        audit["truth_bearing_outcomes"] == truth_bearing_outcomes,
        f"{cell_id}: {label} truth-bearing outcome binding",
    )


def validate_audit_payload(
    cell_id: str,
    scenario_id: str,
    frontier_id: str,
    label: str,
    payload: Any,
    retained_sha256: str,
    filesystem_snapshot_sha256: str,
    sequence_audit: dict[str, Any],
    package: dict[str, Any],
    callback_count: int,
    durable_outcomes: int,
    truth_bearing_outcomes: int,
) -> str:
    require(isinstance(payload, dict) and set(payload) == AUDIT_PAYLOAD_FIELDS, f"{cell_id}: {label} audit payload schema")
    require(payload["schema_version"] == 1, f"{cell_id}: {label} audit payload version")
    require(payload["domain"] == "moex.stage8b.p1d4.structured-runtime-audit.v1", f"{cell_id}: {label} audit domain")
    require(payload["cell_id"] == cell_id, f"{cell_id}: {label} audit cell binding")
    require(payload["scenario_id"] == scenario_id, f"{cell_id}: {label} audit scenario binding")
    require(payload["frontier_id"] == frontier_id, f"{cell_id}: {label} audit frontier binding")
    require(payload["phase"] == label, f"{cell_id}: {label} audit phase binding")
    require(payload["filesystem_snapshot_sha256"] == filesystem_snapshot_sha256, f"{cell_id}: {label} filesystem binding")
    require(isinstance(payload["disposition"], str) and payload["disposition"], f"{cell_id}: {label} disposition")
    require(hashlib.sha256(canonical_bytes(payload)).hexdigest() == retained_sha256, f"{cell_id}: {label} audit hash")
    validate_runtime_audit(
        cell_id,
        label,
        payload["runtime_audit"],
        sequence_audit,
        package,
        callback_count,
        durable_outcomes,
        truth_bearing_outcomes,
    )
    return payload["disposition"]


def truth_replacement_count(before: dict[str, Any], history: list[dict[str, Any]]) -> int:
    prior_p1d3 = before["p1d3_phase"]
    prior_generated = before["generated_market_phase"]
    count = 0
    for commit in history:
        p1d3_transition = commit["p1d3_phase"] in {"Working", "Terminal"} and commit["p1d3_phase"] != prior_p1d3
        generated_transition = commit["generated_market_phase"] == "TruthCommitted" and prior_generated != "TruthCommitted"
        count += int(p1d3_transition or generated_transition)
        prior_p1d3 = commit["p1d3_phase"]
        prior_generated = commit["generated_market_phase"]
    return count


def validate_package_history(cell_id: str, cell: dict[str, Any]) -> None:
    history = cell["package_commit_history"]
    require(isinstance(history, list), f"{cell_id}: package commit history")
    before = cell["package_before"]
    after = cell["package_after"]
    before_generation = before["write_generation"]
    after_generation = after["write_generation"]
    require(type(before_generation) is int and type(after_generation) is int, f"{cell_id}: package generations")
    require(after_generation >= before_generation, f"{cell_id}: package generation reversal")
    require(len(history) == after_generation - before_generation, f"{cell_id}: package commit count")
    for index, commit in enumerate(history, 1):
        require(isinstance(commit, dict) and set(commit) == PACKAGE_COMMIT_FIELDS, f"{cell_id}: package commit schema")
        require(type(commit["write_generation"]) is int and commit["write_generation"] > 0, f"{cell_id}: package commit write generation")
        require(type(commit["covering_seal_generation"]) is int and commit["covering_seal_generation"] > 0, f"{cell_id}: package commit seal generation")
        for phase in ("p1d3_phase", "generated_market_phase"):
            require(commit[phase] is None or isinstance(commit[phase], str), f"{cell_id}: package commit {phase}")
        require(commit["write_generation"] == before_generation + index, f"{cell_id}: package commit generation gap")
        if index > 1:
            require(
                commit["covering_seal_generation"]
                == history[index - 2]["covering_seal_generation"] + 1,
                f"{cell_id}: covering-seal generation gap",
            )
    if history:
        require(
            history[-1]["write_generation"] == after["write_generation"]
            and history[-1]["p1d3_phase"] == after["p1d3_phase"]
            and history[-1]["generated_market_phase"] == after["generated_market_phase"],
            f"{cell_id}: final package commit binding",
        )
    else:
        require(before == after, f"{cell_id}: package changed without observed commit")
    require(type(cell["truth_replacement_commits"]) is int and cell["truth_replacement_commits"] >= 0, f"{cell_id}: truth replacement count")
    require(
        cell["truth_replacement_commits"] == truth_replacement_count(before, history),
        f"{cell_id}: truth replacement derivation",
    )


def validate_base_oracle(cell_id: str, cell: dict[str, Any], oracle: dict[str, Any]) -> None:
    before = cell["package_before"]
    after = cell["package_after"]
    require(
        [allocation["outcome_kind"] for allocation in cell["sequence_audit_before"]["allocations"]]
        == oracle_vector(oracle["pre_kill_allocation_kinds"]),
        f"{cell_id}: exact pre-kill allocation oracle",
    )
    require(
        [allocation["outcome_kind"] for allocation in cell["sequence_audit_after"]["allocations"]]
        == oracle_vector(oracle["final_allocation_kinds"]),
        f"{cell_id}: exact final allocation oracle",
    )
    require(cell["observed_effect_events"] == oracle_vector(oracle["ordered_effect_events"]), f"{cell_id}: exact effect order oracle")
    require(before["p1d3_phase"] == oracle_phase(oracle["package_before_p1d3_phase"]), f"{cell_id}: package-before P1-d3 oracle")
    require(before["generated_market_phase"] == oracle_phase(oracle["package_before_generated_market_phase"]), f"{cell_id}: package-before generated oracle")
    require(before["write_generation"] == oracle["package_before_write_generation"], f"{cell_id}: package-before generation oracle")
    require(after["p1d3_phase"] == oracle_phase(oracle["package_after_p1d3_phase"]), f"{cell_id}: package-after P1-d3 oracle")
    require(after["generated_market_phase"] == oracle_phase(oracle["package_after_generated_market_phase"]), f"{cell_id}: package-after generated oracle")
    require(after["write_generation"] == oracle["package_after_write_generation"], f"{cell_id}: package-after generation oracle")
    require(after["write_generation"] - before["write_generation"] == oracle["write_generation_advance"], f"{cell_id}: generation-advance oracle")
    require(cell["truth_bearing_outcomes"] == oracle["truth_bearing_outcomes"], f"{cell_id}: truth-bearing V3 oracle")
    require(cell["truth_replacement_commits"] == oracle["truth_replacement_commits"], f"{cell_id}: truth replacement oracle")


def validate_group_frontier(cell_id: str, redis: dict[str, Any]) -> dict[str, Any]:
    frontier = redis["group_frontier_v1"]
    require(
        isinstance(frontier, dict) and set(frontier) == GROUP_FRONTIER_FIELDS,
        f"{cell_id}: typed group frontier schema",
    )
    require(frontier["schema_version"] == 1, f"{cell_id}: group frontier version")
    require(
        frontier["domain"] == "moex.stage8b.p1d4.redis-source-frontier.v1",
        f"{cell_id}: group frontier domain",
    )
    for field in ("source_stream", "source_group"):
        require(
            isinstance(frontier[field], str) and frontier[field],
            f"{cell_id}: group frontier {field}",
        )
    source_id = frontier["source_m10_redis_id"]
    require(
        isinstance(source_id, str) and re.fullmatch(r"[0-9]+-[0-9]+", source_id),
        f"{cell_id}: source M10 Redis identity",
    )
    for phase in ("before", "post_restart", "final"):
        point = frontier[phase]
        require(
            isinstance(point, dict) and set(point) == GROUP_FRONTIER_POINT_FIELDS,
            f"{cell_id}: group frontier {phase} schema",
        )
        require(
            point["last_delivered_id"] == source_id,
            f"{cell_id}: {phase} last-delivered/source identity mismatch",
        )
        require(
            type(point["pending"]) is int and point["pending"] >= 0,
            f"{cell_id}: group frontier {phase} pending",
        )
    require(
        redis["pel_before"] == frontier["before"]["pending"]
        and redis["pel_after"] == frontier["post_restart"]["pending"],
        f"{cell_id}: PEL/group-frontier mismatch",
    )
    require(frontier["final"]["pending"] == 0, f"{cell_id}: final source PEL")
    legacy = (
        f"before:last_delivered_id={source_id};pending={frontier['before']['pending']};"
        f"after:last_delivered_id={source_id};pending={frontier['post_restart']['pending']};"
        f"final:last_delivered_id={source_id};pending={frontier['final']['pending']}"
    )
    require(redis["group_frontier"] == legacy, f"{cell_id}: legacy/typed frontier drift")
    require(
        isinstance(redis["group_frontier_sha256"], str)
        and SHA256.fullmatch(redis["group_frontier_sha256"])
        and redis["group_frontier_sha256"]
        == hashlib.sha256(canonical_bytes(frontier)).hexdigest(),
        f"{cell_id}: group frontier canonical hash",
    )
    require(
        (redis["xack_reply"], redis["xack_disposition"])
        in {
            ("integer:1", "AcknowledgedPending"),
            ("integer:0", "AlreadyAcknowledged"),
            ("not_applicable", "NoSource"),
        },
        f"{cell_id}: impossible XACK reply/disposition pair",
    )
    require(
        redis["pre_kill_xack_reply"] in {"integer:1", "not_observed"},
        f"{cell_id}: pre-kill XACK reply vocabulary",
    )
    return frontier


def validate_base_operational_oracle(
    cell_id: str,
    cell: dict[str, Any],
    oracle: dict[str, Any],
    frontier: dict[str, Any],
) -> None:
    redis = cell["redis"]
    for field in (
        "callback_before",
        "callback_after",
        "command_publications",
        "immediate_xack_attempts",
        "source_disposition_before_continuation",
    ):
        require(cell[field] == oracle[field], f"{cell_id}: exact {field} oracle")
    require(redis["xack_reply"] == oracle["xack_reply"], f"{cell_id}: exact XACK reply oracle")
    require(
        redis["pre_kill_xack_reply"] == oracle["pre_kill_xack_reply"],
        f"{cell_id}: exact pre-kill XACK reply oracle",
    )
    require(
        redis["xack_disposition"] == oracle["xack_disposition"],
        f"{cell_id}: exact XACK disposition oracle",
    )
    require(frontier["source_stream"] == oracle["source_stream"], f"{cell_id}: exact source stream oracle")
    require(frontier["source_group"] == oracle["source_group"], f"{cell_id}: exact source group oracle")
    require(
        frontier["source_m10_redis_id"] == oracle["source_m10_redis_id"],
        f"{cell_id}: exact source M10 identity oracle",
    )
    for phase, id_field, pending_field in (
        ("before", "before_last_delivered_id", "before_pending"),
        ("post_restart", "post_restart_last_delivered_id", "post_restart_pending"),
        ("final", "final_last_delivered_id", "final_pending"),
    ):
        require(
            frontier[phase]["last_delivered_id"] == oracle[id_field]
            and frontier[phase]["pending"] == oracle[pending_field],
            f"{cell_id}: exact {phase} group-frontier oracle",
        )


def expected_effect(label: str, base: bool) -> int:
    if label.startswith("+1") or (base and label == "0_before_reissue"):
        return 1
    return 0


def validate_cell(
    cell: dict[str, Any],
    registry_fields: list[str],
    expected: dict[str, Any],
    generated: bool,
    base_oracle: Any = None,
    base_operational_oracle: Any = None,
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
    scenario_id = expected["parent_scenario_id" if generated else "scenario_id"]
    kill_hook_name = (
        f"p1d4-generated-market-{expected['frontier_id'].lower()}"
        if generated
        else expected["kill_hook_name"]
    )

    filesystem = cell["filesystem"]
    require(isinstance(filesystem, dict) and set(filesystem) == FILESYSTEM_FIELDS, f"{cell_id}: filesystem schema")
    require(isinstance(filesystem["scratch_root"], str) and filesystem["scratch_root"], f"{cell_id}: scratch root")
    for field in (
        "raw_marker_sha256",
        "normalized_marker_sha256",
        "pre_kill_snapshot_sha256",
        "post_restart_snapshot_sha256",
        "final_snapshot_sha256",
    ):
        require(isinstance(filesystem[field], str) and SHA256.fullmatch(filesystem[field]), f"{cell_id}: {field}")

    redis = cell["redis"]
    require(isinstance(redis, dict) and set(redis) == REDIS_FIELDS, f"{cell_id}: Redis schema")
    require(type(redis["port"]) is int and 0 < redis["port"] <= 65535, f"{cell_id}: Redis port")
    before_label = expected["source_pel_before" if generated else "pel_before"]
    after_label = expected["source_pel_after" if generated else "pel_after"]
    require(redis["pel_before"] == expected_pel(before_label), f"{cell_id}: PEL before")
    require(redis["pel_after"] == expected_pel(after_label), f"{cell_id}: PEL after")
    require(isinstance(redis["xack_reply"], str) and redis["xack_reply"], f"{cell_id}: XACK reply")
    require(isinstance(redis["xack_disposition"], str) and redis["xack_disposition"], f"{cell_id}: XACK disposition")
    group_frontier = validate_group_frontier(cell_id, redis)
    validate_marker_and_witness(
        cell_id,
        scenario_id,
        expected["frontier_id"],
        kill_hook_name,
        process,
        filesystem,
        redis,
        group_frontier,
    )

    for field in ("pre_kill_audit_sha256", "post_restart_audit_sha256", "final_audit_sha256"):
        require(isinstance(cell[field], str) and SHA256.fullmatch(cell[field]), f"{cell_id}: {field}")
    for field in (
        "callback_before",
        "callback_after",
        "provider_attempts",
        "schedule_issue_attempts",
        "durable_outcomes",
        "truth_bearing_outcomes",
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
        validate_package(cell_id, field, cell[field])
    validate_sequence_audit(cell_id, cell["sequence_audit_before"])
    validate_sequence_audit(cell_id, cell["sequence_audit_after"])
    before_allocations = cell["sequence_audit_before"]["allocations"]
    after_allocations = cell["sequence_audit_after"]["allocations"]
    require(after_allocations[: len(before_allocations)] == before_allocations, f"{cell_id}: sequence allocation replay drift")
    require(
        [allocation["outcome_kind"] for allocation in after_allocations]
        == FINAL_ALLOCATION_KINDS[scenario_id],
        f"{cell_id}: final authenticated outcome sequence",
    )
    require(cell["sequence_before"] == sequence_label(cell["sequence_audit_before"], generated), f"{cell_id}: legacy sequence_before drift")
    require(cell["sequence_after"] == sequence_label(cell["sequence_audit_after"], generated), f"{cell_id}: legacy sequence_after drift")
    require(cell["durable_outcomes"] == len(after_allocations), f"{cell_id}: durable outcome count")
    require(
        cell["truth_bearing_outcomes"] == sum(allocation["seq_truth"] is not None for allocation in after_allocations),
        f"{cell_id}: independently derived durable truth count",
    )
    validate_package_history(cell_id, cell)
    before_truths = sum(allocation["seq_truth"] is not None for allocation in before_allocations)
    pre_disposition = validate_audit_payload(
        cell_id,
        scenario_id,
        expected["frontier_id"],
        "pre-kill",
        cell["pre_kill_audit_payload"],
        cell["pre_kill_audit_sha256"],
        filesystem["pre_kill_snapshot_sha256"],
        cell["sequence_audit_before"],
        cell["package_before"],
        cell["callback_before"],
        len(before_allocations),
        before_truths,
    )
    post_disposition = validate_audit_payload(
        cell_id,
        scenario_id,
        expected["frontier_id"],
        "post-restart",
        cell["post_restart_audit_payload"],
        cell["post_restart_audit_sha256"],
        filesystem["post_restart_snapshot_sha256"],
        cell["sequence_audit_before"],
        cell["package_before"],
        cell["callback_before"],
        len(before_allocations),
        before_truths,
    )
    final_audit_disposition = validate_audit_payload(
        cell_id,
        scenario_id,
        expected["frontier_id"],
        "final",
        cell["final_audit_payload"],
        cell["final_audit_sha256"],
        filesystem["final_snapshot_sha256"],
        cell["sequence_audit_after"],
        cell["package_after"],
        cell["callback_after"],
        cell["durable_outcomes"],
        cell["truth_bearing_outcomes"],
    )
    require(
        cell["pre_kill_audit_payload"]["runtime_audit"]
        == cell["post_restart_audit_payload"]["runtime_audit"],
        f"{cell_id}: pre-kill/post-restart runtime audit drift",
    )
    require(pre_disposition == post_disposition, f"{cell_id}: pre-kill/post-restart disposition drift")
    if cell["restart_disposition"] == "Ready":
        require(
            pre_disposition in {"Ready", "P1SemanticZeroIntentAckPending", "P1d3TruthCommitted"},
            f"{cell_id}: normalized Ready audit disposition",
        )
    else:
        require(pre_disposition == cell["restart_disposition"], f"{cell_id}: audit restart disposition")
    require(final_audit_disposition == cell["final_restart_disposition"], f"{cell_id}: final audit disposition")
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
        require(base_oracle is not None, f"{cell_id}: base oracle missing")
        require(
            base_operational_oracle is not None,
            f"{cell_id}: base operational oracle missing",
        )
        validate_base_oracle(cell_id, cell, base_oracle)
        validate_base_operational_oracle(
            cell_id, cell, base_operational_oracle, group_frontier
        )
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
            and group_frontier["final"]["pending"] == 0
        ),
        f"{cell_id}: final durable restart cannot converge",
    )
    require(group_frontier["final"]["pending"] == 0, f"{cell_id}: final PEL not zero")

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
        require(
            cell["truth_replacement_commits"]
            == (0 if expected_phase == "TruthCommitted" else 1),
            f"{cell_id}: generated truth replacement binding",
        )
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
    base_oracle_rows: dict[str, dict[str, Any]],
    base_operational_oracle_rows: dict[str, dict[str, Any]],
) -> None:
    require(set(value) == ROOT_FIELDS, f"run {ordinal}: root field set drift")
    require(value["schema_version"] == 4, f"run {ordinal}: schema version")
    require(value["domain"] == "moex.stage8b.p1d4.crash-replay.evidence.v4", f"run {ordinal}: domain")
    require(value["accepted_predecessor_ref"] == "1a1ea05775f1d15b86fcc3495ad6863b851e9212", f"run {ordinal}: predecessor")
    require(value["run_ordinal"] == ordinal, f"run {ordinal}: ordinal")
    require(isinstance(value["source_ref"], str) and value["source_ref"], f"run {ordinal}: source ref")
    require(isinstance(value["source_tree"], str) and value["source_tree"], f"run {ordinal}: source tree")
    require(value["matrix_sha256"] == hashlib.sha256(BASE_MATRIX.read_bytes()).hexdigest(), f"run {ordinal}: matrix hash")
    require(
        value["base_evidence_oracle_sha256"] == hashlib.sha256(BASE_ORACLE.read_bytes()).hexdigest(),
        f"run {ordinal}: base evidence oracle hash",
    )
    require(
        value["base_operational_evidence_oracle_sha256"]
        == hashlib.sha256(BASE_OPERATIONAL_ORACLE.read_bytes()).hexdigest(),
        f"run {ordinal}: base operational evidence oracle hash",
    )
    require(value["aggregate"] == AGGREGATE, f"run {ordinal}: aggregate")
    cells = value["cells"]
    require(isinstance(cells, list) and len(cells) == 105, f"run {ordinal}: cell count")
    ids = [cell.get("cell_id") for cell in cells if isinstance(cell, dict)]
    expected_ids = sorted([*base_rows, *generated_rows])
    require(ids == expected_ids, f"run {ordinal}: missing, extra, duplicate or unsorted cells")
    for cell in cells:
        cell_id = cell["cell_id"]
        if cell_id in base_rows:
            validate_cell(
                cell,
                base_fields,
                base_rows[cell_id],
                False,
                base_oracle_rows[cell_id],
                base_operational_oracle_rows[cell_id],
            )
        else:
            validate_cell(cell, generated_fields, generated_rows[cell_id], True)


def check(directory: pathlib.Path) -> dict[str, Any]:
    base_fields, base_rows = load_registry(BASE_MATRIX)
    generated_fields, generated_rows = load_registry(GENERATED_MATRIX)
    base_oracle_rows = load_base_oracle(BASE_ORACLE)
    base_operational_oracle_rows = load_base_operational_oracle(
        BASE_OPERATIONAL_ORACLE
    )
    require(len(base_rows) == 92 and len(generated_rows) == 13, "registry cardinality drift")
    require(set(base_oracle_rows) == set(base_rows), "base evidence oracle identity drift")
    require(
        set(base_operational_oracle_rows) == set(base_rows),
        "base operational evidence oracle identity drift",
    )
    runs = [load_json(directory / name) for name in RUN_FILES]
    for ordinal, run in enumerate(runs, 1):
        validate_run(
            run,
            ordinal,
            base_fields,
            base_rows,
            generated_fields,
            generated_rows,
            base_oracle_rows,
            base_operational_oracle_rows,
        )
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
