#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e I1 aggregate candidate."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
from pathlib import Path
from typing import Any

import stage8b_p1e_i1_aggregate_readiness_check as readiness
import stage8b_p1e_i1_fixed_install_check as fixed_install


ROOT = Path(__file__).resolve().parents[1]
BASE = "7f2e876c4cad7a3a4a0fa10a1eb5202e58202d2f"
DOCUMENT = "docs/stage-8/stage8b-p1e-i1-aggregate-acceptance.md"
INVENTORY = "docs/stage-8/stage8b-p1e-i1-aggregate-acceptance.json"
MATRIX = "docs/stage-8/stage8b-p1e-i1-aggregate-acceptance-matrix.csv"
FIXED_DOCUMENT = "docs/stage-8/stage8b-p1e-i1-fixed-path-installation.md"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    FIXED_DOCUMENT,
    STATUS,
    ROADMAP,
    "scripts/make_stage8b_p1e_i1_aggregate_acceptance_handoff.py",
    "scripts/stage8b_p1e_i1_aggregate_acceptance_check.py",
    "scripts/stage8b_p1e_i1_aggregate_acceptance_gate.sh",
    "scripts/stage8b_p1e_i1_aggregate_acceptance_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_aggregate_acceptance_negative_harness.py",
}

MILESTONES = readiness.MILESTONES + [
    (
        "I1 telemetry composition",
        "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac",
        "9157abb28e34df6afb16551a60cdf94a42f5b9c53172b99ba974ab576e037bd8",
        "SOURCE_ACCEPT",
    ),
    (
        "I1 fixed-path installation",
        BASE,
        "1ba875ce95187c984c36a76d9ce70973bfea47ec620028406febdca01fbe085c",
        "SOURCE_MATERIAL_ACCEPT",
    ),
]

COMPONENTS = {
    "process": {
        "commit": "1086b8d95e10514532d1c25c57956eca943b732c",
        "tree": "ec966be4429a20acd09fe93093494c40b2dcfd38",
        "archive_sha256": "48e43f5d70666049c47a26a9b6742cf2002149a6f5fb60209a4cb6a743d0182a",
    },
    "telemetry": {
        "commit": "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac",
        "tree": "5e29d320d9083a877f43a3148fff86766bd0f99e",
        "archive_sha256": "4ee845f6a9869908a3feb735419f799a2b01e09d0b6a6c31c41c4c4ab61e0cfc",
    },
    "fixed_installation": {
        "commit": BASE,
        "tree": "056227feea871b7f5be684b931f58eb1772346bb",
        "archive_sha256": "6a086f06292e0accce4655caf8fdc3354cd0cdcb58cf2eafbadb62571bc09948",
        "target_evidence_files": 21,
        "behavioral_cases": 14,
    },
}

CLOSED_SURFACES = {
    "operational_redis_db0",
    "operational_redis_db15",
    "vps_installation_or_service_start",
    "paper_provider_operational_activation",
    "finam_post_delete_send",
    "broker_dispatch",
    "runtime_live",
    "real_orders",
    "p1f_authorized",
}

MATRIX_ROWS = [
    ("I1ACC-001", "lineage", "All sixteen accepted milestones bind exact commit review digest and verdict", "REQUIRED"),
    ("I1ACC-002", "lineage", "Process telemetry and fixed-install archives bind exact commit tree and SHA-256", "REQUIRED"),
    ("I1ACC-003", "scope", "Aggregate commit changes no Rust Cargo deploy config or workflow file", "REQUIRED"),
    ("I1ACC-004", "status", "Aggregate package remains review candidate and cannot self-close I1", "REQUIRED"),
    ("I1ACC-005", "process", "Accepted process checker and 77-case negative harness pass", "REQUIRED"),
    ("I1ACC-006", "telemetry", "Accepted telemetry checker and 79-case negative harness pass", "REQUIRED"),
    ("I1ACC-007", "installation", "Fixed installation checker and 48-case negative harness pass", "REQUIRED"),
    ("I1ACC-008", "target", "All 21 target-Linux evidence files pass cross-validation", "REQUIRED"),
    ("I1ACC-009", "behavior", "Target evidence records the 14-case filesystem behavioral matrix", "REQUIRED"),
    ("I1ACC-010", "redis", "P1-c real Redis tests run only against an isolated subprocess instance", "REQUIRED"),
    ("I1ACC-011", "lifecycle", "I1A transaction committed-restart and P1-d4 inherited gates pass", "REQUIRED"),
    ("I1ACC-012", "rust", "Full strategy-runtime-core and runtime-durable-service library tests pass", "REQUIRED"),
    ("I1ACC-013", "rust", "Doctests pass for both runtime crates", "REQUIRED"),
    ("I1ACC-014", "rust", "Workspace formatting and strict all-target all-feature Clippy pass", "REQUIRED"),
    ("I1ACC-015", "evidence", "One immutable handoff binds source tree reviews gate log and target evidence", "REQUIRED"),
    ("I1ACC-016", "safety", "Handoff has no duplicate unsafe symlink secret or runtime-state member", "REQUIRED"),
    ("I1ACC-017", "closed", "Operational Redis DB0 DB15 and VPS installation or service start remain closed", "REQUIRED"),
    ("I1ACC-018", "closed", "FINAM send broker dispatch runtime-live and real orders remain closed", "REQUIRED"),
    ("I1ACC-019", "next", "P1-f remains unauthorized until separate design and operational acceptance reviews", "REQUIRED"),
    ("I1ACC-020", "review", "Only independent acceptance may close I1", "REQUIRED"),
]


class CheckFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads((root / relative).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(isinstance(value, dict), f"{relative} must contain an object")
    return value


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == expected, f"{label} key set drift")
    return value


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True
            ).splitlines()
        )
        changed |= set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify aggregate lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "aggregate changed-path inventory drift")


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    exact_keys(
        value,
        {
            "schema_version",
            "stage",
            "status",
            "accepted_milestones",
            "accepted_components",
            "aggregate_result",
            "closed_surfaces",
        },
        "aggregate inventory",
    )
    require(value["schema_version"] == 1, "schema version drift")
    require(value["stage"] == "Stage 8B-P1-e I1 aggregate acceptance", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_I1_NOT_CLOSED", "I1 self-accepted")
    milestones = value["accepted_milestones"]
    require(isinstance(milestones, list) and len(milestones) == 16, "milestone count drift")
    for index, row in enumerate(milestones):
        exact_keys(row, {"name", "commit", "review_sha256", "verdict"}, f"milestone {index}")
    actual = [
        (row.get("name"), row.get("commit"), row.get("review_sha256"), row.get("verdict"))
        for row in milestones
    ]
    require(actual == MILESTONES, "accepted milestone lineage drift")
    require(value["accepted_components"] == COMPONENTS, "accepted component binding drift")
    require(
        value["aggregate_result"]
        == {
            "all_required_gates_must_pass": True,
            "production_source_changed": False,
            "i1_closed": False,
            "independent_review_required": True,
            "next_after_acceptance": "Stage 8B-P1-f isolated operational acceptance design",
        },
        "aggregate result drift",
    )
    surfaces = exact_keys(value["closed_surfaces"], CLOSED_SURFACES, "closed surfaces")
    require(all(flag is False for flag in surfaces.values()), "operational surface opened")


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    actual = [(row["id"], row["area"], row["requirement"], row["status"]) for row in rows]
    require(actual == MATRIX_ROWS, "matrix rows drift")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    fixed = (root / FIXED_DOCUMENT).read_text()
    for fragment in (
        "REVIEW CANDIDATE — I1 NOT CLOSED",
        "It changes no Rust, Cargo,",
        "deployment unit, configuration or workflow file",
        "Only independent review",
        "P1-f isolated operational acceptance",
        "operational Redis DB0 or DB15",
    ):
        require(fragment in document, f"aggregate document fragment missing: {fragment}")
    require("active boundary is now the governance/evidence-only aggregate I1" in status, "status boundary drift")
    require("cannot self-close I1" in status, "status self-accepts I1")
    require("I1 remains open until independent aggregate review" in roadmap, "roadmap self-accepts")
    require("34 source/material and 14" in fixed and "(48 total)" in fixed, "fixed-install mutation count stale")


def validate(
    root: Path = ROOT,
    *,
    verify_lineage: bool = True,
    evidence_dir: Path | None = None,
) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_documents(root)
    if evidence_dir is not None:
        fixed_install.check_evidence(evidence_dir)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--skip-lineage", action="store_true")
    args = parser.parse_args()
    try:
        validate(verify_lineage=not args.skip_lineage, evidence_dir=args.evidence_dir)
    except (CheckFailure, fixed_install.CheckError, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1e-i1-aggregate-acceptance-check: FAIL {error}")
        return 1
    print("stage8b-p1e-i1-aggregate-acceptance-check: PASS milestones=16 rows=20 closed_surfaces=9")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
