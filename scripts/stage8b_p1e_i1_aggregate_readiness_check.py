#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-e I1 aggregate-readiness package."""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
ACCEPTED_SOURCE = "1086b8d95e10514532d1c25c57956eca943b732c"
ACCEPTED_TREE = "ec966be4429a20acd09fe93093494c40b2dcfd38"
DOCUMENT = "docs/stage-8/stage8b-p1e-i1-aggregate-closure-readiness.md"
INVENTORY = "docs/stage-8/stage8b-p1e-i1-aggregate-closure-inventory.json"
MATRIX = "docs/stage-8/stage8b-p1e-i1-aggregate-closure-acceptance-matrix.csv"
PROCESS_MATRIX = "docs/stage-8/stage8b-p1e-i1-process-supervision-matrix.md"
STATUS = "docs/current-status.md"
SUPERVISOR = "crates/runtime-durable-service/src/stage8b_p1_supervisor.rs"
PROCESS = "crates/runtime-durable-service/src/stage8b_p1e_process.rs"
BINARY = "crates/runtime-durable-service/src/bin/stage8b-p1-paper-supervisor.rs"

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    PROCESS_MATRIX,
    STATUS,
    "scripts/make_stage8b_p1e_i1_aggregate_readiness_handoff.py",
    "scripts/stage8b_p1e_i1_aggregate_readiness_check.py",
    "scripts/stage8b_p1e_i1_aggregate_readiness_gate.sh",
    "scripts/stage8b_p1e_i1_aggregate_readiness_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_aggregate_readiness_negative_harness.py",
    "scripts/stage8b_p1e_i1_process_supervision_check.py",
    "scripts/stage8b_p1e_i1_process_supervision_negative_harness.py",
}

