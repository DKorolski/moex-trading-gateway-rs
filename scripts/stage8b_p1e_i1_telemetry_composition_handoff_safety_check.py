#!/usr/bin/env python3
"""Validate the immutable I1 production telemetry correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_telemetry_composition_check as telemetry_check
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = "a38d8c6c539a47f3f0e82965d40960d239114815"
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 production telemetry composition correction"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-telemetry-composition-evidence.json"
REVIEW = "handoff-evidence/reviews/FINAM_I1_TELEMETRY_CORRECTION_REVIEW_a38d8c6_2026-09-24.md"
REVIEW_SHA256 = "2a0f15e7d6920062cda7838664e017e6ccc67118587c5b5d6c960f577b0a1c9e"
LOGS = {
    "source_gate": "handoff-evidence/stage8b-p1e-i1-telemetry-source-gate.log",
    "runtime_process": "handoff-evidence/stage8b-p1e-i1-telemetry-process-tests.log",
    "runtime_lib": "handoff-evidence/stage8b-p1e-i1-telemetry-runtime-lib.log",
    "runtime_redis_integration": "handoff-evidence/stage8b-p1e-i1-telemetry-redis-integration.log",
    "runtime_writer_integration": "handoff-evidence/stage8b-p1e-i1-telemetry-writer-integration.log",
    "runtime_doc": "handoff-evidence/stage8b-p1e-i1-telemetry-runtime-doc.log",
    "core_full": "handoff-evidence/stage8b-p1e-i1-telemetry-core-full.log",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, REVIEW, *LOGS.values()}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require("=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in values, "invalid marker field")
        values[key] = value
    require(
        set(values)
        == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"},
        "marker inventory drift",
    )
    return values


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive member")
        require(not (GENERATED - set(names)), "generated evidence member missing")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000}, f"special member: {item.filename}")
            parts = PurePosixPath(item.filename).parts
            require(not any(part == ".env" for part in parts), f"secret member: {item.filename}")
            require(
                not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")),
                f"secret/runtime member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["source_parent"] == PARENT, "source parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        lines = commit_raw.decode().splitlines()
        require(lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"tracked member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(entry["size"] == len(raw), f"size mismatch: {name}")
            require(entry["sha256"] == sha256(raw), f"digest mismatch: {name}")
            require(entry["mode"] == mode, f"mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE])
        require(
            evidence["status"] == "SOURCE_CORRECTION_REVIEW_CANDIDATE_I1_NOT_CLOSED",
            "evidence status drift",
        )
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == telemetry_check.ALLOWED_CHANGES, "changed-path evidence drift")
        require(evidence["i1_closed"] is False, "I1 self-closed")
        require(evidence["next_slice"] == "fixed-path installation and systemd material", "next slice drift")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(sha256(files[REVIEW]) == REVIEW_SHA256, "predecessor review digest mismatch")
        commands = evidence["commands"]
        require(set(commands) == set(LOGS), "command inventory drift")
        for name, log_path in LOGS.items():
            require(commands[name]["exit_code"] == 0, f"failed command: {name}")
            require(commands[name]["log_path"] == log_path, f"log path drift: {name}")
            require(commands[name]["log_sha256"] == sha256(files[log_path]), f"log digest drift: {name}")
            require(b"exit_code=0" in files[log_path], f"successful exit marker missing: {name}")
        for token in (
            b"stage8b-p1e-i1-telemetry-composition-check: PASS rows=29 closed_surfaces=9 targeted_findings=3",
            b"stage8b-p1e-i1-telemetry-composition-negative-harness 79/79",
            b"stage8b-p1e-i1-process-supervision-negative-harness 77/77",
            b"PASS stage8b-p1e-i1-telemetry-composition-gate",
        ):
            require(token in files[LOGS["source_gate"]], f"source gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-i1-telemetry-") as directory:
            root = Path(directory)
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            telemetry_check.validate(root, verify_lineage=False)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_parent": PARENT,
            "source_tree": marker["source_tree"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_telemetry_composition_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1-telemetry-composition-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-telemetry-composition-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
