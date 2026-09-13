#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-e I1A source handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1d4_crash_evidence_check as crash_check
import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1e_i1a_source_check as source_check


ACCEPTED_DESIGN = source_check.ACCEPTED_DESIGN
REVIEWED_SOURCE = source_check.REVIEWED_SOURCE
BRANCH = "stage8b-paper-shadow-resumption"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1e-i1a-source-evidence.json"
GATE = PREFIX + "stage8b-p1e-i1a-source-gate.txt"
CRASH_RUN_1 = PREFIX + "stage8b-p1d4-crash-replay-run-1.json"
CRASH_RUN_2 = PREFIX + "stage8b-p1d4-crash-replay-run-2.json"
CRASH_DIGEST = PREFIX + "stage8b-p1d4-crash-replay-semantic-digest.txt"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, CRASH_RUN_1, CRASH_RUN_2, CRASH_DIGEST}
REQUIRED = GENERATED | source_check.EXPECTED_CHANGED | {
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v3.md",
    "docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v3.schema.json",
    "docs/stage-8/stage8b-p1e-i1a-source-progression-v2.json",
    "docs/stage-8/stage8b-p1e-i1a-r2-semantic-fixtures-v1.json",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv",
    "docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv",
    "docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker key")
        result[key] = value
    require(
        set(result)
        == {
            "stage",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "accepted_design_ref",
            "reviewed_source_ref",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        require(not (REQUIRED - set(names)), f"missing required members: {sorted(REQUIRED - set(names))}")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")
            parts = PurePosixPath(item.filename).parts
            require(not any(part == ".env" for part in parts), f"secret path: {item.filename}")
            require(not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")), f"secret/runtime artifact: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == "Stage 8B-P1-e I1A source implementation", "stage mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_parent"] == REVIEWED_SOURCE, "source parent mismatch")
        require(marker["accepted_design_ref"] == ACCEPTED_DESIGN, "accepted design mismatch")
        require(marker["reviewed_source_ref"] == REVIEWED_SOURCE, "reviewed source mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {REVIEWED_SOURCE}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 3, "manifest version mismatch")
        require(manifest["stage"] == "Stage 8B-P1-e I1A source implementation", "manifest stage mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        require(manifest["source_tree"] == marker["source_tree"], "manifest tree mismatch")
        require(manifest["source_branch"] == BRANCH, "manifest branch mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(raw) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(raw) == entry["sha256"], f"manifest digest mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "reconstructed tree mismatch")

        evidence = json.loads(files[EVIDENCE])
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["source_parent"] == REVIEWED_SOURCE, "evidence parent mismatch")
        require(evidence["reviewed_source_ref"] == REVIEWED_SOURCE, "evidence reviewed source mismatch")
        require(evidence["accepted_design_ref"] == ACCEPTED_DESIGN, "evidence design mismatch")
        require(evidence["status"] == "SOURCE_CORRECTION_REVIEW_CANDIDATE", "evidence status mismatch")
        require(evidence["acceptance_rows"] == 81 and evidence["r2_overlay_rows"] == 8, "evidence inventory mismatch")
        require(evidence["source_negative_cases"] == 82, "negative count mismatch")
        require(evidence["p1d4_sigkill_cells"] == 105 and evidence["p1d4_sigkill_runs"] == 2, "crash inventory mismatch")
        require(evidence["changed_paths"] == sorted(source_check.EXPECTED_CHANGED), "changed path inventory mismatch")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(evidence["next_stage_authorized"] is False, "next stage opened early")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")

        for expected in (
            b"stage8b-p1e-i1a-r2-design-gate: ok",
            b"PASS stage8b-p1e-i1a-source-check",
            b"PASS stage8b-p1e-i1a-source-negative-harness 82/82",
            b"PASS stage8b-p1d4-source-negative-harness 60/60",
            b"PASS stage8b-p1d4-crash-evidence-check cells=105 runs=2",
            b"PASS stage8b-p1d4-crash-evidence-negative-harness 41/41",
            b"PASS stage8b-p1e-i1a-source-gate",
        ):
            require(expected in files[GATE], f"gate marker missing: {expected!r}")

        crash_hashes = evidence["crash_evidence_sha256"]
        for member in (CRASH_RUN_1, CRASH_RUN_2, CRASH_DIGEST):
            require(sha256(files[member]) == crash_hashes[PurePosixPath(member).name], f"crash digest mismatch: {member}")
        with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1a-safety-") as directory:
            root = Path(directory)
            mappings = {
                crash_check.BASE_MATRIX: "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv",
                crash_check.BASE_ORACLE: "docs/stage-8/stage8b-p1d4-base-evidence-oracle-v1.csv",
                crash_check.BASE_OPERATIONAL_ORACLE: "docs/stage-8/stage8b-p1d4-base-operational-evidence-oracle-v1.csv",
                crash_check.GENERATED_MATRIX: "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv",
            }
            original = tuple(mappings)
            targets = []
            for index, (_, member) in enumerate(mappings.items()):
                target = root / f"matrix-{index}.csv"
                target.write_bytes(files[member])
                targets.append(target)
            for member in (CRASH_RUN_1, CRASH_RUN_2, CRASH_DIGEST):
                (root / PurePosixPath(member).name).write_bytes(files[member])
            crash_check.BASE_MATRIX, crash_check.BASE_ORACLE, crash_check.BASE_OPERATIONAL_ORACLE, crash_check.GENERATED_MATRIX = targets
            try:
                crash_result = crash_check.check(root)
            finally:
                crash_check.BASE_MATRIX, crash_check.BASE_ORACLE, crash_check.BASE_OPERATIONAL_ORACLE, crash_check.GENERATED_MATRIX = original
        require(crash_result["cells"] == 105 and crash_result["runs"] == 2, "independent crash evidence mismatch")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "accepted_design_ref": ACCEPTED_DESIGN,
            "reviewed_source_ref": REVIEWED_SOURCE,
            "p1d4_sigkill_cells": crash_result["cells"],
            "p1d4_sigkill_runs": crash_result["runs"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1a_source_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1a-source-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1a-source-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
