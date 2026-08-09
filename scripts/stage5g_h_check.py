#!/usr/bin/env python3
"""Exact aggregate closure checks for Stage 5G-h."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

BASE = "ee0505dfee71f043f3185c16cbdd563e3b36a6c1"
STAGE5G_F = "12af52d23218c67bc15b7b79835790e40834dfbb"
BRANCH = "stage5g-lifecycle"
ARTIFACT_SHA256 = "0f6698a7256537596071eef762f7d623050d1a1ec3023ecafc9b3799e9ba8bf0"
INVENTORY_SHA256 = "546552301c26fe80cd4106221e25aa2ec35c378708fc208cd3b9a46aa6ce2fd0"
ARTIFACT = Path("docs/stage-5/accepted-stage5g-g-lifecycle-artifact.json")
DESCRIPTOR = Path("docs/stage-5/stage5g-closure-descriptor.json")
INVENTORY = Path("docs/stage-5/stage5g-authority-inventory.json")
SOURCE = Path("crates/strategy-runtime-core/src/stage5g_lifecycle_freeze.rs")
FAMILY_COUNTS = {"ACK": 10, "ORDER_POSITION": 16, "TIMER": 8, "RESTART": 12, "PROTECTIVE": 8}


class CheckFailure(ValueError):
    pass


def require(value: bool, message: str) -> None:
    if not value:
        raise CheckFailure(message)


def canonical_row_sha256(row: dict) -> str:
    value = json.loads(json.dumps(row))
    value["canonical_row_fingerprint_sha256"] = ""
    encoded = json.dumps(
        ["stage5g-g-lifecycle-row-v1", value],
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode()
    return hashlib.sha256(encoded).hexdigest()


def validate_rows(rows: object, accepted: list[dict]) -> None:
    require(isinstance(rows, list), "artifact is not an array")
    require(len(rows) == 54, "artifact row count drift")
    require(len({row.get("scenario_id") for row in rows}) == 54, "scenario identity drift")
    counts = {family: sum(row.get("family") == family for row in rows) for family in FAMILY_COUNTS}
    require(counts == FAMILY_COUNTS, "family count drift")
    for row in rows:
        require(row.get("schema_version") == 1, "row schema drift")
        require(row.get("accepted_predecessor") == STAGE5G_F, "row predecessor drift")
        require(
            row.get("canonical_row_fingerprint_sha256") == canonical_row_sha256(row),
            f"canonical row digest drift: {row.get('scenario_id')}",
        )
        require(not any(row.get("closed_surfaces", {}).values()), "closed surface opened")
    require(rows == accepted, "accepted semantic row bytes drift")


def validate_descriptor(value: dict) -> None:
    require(value.get("schema_version") == 1, "descriptor schema drift")
    require(value.get("accepted_stage5g_g_predecessor") == BASE, "descriptor Stage G ref drift")
    require(value.get("accepted_stage5g_f_predecessor") == STAGE5G_F, "descriptor Stage F ref drift")
    binding = value.get("source_ref_binding", {})
    require(binding.get("kind") == "handoff_commit_marker", "descriptor source binding drift")
    require(binding.get("required_branch") == BRANCH, "descriptor branch drift")
    require(binding.get("required_parent") == BASE, "descriptor parent drift")
    matrix = value.get("accepted_matrix", {})
    require(matrix.get("path") == str(ARTIFACT), "descriptor artifact path drift")
    require(matrix.get("sha256") == ARTIFACT_SHA256, "descriptor artifact digest drift")
    require(matrix.get("row_count") == 54, "descriptor row count drift")
    require(matrix.get("family_counts") == FAMILY_COUNTS, "descriptor family counts drift")
    require(value.get("parallel_source_families") == ["ACK", "ORDER_POSITION", "PROTECTIVE"], "parallel family drift")
    require(value.get("immutable_witness_families") == ["TIMER", "RESTART"], "witness family drift")
    require(value.get("authority_inventory_sha256") == INVENTORY_SHA256, "inventory digest drift")
    require(value.get("next_transition") == "Transition Gate 5->6", "next transition drift")
    require(value.get("stage6_status") == "closed_pending_transition_gate_acceptance", "Stage 6 opened")
    require(not any(value.get("closed_surfaces", {}).values()), "descriptor closed surface opened")


def validate_inventory(root: Path, value: dict) -> None:
    require(value.get("schema_version") == 1, "inventory schema drift")
    require(value.get("accepted_stage5g_g_predecessor") == BASE, "inventory predecessor drift")
    entries = value.get("authorities")
    require(isinstance(entries, list) and len(entries) >= 12, "authority inventory incomplete")
    paths: set[str] = set()
    classes = {"semantic_authority", "restart_authority", "evidence_adapter", "fixture_only_adapter"}
    for entry in entries:
        path = entry.get("path", "")
        require(path not in paths, f"duplicate authority: {path}")
        paths.add(path)
        require(entry.get("classification") in classes, f"bad authority class: {path}")
        target = root / path
        require(target.is_file(), f"missing authority: {path}")
        require(hashlib.sha256(target.read_bytes()).hexdigest() == entry.get("sha256"), f"authority digest drift: {path}")
    require(str(ARTIFACT) in paths, "accepted artifact absent from inventory")


def validate_parallel_source(source: str) -> None:
    required = (
        "stage5g_g_ack_artifact_rows",
        "stage5g_g_order_position_artifact_rows",
        "stage5g_f_gprt_artifact_rows_parallel_verified",
        "Stage 5G-h true-parallel source production must preserve accepted bytes",
    )
    for token in required:
        require(token in source, f"parallel source producer missing: {token}")
    require(source.count("std::thread::spawn") >= 3, "three source workers are required")
    require("spawn(move || row)" not in source, "parallel path returns cloned completed rows")
    ack_at = source.index("stage5g_g_ack_artifact_rows")
    order_at = source.index("stage5g_g_order_position_artifact_rows")
    protective_at = source.index("stage5g_f_gprt_artifact_rows_parallel_verified")
    witness_at = source.index("parallel.extend(TIMER")
    require(max(ack_at, order_at, protective_at) < witness_at, "source workers are not started before canonical merge")


def check(root: Path, generated: Path | None, parallel: Path | None) -> None:
    accepted_bytes = (root / ARTIFACT).read_bytes()
    require(hashlib.sha256(accepted_bytes).hexdigest() == ARTIFACT_SHA256, "accepted artifact SHA drift")
    accepted = json.loads(accepted_bytes)
    validate_rows(accepted, accepted)
    descriptor = json.loads((root / DESCRIPTOR).read_text())
    validate_descriptor(descriptor)
    inventory_bytes = (root / INVENTORY).read_bytes()
    require(descriptor.get("authority_inventory") == str(INVENTORY), "descriptor inventory path drift")
    require(descriptor.get("authority_inventory_sha256") == hashlib.sha256(inventory_bytes).hexdigest(), "descriptor inventory digest drift")
    validate_inventory(root, json.loads(inventory_bytes))
    validate_parallel_source((root / SOURCE).read_text())
    for label, candidate in (("generated", generated), ("true-parallel", parallel)):
        if candidate is not None:
            data = candidate.read_bytes()
            require(data == accepted_bytes, f"{label} artifact is not byte-identical to accepted artifact")
            validate_rows(json.loads(data), accepted)
    print("stage5g-h-check: PASS rows=54 exact_artifact=true")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--artifact", type=Path)
    parser.add_argument("--parallel-artifact", type=Path)
    args = parser.parse_args()
    try:
        check(args.root.resolve(), args.artifact.resolve() if args.artifact else None, args.parallel_artifact.resolve() if args.parallel_artifact else None)
    except CheckFailure as error:
        raise SystemExit(f"stage5g-h-check: FAIL: {error}") from error


if __name__ == "__main__":
    main()
