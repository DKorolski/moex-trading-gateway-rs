#!/usr/bin/env python3
"""Fail-closed checker for Stage 8B-P1-f Ie aggregate source closure."""

from __future__ import annotations

import csv
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
BASE = "512db6e6e652a2b0a15be7b6dcb72b96e231950d"
BRANCH = "stage8b-paper-shadow-resumption"
DOCUMENT = "docs/stage-8/stage8b-p1f-ie-aggregate-source-closure.md"
INVENTORY = "docs/stage-8/stage8b-p1f-ie-aggregate-source-closure.json"
MATRIX = "docs/stage-8/stage8b-p1f-ie-acceptance-matrix.csv"
STATUS = "docs/current-status.md"
ROADMAP = "docs/roadmap.md"
LINKED = "scripts/stage8b_p1f_ie_linked_local_composition.sh"
CHECKER = "scripts/stage8b_p1f_ie_check.py"
NEGATIVE = "scripts/stage8b_p1f_ie_negative_harness.py"
GATE = "scripts/stage8b_p1f_ie_gate.sh"
SAFETY = "scripts/stage8b_p1f_ie_handoff_safety_check.py"
BUILDER = "scripts/make_stage8b_p1f_ie_handoff.py"

ACCEPTED_SOURCES = (
    {
        "slice": "P1F-Ia",
        "commit": "9be356b04a38e627337ed148ccc9fbdaebae8d4a",
        "tree": "98ae44fd82c0dfcd342d06487a4fbe219d455062",
        "review_file": "FINAM_P1F_IA_R3_SOURCE_ACCEPT_9be356b_2026-09-25.md",
        "review_sha256": "db684cb6ba10cb951801cc917c317d2817c99c3f10874d5a1d959c053f6ebb60",
    },
    {
        "slice": "P1F-Ib",
        "commit": "7c481bc60699b514b016e8dffe62eb9ca462a100",
        "tree": "9b3feb5a1f4dde69bf17c910f95c3ca2b096c8b9",
        "review_file": "FINAM_P1F_IB_R2_SOURCE_ACCEPT_7c481bc_2026-09-25.md",
        "review_sha256": "098a24fc87871968bc6e7b0a77deefffd403bc56d997d21af18e1869750acf7e",
    },
    {
        "slice": "P1F-Ic",
        "commit": "5c2656fbe8691da256b5380dd16ce6f6b6aa1fa8",
        "tree": "e18034883ea05bb6c0259f24043b29aa2405555a",
        "review_file": "FINAM_P1F_IC_SOURCE_ACCEPT_5c2656f_2026-09-26.md",
        "review_sha256": "ee69d58bc70288f447a9ab880d2a2eec01fefdfe6f86ce3be603eb7f5a1b30b3",
    },
    {
        "slice": "P1F-Id",
        "commit": BASE,
        "tree": "8235ce38f87e35b7258d89eb22ee0ee1e0c0e8c9",
        "review_file": "FINAM_P1F_ID_R2_SOURCE_ACCEPT_512db6e_2026-09-26.md",
        "review_sha256": "3208f8472f7e9a0109f41187458f78bfdab780029b4cc6be610640e5c652b1b9",
    },
)

ALLOWED_CHANGES = {
    DOCUMENT,
    INVENTORY,
    MATRIX,
    STATUS,
    ROADMAP,
    LINKED,
    CHECKER,
    NEGATIVE,
    GATE,
    SAFETY,
    BUILDER,
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


def read_json(root: Path, relative: str) -> dict[str, Any]:
    try:
        value = json.loads(
            (root / relative).read_text(), object_pairs_hook=strict_object
        )
    except (OSError, json.JSONDecodeError) as error:
        raise CheckFailure(f"cannot read {relative}: {error}") from error
    require(type(value) is dict, f"{relative} must be an object")
    return value


def git(root: Path, *args: str) -> str:
    try:
        return subprocess.check_output(("git", *args), cwd=root, text=True).strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"git verification failed: {error}") from error


