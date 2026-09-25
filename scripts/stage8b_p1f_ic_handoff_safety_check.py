#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-f Ic fixed-producer handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_ic_check as source_check


STAGE = "Stage 8B-P1-f Ic fixed producers and retained high-water"
BRANCH = "stage8b-paper-shadow-resumption"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-ic-evidence.json"
GATE = PREFIX + "stage8b-p1f-ic-gate.txt"
REVIEW = PREFIX + "reviews/FINAM_P1F_IB_R2_SOURCE_ACCEPT_7c481bc_2026-09-25.md"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, REVIEW}
REQUIRED = GENERATED | source_check.ALLOWED_CHANGES


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker item")
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
            "accepted_ib_ref",
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

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_parent"] == source_check.BASE, "source parent mismatch")
        require(marker["accepted_ib_ref"] == source_check.BASE, "accepted Ib ref mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {source_check.BASE}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
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
        require(evidence["source_parent"] == source_check.BASE, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["status"] == "FIXED_PRODUCERS_REVIEW_CANDIDATE_NO_ACTIVATION", "evidence status mismatch")
        require(evidence["changed_paths"] == sorted(source_check.ALLOWED_CHANGES), "changed path drift")
        require(evidence["source_negative_cases"] == 24, "negative count mismatch")
        require(evidence["acceptance_scenarios"] == 24, "scenario count mismatch")
        require(evidence["targeted_rust_tests"] == 4, "test count mismatch")
        require(all(flag is False for flag in evidence["closed_surfaces"].values()), "surface opened")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(sha256(files[REVIEW]) == source_check.REVIEW_SHA256, "accepted review digest mismatch")

        for expected in (
            b"PASS stage8b-p1f-ic-check",
            b"PASS stage8b-p1f-ic-negative-harness 24/24",
            b"PASS positive-control",
            b"PASS nonsemantic-control",
            b"test result: ok.",
            b"PASS stage8b-p1f-ic-gate",
        ):
            require(expected in files[GATE], f"gate marker missing: {expected!r}")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_ic_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (
        OSError,
        UnicodeDecodeError,
        ValueError,
        KeyError,
        TypeError,
        zipfile.BadZipFile,
        json.JSONDecodeError,
    ) as error:
        print(f"stage8b-p1f-ic-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-ic-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
