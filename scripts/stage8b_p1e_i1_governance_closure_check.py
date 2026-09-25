#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e I1 governance closure."""

from __future__ import annotations

import csv
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "a9bcd940635b62c2a13f8d378453e6ca21511e30"
DOCUMENT = "docs/stage-8/stage8b-p1e-i1-governance-closure.md"
INVENTORY = "docs/stage-8/stage8b-p1e-i1-governance-closure.json"
MATRIX = "docs/stage-8/stage8b-p1e-i1-governance-closure-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    STATUS,
    ROADMAP,
    "scripts/make_stage8b_p1e_i1_governance_closure_handoff.py",
    "scripts/stage8b_p1e_i1_governance_closure_check.py",
    "scripts/stage8b_p1e_i1_governance_closure_gate.sh",
    "scripts/stage8b_p1e_i1_governance_closure_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_governance_closure_negative_harness.py",
}

ACCEPTED_CANDIDATE = {
    "commit": BASE,
    "parent": "7f2e876c4cad7a3a4a0fa10a1eb5202e58202d2f",
    "tree": "2d4196abf22d95af8e794bd0ae9c360794e44802",
    "archive": "moex-trading-project-a9bcd94-stage8b-p1e-i1-aggregate-acceptance.zip",
    "archive_sha256": "5dfb664d86f37c00441c40b0d622db1fc87f6559812ab726d2a076c1f6bac81d",
}
REVIEW = {
    "file": "FINAM_I1_AGGREGATE_ACCEPTANCE_a9bcd94_2026-09-24.md",
    "sha256": "c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54",
    "verdict": "AGGREGATE_I1_ACCEPT",
}
SCOPE = {
    "source_accepted": True,
    "installation_systemd_material_accepted": True,
    "retained_evidence_accepted": True,
    "production_source_changed_by_closure": False,
}
TRANSITION = {
    "i1_closed": True,
    "p1f_design_authorized": True,
    "p1f_implementation_authorized": False,
    "p1f_operational_activation_authorized": False,
}
CLOSED_SURFACES = {
    "installation_or_service_start",
    "operational_redis_db15",
    "operational_redis_db0",
    "vps_deployment",
    "paper_provider_execution",
    "finam_post_delete_send",
    "broker_dispatch",
    "runtime_live",
    "real_orders",
}
MATRIX_ROWS = [
    ("I1CLOSE-001", "lineage", "Closure parent is the independently accepted aggregate candidate", "REQUIRED"),
    ("I1CLOSE-002", "identity", "Accepted commit parent tree archive and SHA-256 are exact", "REQUIRED"),
    ("I1CLOSE-003", "review", "External verdict filename digest and verdict are exact", "REQUIRED"),
    ("I1CLOSE-004", "scope", "Closure changes no production source Cargo deploy config or workflow", "REQUIRED"),
    ("I1CLOSE-005", "status", "I1 is closed only by the independent aggregate verdict", "REQUIRED"),
    ("I1CLOSE-006", "next", "P1-f design is authorized", "REQUIRED"),
    ("I1CLOSE-007", "next", "P1-f implementation remains unauthorized", "REQUIRED"),
    ("I1CLOSE-008", "next", "P1-f operational activation remains unauthorized", "REQUIRED"),
    ("I1CLOSE-009", "closed", "Installation service start and VPS deployment remain closed", "REQUIRED"),
    ("I1CLOSE-010", "closed", "Operational Redis DB15 and DB0 remain closed", "REQUIRED"),
    ("I1CLOSE-011", "closed", "Paper-provider execution remains closed", "REQUIRED"),
    ("I1CLOSE-012", "closed", "FINAM send broker dispatch runtime-live and real orders remain closed", "REQUIRED"),
    ("I1CLOSE-013", "evidence", "Immutable handoff binds source tree verdict and gate log", "REQUIRED"),
    ("I1CLOSE-014", "safety", "Handoff rejects duplicate unsafe symlink secret and runtime-state members", "REQUIRED"),
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
            subprocess.check_output(["git", "diff", "--name-only", BASE, "--"], cwd=root, text=True).splitlines()
        )
        changed |= set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"], cwd=root, text=True
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify closure lineage: {error}") from error
    require(changed == ALLOWED_CHANGES, "closure changed-path inventory drift")


def validate(root: Path = ROOT, *, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    try:
        inventory = json.loads((root / INVENTORY).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read closure inventory: {error}") from error
    require(
        set(inventory)
        == {"schema_version", "stage", "status", "accepted_candidate", "independent_review", "scope", "transition", "closed_surfaces"},
        "closure inventory keys drift",
    )
    require(inventory["schema_version"] == 1, "schema drift")
    require(inventory["stage"] == "Stage 8B-P1-e I1 governance closure", "stage drift")
    require(inventory["status"] == "CLOSED_ACCEPTED", "closure status drift")
    require(inventory["accepted_candidate"] == ACCEPTED_CANDIDATE, "accepted candidate drift")
    require(inventory["independent_review"] == REVIEW, "review binding drift")
    require(inventory["scope"] == SCOPE, "closure scope drift")
    require(inventory["transition"] == TRANSITION, "transition authority drift")
    surfaces = inventory["closed_surfaces"]
    require(isinstance(surfaces, dict) and set(surfaces) == CLOSED_SURFACES, "closed surface inventory drift")
    require(all(value is False for value in surfaces.values()), "operational surface opened")

    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read closure matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    actual = [(row["id"], row["area"], row["requirement"], row["status"]) for row in rows]
    require(actual == MATRIX_ROWS, "closure matrix drift")

    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for fragment in (
        "Status: **CLOSED / ACCEPTED**",
        BASE,
        REVIEW["sha256"],
        "authorizes only the preparation",
        "No operational action is performed",
    ):
        require(fragment in document, f"closure document fragment missing: {fragment}")
    require("## Active Stage 8B-P1-f design boundary" in status, "current status boundary drift")
    require("Design authority is not activation authority" in status, "status activation ambiguity")
    require("all three I1 closure steps are complete" in roadmap, "roadmap closure missing")
    require("does not authorize installation" in roadmap, "roadmap opened activation")


def main() -> int:
    try:
        validate()
    except (CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1e-i1-governance-closure-check: FAIL {error}")
        return 1
    print("stage8b-p1e-i1-governance-closure-check: PASS rows=14 closed_surfaces=9 p1f_design=true activation=false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