def validate_lineage(root: Path) -> None:
    try:
        subprocess.run(
            ["git", "merge-base", "--is-ancestor", BASE, "HEAD"],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise CheckFailure(f"Id baseline is not an ancestor: {error}") from error
    changed = set(git(root, "diff", "--name-only", BASE, "--").splitlines())
    changed |= set(
        git(root, "ls-files", "--others", "--exclude-standard").splitlines()
    )
    require(changed == ALLOWED_CHANGES, f"Ie changed-path drift: {sorted(changed ^ ALLOWED_CHANGES)}")
    prior = None
    for source in ACCEPTED_SOURCES:
        commit = source["commit"]
        require(git(root, "rev-parse", f"{commit}^{{commit}}") == commit, f"missing accepted commit: {commit}")
        require(git(root, "rev-parse", f"{commit}^{{tree}}") == source["tree"], f"accepted tree drift: {source['slice']}")
        if prior is not None:
            try:
                subprocess.run(
                    ["git", "merge-base", "--is-ancestor", prior, commit],
                    cwd=root,
                    check=True,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.PIPE,
                )
            except subprocess.CalledProcessError as error:
                raise CheckFailure(f"accepted lineage drift: {source['slice']}") from error
        prior = commit


def validate_inventory(root: Path) -> None:
    value = read_json(root, INVENTORY)
    require(
        set(value)
        == {
            "schema_version",
            "stage",
            "status",
            "accepted_sources",
            "lineage",
            "aggregate_composition",
            "evidence",
            "closed_surfaces",
            "next_after_acceptance",
        },
        "Ie inventory key set drift",
    )
    require(value["schema_version"] == 1 and type(value["schema_version"]) is int, "schema drift")
    require(value["stage"] == "Stage 8B-P1-f Ie aggregate source closure", "stage drift")
    require(value["status"] == "REVIEW_CANDIDATE_AGGREGATE_SOURCE_CLOSURE_NO_ACTIVATION", "Ie self-accepted")
    require(value["accepted_sources"] == list(ACCEPTED_SOURCES), "accepted source authority drift")
    require(value["lineage"] == ["P1F-Ia", "P1F-Ib", "P1F-Ic", "P1F-Id", "P1F-Ie"], "lineage inventory drift")
    composition = value["aggregate_composition"]
    require(
        composition
        == {
            "linked_local_script": LINKED,
            "existing_fixture_count": 9,
            "new_production_rust_files": 0,
            "new_cargo_changes": 0,
            "isolated_local_redis_only": True,
        },
        "aggregate composition drift",
    )
    require(
        value["evidence"]
        == {
            "checker": CHECKER,
            "negative_harness": NEGATIVE,
            "aggregate_gate": GATE,
            "handoff_builder": BUILDER,
        },
        "evidence inventory drift",
    )
    closed = value["closed_surfaces"]
    require(type(closed) is dict and len(closed) == 9, "closed surface inventory drift")
    require(all(flag is False for flag in closed.values()), "operational surface opened")
    require(
        value["next_after_acceptance"]
        == "P1F-O0 immutable read-only target preflight; operational activation remains closed",
        "next boundary drift",
    )


def validate_matrix(root: Path) -> None:
    try:
        with (root / MATRIX).open(newline="") as handle:
            rows = list(csv.DictReader(handle))
    except OSError as error:
        raise CheckFailure(f"cannot read matrix: {error}") from error
    require(rows and list(rows[0]) == ["id", "area", "requirement", "status"], "matrix header drift")
    require([row["id"] for row in rows] == [f"P1FIE-{index:03}" for index in range(1, 21)], "matrix row inventory drift")
    require(all(row["status"] == "REQUIRED" for row in rows), "optional matrix row")


def validate_linked_proof(root: Path) -> None:
    script = (root / LINKED).read_text()
    required_tests = (
        "o2_materialization_finalizes_only_source_hash_and_replays_exactly",
        "local_supervision_starts_after_admission_and_stops_on_sigterm",
        "fixed_o3_o4_schedule_uses_one_retained_sequence_and_revision",
        "id_linked_real_redis_response_loss_restarts_prepared_without_duplicate",
        "resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit",
        "p1c_journal_ahead_reclaims_real_pel_before_reconstructing_s1",
        "p1f_id_process_supervision_retains_failed_attach_audit_after_early_owner_return",
        "p1f_id_process_supervision_retains_audit_after_owner_abort",
        "production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly",
    )
    for test in required_tests:
        require(script.count(test) == 1, f"linked fixture missing or duplicated: {test}")
    require("--exact --test-threads=1" in script, "linked fixtures are not exact and serial")
    require("PASS stage8b-p1f-ie-linked-local-composition steps=9 operational=false" in script, "linked proof marker drift")
    for forbidden in ("ssh ", "docker ", "redis-cli", "curl ", "FINAM_TOKEN", "POST ", "DELETE "):
        require(forbidden not in script, f"operational command in linked proof: {forbidden}")


def validate_documents(root: Path) -> None:
    document = (root / DOCUMENT).read_text()
    status = (root / STATUS).read_text()
    roadmap = (root / ROADMAP).read_text()
    for source in ACCEPTED_SOURCES:
        require(source["commit"] in document, f"document commit missing: {source['slice']}")
        require(source["review_sha256"] in document, f"document review missing: {source['slice']}")
    for fragment in (
        "REVIEW_CANDIDATE_AGGREGATE_SOURCE_CLOSURE_NO_ACTIVATION",
        "nine existing",
        "not a claim",
        "P1F-O0 immutable read-only target preflight",
    ):
        require(fragment in document, f"Ie document fragment missing: {fragment}")
    require("The active source candidate is now P1F-Ie aggregate source closure" in status, "current status boundary drift")
    require("P1F-Ie is the active and final\nsource candidate" in roadmap, "roadmap boundary drift")
    require("P1F-O0 stays closed" in roadmap, "roadmap O0 boundary drift")


def validate(root: Path = ROOT, *, check_lineage: bool = True) -> None:
    if check_lineage:
        validate_lineage(root)
    validate_inventory(root)
    validate_matrix(root)
    validate_linked_proof(root)
    validate_documents(root)


def main() -> int:
    try:
        validate()
    except (CheckFailure, OSError, UnicodeDecodeError, KeyError, TypeError) as error:
        print(f"stage8b-p1f-ie-check: FAIL {error}")
        return 1
    print("PASS stage8b-p1f-ie-check accepted_sources=4 rows=20 linked_steps=9 production_changes=0 operational=false")
    return 0


if __name__ == "__main__":
    sys.exit(main())
