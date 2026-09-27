#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P1-f O0 read-only preflight."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import stage8b_p1f_o0_collect as collect


ROOT = Path(__file__).resolve().parents[1]
BASE = "3a46a460ea4bd5c85c5befd036510c580941a265"
BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_IE = "940377ab2bd406be31547200ca0b8cc3bb0f3e22"
IE_REVIEW = "FINAM_P1F_IE_SOURCE_ACCEPT_940377a_2026-09-27.md"
IE_REVIEW_SHA256 = "94b224e56c30e4ad54b5db6d0d744b1fd7fbf06897e58af9be3382a3c9d5af96"

DOCUMENT = "docs/stage-8/stage8b-p1f-o0-target-preflight.md"
EVIDENCE = "docs/stage-8/stage8b-p1f-o0-target-preflight.json"
MATRIX = "docs/stage-8/stage8b-p1f-o0-acceptance-matrix.csv"
RAW = "reports/stage8b/stage8b-p1f-o0-readonly-probe.txt"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
PROBE = "scripts/stage8b_p1f_o0_readonly_probe.sh"
COLLECTOR = "scripts/stage8b_p1f_o0_collect.py"
CHECKER = "scripts/stage8b_p1f_o0_check.py"
NEGATIVE = "scripts/stage8b_p1f_o0_negative_harness.py"
GATE = "scripts/stage8b_p1f_o0_gate.sh"
BUILDER = "scripts/make_stage8b_p1f_o0_handoff.py"
SAFETY = "scripts/stage8b_p1f_o0_handoff_safety_check.py"

ALLOWED_CHANGES = {
    DOCUMENT, EVIDENCE, MATRIX, RAW, STATUS, ROADMAP, PROBE, COLLECTOR,
    CHECKER, NEGATIVE, GATE, BUILDER, SAFETY,
}
EXPECTED_PATHS = [
    "/usr/local/libexec/moex/stage8b-p1-paper-supervisor",
    "/etc/moex-finam-p1-paper",
    "/var/lib/moex-finam-p1-paper",
    "/var/lib/moex-finam-p1-paper-control",
    "/etc/systemd/system/moex-finam-p1-paper.service",
    "/etc/systemd/system/moex-finam-p1-paper-bootstrap.service",
]
EXPECTED_SERVICES = {
    "moex-finam-paper-runtime.service": {
        "fragment_sha256": "8f8f2854191887a75317869c8e6ff3c8edd4197c1ae594fa93c0b56fc35585fc",
        "execstart_sha256": "cd6438997ac9442fa9305614f08897e6e89d9c017b0fdcf5081a7fa7b72d525b",
    },
    "moex-finam-paper-ws.service": {
        "fragment_sha256": "e61591a064838725a2fc15ee089fb862bd62df97f8995549773f9f3ee65cf1b9",
        "execstart_sha256": "2336dae9c4e27825cd4efd80395ce07f5509aca4c1fb58fc95057ace96fba3ae",
    },
}


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


def read_json(root: Path) -> dict[str, Any]:
    try:
        value = json.loads((root / EVIDENCE).read_text(), object_pairs_hook=strict_object)
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read evidence: {error}") from error
    require(type(value) is dict, "evidence must be an object")
    return value


def git(root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(("git", *args), cwd=root, text=True).strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"git verification failed: {error}") from error


def validate_lineage(root: Path) -> None:
    head = git(root, "rev-parse", "HEAD")
    if head != BASE:
        require(git(root, "rev-parse", "HEAD^") == BASE, "O0 commit must be a direct child of Ie closure")
    require(git(root, "branch", "--show-current") == BRANCH, "branch drift")
    changed = set(git(root, "diff", "--name-only", BASE, "--").splitlines())
    changed |= set(git(root, "ls-files", "--others", "--exclude-standard").splitlines())
    require(changed == ALLOWED_CHANGES, f"O0 changed-path drift: {sorted(changed ^ ALLOWED_CHANGES)}")