MILESTONES = [
    ("I0 latch-aware source seam", "afda87a98ae3b0d0f4506292a162310f4b9068c0", "de61700f1fa13124ba4b73bf1e43d25d876528b0e7f5817dde15acbbb401745d", "ACCEPTED"),
    ("I1 supervisor foundation", "37088964e50c0ceb4d82887a30c32103110749b0", "28b41968954924cf28f2dd27cdcc524a3b00285888be3baeaca89ebf1dbe3485", "ACCEPTED"),
    ("I1A signed schedule design", "aa24e840ed8b7d18c80be6f1fdd8f50facf5b6d4", "38f85a38452a0650713ced0c4569793509d4e144ed2a3bd58b3f82efd3936e29", "ACCEPTED"),
    ("I1A signed schedule source", "8360c4701b6abbe75ced988cf8dd2d74487e1846", "5ccdc3e7beafbc35090ec9e7f406cef9c330da4e15db8d928224784ae543a2e5", "SOURCE_ACCEPT"),
    ("I1 first boot source", "4d7ee64730ffba72d21f9b51a2c65e7d1b8a2721", "d1baa8fe5ca0e6acc37948deceb120bde9906e32aeb14adf75cf7981e80737b1", "SOURCE_ACCEPT"),
    ("I1 first boot governance", "21eaf01916f2da5eaacb191b4d7339a8101070ad", "cbd70851c19b7c7d502acb685566f077211c528a213f8ba70471f9f5be3580b2", "ACCEPTED"),
    ("I1 transaction V5", "5e2e157e032406fdbb9047c33c641f5973514504", "625b4a5508494d8d68d9f7259bbcf9a2c40eb40433f29f540e141c8d17668fd7", "SOURCE_ACCEPT"),
    ("I1 pre-seal recovery", "a655da96ace23eb61d89642f63c49e5275ff98bd", "777ccbb2c3ad4377c2d5d8296b53387da622492c1a105e8a120472ac48ba8ddc", "SOURCE_ACCEPT"),
    ("I1 generated Market composition", "ff6639ef45f1504decb4be2f6d8981bb3ba172e7", "668b2be4bae317adea3b8cf3741ade3384cf10414a45f45d32f44a929af21639", "SOURCE_ACCEPT"),
    ("I1 Cancel composition", "cc1f02c19f35bb06136db2521538c6929f9dc101", "5fd0b3220c3c680ac41e597d54e639cef9709879aea51cab0d5002c49c583a86", "SOURCE_ACCEPT"),
    ("I1 Day-expiry composition", "a66793885e425465e5c4f49426748333b1cc448b", "abab1053624b8f14a262045a3a7fc8b2b268cb62f6650254d6c82f93ef380e0b", "SOURCE_ACCEPT"),
    ("I1 committed Cancel and Day-expiry recovery", "efe56a9af6272f13b2d87f4ad14a2709c605cd42", "2ee294118e2e4e92d1ffc5a616a5c392fdc67c8ca8f7dd648b81b143e585d413", "SOURCE_ACCEPT"),
    ("I1 committed owner loop", "e2ce44206e49c3927fbb42bb252c25eecd1000de", "467ead8c60e605b362cca07ca4fe56c8e10ef92318f0782ea18549fbdbb0972f", "SOURCE_ACCEPT"),
    ("I1 process supervision", ACCEPTED_SOURCE, "df7dc27aa0ed3981ed5c6f3271a0f30f6073b7ef3ac36cb694520b243b102f4a", "SOURCE_ACCEPT"),
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
            ["git", "merge-base", "--is-ancestor", ACCEPTED_SOURCE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        changed = set(
            subprocess.check_output(
                ["git", "diff", "--name-only", ACCEPTED_SOURCE, "--"],
                cwd=root,
                text=True,
            ).splitlines()
        )
        untracked = set(
            subprocess.check_output(
                ["git", "ls-files", "--others", "--exclude-standard"],
                cwd=root,
                text=True,
            ).splitlines()
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"cannot verify accepted source lineage: {error}") from error
    require(changed | untracked == ALLOWED_CHANGES, "aggregate package changed-path inventory drift")


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    exact_keys(
        value,
        {
            "schema_version", "stage", "status", "accepted_source",
            "accepted_milestones", "implementation_inventory",
            "remaining_slices", "closed_surfaces",
        },
        "inventory",
    )
    require(value["schema_version"] == 1, "schema version drift")
    require(value["stage"] == "Stage 8B-P1-e I1 aggregate closure readiness", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_I1_NOT_CLOSED", "I1 self-accepted")
    require(
        value["accepted_source"]
        == {
            "commit": ACCEPTED_SOURCE,
            "tree": ACCEPTED_TREE,
            "archive": "moex-trading-project-1086b8d-stage8b-p1e-i1-process-supervision.zip",
            "archive_sha256": "48e43f5d70666049c47a26a9b6742cf2002149a6f5fb60209a4cb6a743d0182a",
            "review_sha256": "df7dc27aa0ed3981ed5c6f3271a0f30f6073b7ef3ac36cb694520b243b102f4a",
            "verdict": "SOURCE_ACCEPT",
        },
        "accepted source binding drift",
    )
    milestones = value["accepted_milestones"]
    require(isinstance(milestones, list) and len(milestones) == len(MILESTONES), "milestone count drift")
    actual = [(row.get("name"), row.get("commit"), row.get("review_sha256"), row.get("verdict")) for row in milestones]
    require(actual == MILESTONES, "accepted milestone lineage drift")

    implementation = exact_keys(
        value["implementation_inventory"],
        {
            "process_source", "fixed_binary_and_cli",
            "fixed_path_config_and_credential_loading",
            "telemetry_contract_and_dtos", "telemetry_redis_write_primitive",
            "production_telemetry_composition", "p1e_installation_transaction",
            "p1e_systemd_material", "target_linux_installation_evidence",
            "aggregate_i1_acceptance",
        },
        "implementation inventory",
    )
    require(implementation["process_source"] == "ACCEPTED", "process source status drift")
    require(implementation["fixed_binary_and_cli"] == "IMPLEMENTED_SOURCE_ACCEPTED", "binary status drift")
    for key in ("telemetry_contract_and_dtos", "telemetry_redis_write_primitive"):
        require(implementation[key] == "IMPLEMENTED_FOUNDATION_ONLY", f"{key} overclaimed")
    for key in (
        "production_telemetry_composition", "p1e_installation_transaction",
        "p1e_systemd_material", "target_linux_installation_evidence",
        "aggregate_i1_acceptance",
    ):
        require(implementation[key] == "OPEN", f"{key} must remain open")

    require(
        value["remaining_slices"]
        == [
            {"order": 1, "name": "telemetry composition", "status": "OPEN", "operational_activation_allowed": False},
            {"order": 2, "name": "fixed-path installation and systemd material", "status": "OPEN", "operational_activation_allowed": False},
            {"order": 3, "name": "aggregate I1 acceptance", "status": "OPEN", "operational_activation_allowed": False},
        ],
        "remaining slice order drift",
    )
    surfaces = exact_keys(
        value["closed_surfaces"],
        {
            "operational_redis_db0", "operational_redis_db15",
            "vps_installation_or_service_start", "paper_provider_operational_activation",
            "finam_post_delete_send", "broker_dispatch", "runtime_live",
            "real_orders", "p1f_authorized",
        },
        "closed surfaces",
    )
    require(all(flag is False for flag in surfaces.values()), "operational surface opened")


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read acceptance matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"I1AGG-{index:03}" for index in range(1, 16)], "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row introduced")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    process_matrix = (root / PROCESS_MATRIX).read_text()
    status = (root / STATUS).read_text()
    for fragment in (
        "REVIEW CANDIDATE — I1 NOT CLOSED",
        "production process loop does not yet construct and publish",
        "no tracked P1-e service unit",
        "Telemetry composition",
        "Fixed-path installation and systemd material",
        "Aggregate I1 acceptance",
        "P1-f or any operational paper activation remains downstream",
    ):
        require(fragment in document, f"aggregate document fragment missing: {fragment}")
    require("and crashes at\nthat pre-effect frontier" not in process_matrix, "controlled restart still called a crash")
    for fragment in (
        "controlled owner drop/restart at that durable V4 pre-effect frontier",
        "initial adoption, committed V4 before the provider effect, post-truth/XACK, and",
        "repeat admission after `AlreadyAcknowledged`",
        "not an OS-SIGKILL witness",
    ):
        require(fragment in process_matrix, f"process evidence clarification missing: {fragment}")
    require("I1 aggregate closure readiness package" in status, "current status aggregate entry missing")
    require("I1 remains open" in status, "current status overclaims closure")


def validate_source_boundary(root: Path) -> None:
    require((root / BINARY).is_file(), "accepted supervisor binary missing")
    supervisor = (root / SUPERVISOR).read_text()
    process = (root / PROCESS).read_text()
    for token in (
        "pub async fn publish_health(",
        "pub async fn publish_readiness(",
        '.arg("NOMKSTREAM")',
        "STAGE8B_P1E_TELEMETRY_RETENTION",
        "pub fn stage8b_p1e_readiness_v1(",
    ):
        require(token in supervisor, f"telemetry foundation missing: {token}")
    for token in (".publish_health(", ".publish_readiness(", "stage8b_p1e_telemetry_envelope_v1("):
        require(token not in process, f"production telemetry composition unexpectedly present: {token}")
    deploy = root / "deploy"
    names = [path.name for path in deploy.rglob("*") if path.is_file()] if deploy.exists() else []
    require("stage8b-p1-paper-supervisor.service" not in names, "P1-e systemd service exists but is marked open")


def validate(root: Path = ROOT, verify_lineage: bool = True) -> None:
    if verify_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_documents(root)
    validate_source_boundary(root)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--skip-lineage", action="store_true")
    args = parser.parse_args()
    try:
        validate(args.root.resolve(), verify_lineage=not args.skip_lineage)
    except (CheckFailure, OSError, UnicodeDecodeError) as error:
        print(f"stage8b-p1e-i1-aggregate-readiness-check: FAIL {error}")
        return 1
    print(f"stage8b-p1e-i1-aggregate-readiness-check: PASS milestones={len(MILESTONES)} rows=15 open_slices=3")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
