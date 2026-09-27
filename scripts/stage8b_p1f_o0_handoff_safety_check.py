#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-f O0 handoff archive."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_o0_check as source_check


STAGE = "Stage 8B-P1-f O0 immutable read-only target preflight"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-o0-handoff-evidence.json"
GATE = PREFIX + "stage8b-p1f-o0-gate.txt"
REVIEW = PREFIX + "reviews/" + source_check.IE_REVIEW
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, REVIEW}
REQUIRED = GENERATED | source_check.ALLOWED_CHANGES


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def validate_member_name(name: str) -> None:
    path = PurePosixPath(name)
    require(
        bool(name)
        and not name.startswith("/")
        and "\\" not in name
        and all(part not in {"", ".", ".."} for part in path.parts),
        f"unsafe archive member: {name!r}",
    )
    forbidden = {".git", "target", "tmp", "__pycache__", "__MACOSX"}
    require(not any(part in forbidden for part in path.parts), f"forbidden archive path: {name}")
    if "reports" in path.parts:
        require(name == source_check.RAW, f"forbidden reports path: {name}")
    basename = path.name
    require(
        basename != ".env" and not (basename.startswith(".env.") and basename != ".env.example"),
        f"secret-bearing env member: {name}",
    )
    require(
        not basename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")),
        f"secret/runtime artifact: {name}",
    )


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker item")
        result[key] = value
    require(set(result) == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "accepted_ie_ref", "archive_name"}, "marker inventory drift")
    return result


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        require(not (REQUIRED - set(names)), f"missing required members: {sorted(REQUIRED - set(names))}")
        for item in infos:
            validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")
        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_parent"] == source_check.BASE, "source parent mismatch")
        require(marker["accepted_ie_ref"] == source_check.ACCEPTED_IE, "accepted Ie mismatch")
        require(marker["branch"] == source_check.BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit mismatch")
        lines = commit_raw.decode().splitlines()
        require(lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {source_check.BASE}" in lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=strict_object)
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

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=strict_object)
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == source_check.BASE, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["status"] == "O0_REVIEW_CANDIDATE_NO_REMOTE_MUTATION", "handoff status mismatch")
        require(evidence["changed_paths"] == sorted(source_check.ALLOWED_CHANGES), "changed-path drift")
        require(evidence["negative_cases"] == 20 and evidence["acceptance_matrix_rows"] == 20, "test count drift")
        require(evidence["rust_changes"] == 0 and evidence["cargo_changes"] == 0, "source scope opened")
        require(evidence["remote_mutation_performed"] is False, "remote mutation opened")
        require(evidence["o1_authorized"] is False, "O1 opened")
        require(all(flag is False for flag in evidence["closed_surfaces"].values()), "surface opened")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["target_evidence_sha256"] == sha256(files[source_check.EVIDENCE]), "target evidence digest mismatch")
        require(evidence["raw_probe_sha256"] == sha256(files[source_check.RAW]), "raw probe digest mismatch")
        require(sha256(files[REVIEW]) == source_check.IE_REVIEW_SHA256, "Ie review digest mismatch")
        for expected in (
            b"PASS stage8b-p1f-o0-check",
            b"PASS positive-control",
            b"PASS nonsemantic-control",
            b"PASS stage8b-p1f-o0-negative-harness 20/20",
            b"PASS stage8b-p1f-o0-gate",
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
        raise SystemExit("usage: stage8b_p1f_o0_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o0-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-o0-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