def validate_probe(root: Path) -> None:
    text = (root / PROBE).read_text()
    require('target="root@45.150.11.252"' in text, "probe target drift")
    require("StrictHostKeyChecking=yes" in text and "BatchMode=yes" in text, "SSH fail-closed options missing")
    require(text.count("remote_mutation_performed false") == 1, "mutation marker drift")
    forbidden = (
        r"\b(?:sudo|scp|rsync|curl|wget|docker|podman|useradd|groupadd|mkdir|install|chmod|chown)\b",
        r"systemctl\s+(?:start|stop|restart|reload|enable|disable|daemon-reload)",
        r"redis-cli[^\n]*(?:\bSET\b|\bXADD\b|\bXACK\b|\bXDEL\b|\bDEL\b|\bFLUSH\w*\b)",
        r"\b(?:POST|DELETE)\b",
    )
    remote = text.split("<<'REMOTE'", 1)[1]
    for pattern in forbidden:
        require(re.search(pattern, remote, re.IGNORECASE) is None, f"mutating probe surface: {pattern}")


def validate_evidence(root: Path) -> None:
    raw = (root / RAW).read_bytes()
    values = collect.parse(raw)
    expected = collect.build(values, raw)
    actual = read_json(root)
    require(actual == expected, "normalized evidence is not an exact raw-probe rebuild")
    require(actual["source_baseline"] == BASE, "baseline drift")
    require(actual["status"] == "REVIEW_CANDIDATE_READY_FOR_O1_REVIEW_NO_MUTATION", "self-acceptance/status drift")
    require(actual["all_required_checks_passed"] is True, "required check failed")
    require(actual["remote_mutation_performed"] is False, "remote mutation declared")
    require(actual["platform"]["ntp_synchronized"] == "yes", "NTP is not synchronized")
    require(actual["redis"]["listener_sha256"] == "b417199e5def64d046d05097b8e2faaed813a1e45bb2a2ea2d544a674c08d4a1", "Redis listener evidence drift")
    require(actual["redis"]["db15_size"] == 0, "DB15 is not empty")
    require(actual["redis"]["db15_keyspace_sha256"] == collect.EMPTY_SHA256, "DB15 digest drift")
    require(type(actual["redis"]["db0_size"]) is int and actual["redis"]["db0_size"] >= 0, "invalid DB0 observation")
    require(re.fullmatch(r"[0-9a-f]{64}", actual["redis"]["db0_keyspace_sha256"]) is not None, "invalid DB0 hash")
    require([item["path"] for item in actual["p1_paths"]] == EXPECTED_PATHS, "P1 path inventory drift")
    require(all(item["present"] is False for item in actual["p1_paths"]), "P1 path exists")
    require(actual["p1_service_user_present"] is False, "P1 user exists")
    require(set(actual["p0_services"]) == set(EXPECTED_SERVICES), "P0 service inventory drift")
    for name, hashes in EXPECTED_SERVICES.items():
        service = actual["p0_services"][name]
        require({key: service[key] for key in hashes} == hashes, f"P0 identity drift: {name}")
        require((service["load_state"], service["active_state"], service["sub_state"], service["unit_file_state"]) == ("loaded", "active", "running", "enabled"), f"P0 health drift: {name}")
    require(type(actual["closed_surfaces"]) is dict and len(actual["closed_surfaces"]) == 8, "closed surface inventory drift")
    require(all(value is False for value in actual["closed_surfaces"].values()), "operational surface opened")
    require(actual["raw_probe_sha256"] == hashlib.sha256(raw).hexdigest(), "raw digest drift")


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FO0-{index:03}" for index in range(1, 21)], "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row")


def validate_docs(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for value in (BASE, ACCEPTED_IE, IE_REVIEW_SHA256, "REVIEW_CANDIDATE_READY_FOR_O1_REVIEW_NO_MUTATION"):
        require(value in document, f"O0 document binding missing: {value}")
    require("O0 does not authorize O1" in document, "O1 boundary missing")
    require("P1F-O0 has now produced an immutable read-only target" in status, "status not updated")
    require("P1F-O1 provisioning and every activation surface remain closed" in status, "status boundary drift")
    require("P1F-O0 has an\nimmutable read-only target-preflight candidate" in roadmap, "roadmap not updated")
    require("cannot authorize O1 without independent acceptance" in roadmap, "roadmap boundary drift")


def validate(root: Path = ROOT, check_lineage: bool = True) -> None:
    if check_lineage:
        validate_lineage(root)
    validate_probe(root)
    validate_evidence(root)
    validate_matrix(root)
    validate_docs(root)


def main() -> None:
    try:
        validate()
    except (CheckFailure, OSError, UnicodeDecodeError, ValueError, KeyError, TypeError) as error:
        print(f"stage8b-p1f-o0-check: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-o0-check rows=20 target=exact db15=empty remote_mutation=false o1_authorized=false")


if __name__ == "__main__":
    main()
