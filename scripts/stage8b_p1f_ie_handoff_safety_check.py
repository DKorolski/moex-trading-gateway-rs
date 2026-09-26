#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-f Ie aggregate closure handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_ie_check as source_check


STAGE = "Stage 8B-P1-f Ie aggregate source closure"
BRANCH = source_check.BRANCH
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-ie-evidence.json"
GATE = PREFIX + "stage8b-p1f-ie-gate.txt"
REVIEWS = {
    PREFIX + "reviews/" + item["review_file"]: item["review_sha256"]
    for item in source_check.ACCEPTED_SOURCES
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE} | set(REVIEWS)
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
            "accepted_id_ref",
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
        require(marker["accepted_id_ref"] == source_check.BASE, "accepted Id mismatch")
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
        require(evidence["status"] == "AGGREGATE_SOURCE_CLOSURE_REVIEW_CANDIDATE_NO_ACTIVATION", "evidence status mismatch")
        require(evidence["accepted_sources"] == list(source_check.ACCEPTED_SOURCES), "accepted source evidence drift")
        require(evidence["changed_paths"] == sorted(source_check.ALLOWED_CHANGES), "changed path drift")
        require(evidence["negative_cases"] == 20, "negative count mismatch")
        require(evidence["acceptance_matrix_rows"] == 20, "matrix count mismatch")
        require(evidence["linked_fixture_steps"] == 9, "linked step count mismatch")
        require(evidence["production_rust_changes"] == 0, "production Rust opened")
        require(evidence["cargo_changes"] == 0, "Cargo opened")
        require(all(flag is False for flag in evidence["closed_surfaces"].values()), "surface opened")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        for name, digest in REVIEWS.items():
            require(sha256(files[name]) == digest, f"review digest mismatch: {name}")
        for expected in (
            b"PASS stage8b-p1f-ie-check",
            b"PASS stage8b-p1f-ie-negative-harness 20/20",
            b"PASS positive-control",
            b"PASS nonsemantic-control",
            b"PASS stage8b-p1f-ie-linked-local-composition steps=9 operational=false",
            b"test result: ok.",
            b"PASS stage8b-p1f-ie-gate",
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
            "accepted_reviews_verified": len(REVIEWS),
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_ie_handoff_safety_check.py ARCHIVE")
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
        print(f"stage8b-p1f-ie-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-ie-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
