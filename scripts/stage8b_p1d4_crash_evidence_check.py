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
    "callback_before",
    "callback_after",
    "provider_attempts",
    "schedule_issue_attempts",
    "durable_outcomes",
    "durable_truths",
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
        "command_publications",
        "immediate_xack_attempts",
    ):
        require(type(cell[field]) is int and cell[field] >= 0, f"{cell_id}: invalid counter {field}")
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
        require(not cell["sequence_before"].startswith("seq_ack="), f"{cell_id}: GM07 allocated a pair")
    if generated:
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
