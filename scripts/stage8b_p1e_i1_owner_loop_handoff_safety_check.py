#!/usr/bin/env python3
"""Validate the immutable committed owner-loop review handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_committed_restart_check as source_check
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = "efe56a9af6272f13b2d87f4ad14a2709c605cd42"
BRANCH = "stage8b-paper-shadow-resumption"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-owner-loop-evidence.json"
LOGS = {
    "source_gate": "handoff-evidence/stage8b-p1e-i1-owner-loop-source-gate.log",
    "runtime_lib": "handoff-evidence/stage8b-p1e-i1-owner-loop-runtime-lib.log",
    "runtime_redis_integration": "handoff-evidence/stage8b-p1e-i1-owner-loop-runtime-redis-integration.log",
    "runtime_writer_integration": "handoff-evidence/stage8b-p1e-i1-owner-loop-runtime-writer-integration.log",
    "runtime_doc": "handoff-evidence/stage8b-p1e-i1-owner-loop-runtime-doc.log",
    "core_full": "handoff-evidence/stage8b-p1e-i1-owner-loop-core-full.log",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, *LOGS.values()}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        require("=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in values, "invalid marker field")
        values[key] = value
    require(
        set(values)
        == {
            "stage",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "archive_name",
        },
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
        require(marker["stage"] == "Stage 8B-P1-e I1 committed owner-loop wiring", "stage mismatch")
        require(marker["source_parent"] == PARENT, "source parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in commit_lines, "commit parent mismatch")

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
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(evidence["production_owner_loop_wiring"] is True, "owner-loop wiring not declared")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(set(evidence["commands"]) == set(LOGS), "command inventory mismatch")
        for name, log_path in LOGS.items():
            record = evidence["commands"][name]
            raw = files[log_path]
            require(record["log_path"] == log_path, f"log path mismatch: {name}")
            require(record["log_sha256"] == sha256(raw), f"log digest mismatch: {name}")
            require(record["log_bytes"] == len(raw), f"log size mismatch: {name}")
            require(record["exit_code"] == 0, f"nonzero evidence exit: {name}")
            require(b"exit_code=0" in raw, f"exit status missing from log: {name}")
            require(marker["source_ref"].encode() in raw, f"source ref missing from log: {name}")
            require(marker["source_tree"].encode() in raw, f"source tree missing from log: {name}")

        require(b"PASS stage8b-p1e-i1-owner-loop-gate" in files[LOGS["source_gate"]], "source gate marker missing")
        for name in (
            "runtime_lib",
            "runtime_redis_integration",
            "runtime_writer_integration",
            "runtime_doc",
            "core_full",
        ):
            raw = files[LOGS[name]]
            require(b"test result: ok." in raw, f"full test success missing: {name}")
            require(b"test result: FAILED" not in raw, f"full test failure present: {name}")

        with tempfile.TemporaryDirectory(prefix="stage8b-p1e-owner-loop-") as directory:
            root = Path(directory)
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            source_check.validate_content(source_check.load_content(root))

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_parent": PARENT,
            "source_tree": marker["source_tree"],
            "actual_logs_verified": len(LOGS),
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_owner_loop_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1-owner-loop-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-owner-loop-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
