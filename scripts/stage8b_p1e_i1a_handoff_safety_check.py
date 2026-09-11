#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-e foundation R1 + I1A design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath
from typing import Any


DESIGN_REF = "4a3cdd33bcbb37940cf4a77c5b7485b993bd96fe"
FOUNDATION_REF = "a0b07f6ef16ac8e43004204f1e52184bc615fa97"
REVIEW_SHA256 = "8e828388d61f80719c13afb5da213f0d8610c26d4c6b47355eaeb6aac017a49d"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
INDEX = PREFIX + "stage8b-p1e-i1a-verification-index.json"
REVIEW = PREFIX + "FINAM_P1e_I1_FOUNDATION_REVIEW_c7cec79_I1A_DECISION_2026-09-11.md"
LOGS = {
    "foundation_gate": PREFIX + "stage8b-p1e-i1-foundation-r1-gate.txt",
    "i1a_design_gate": PREFIX + "stage8b-p1e-i1a-design-gate.txt",
    "fmt": PREFIX + "cargo-fmt-all-check.txt",
    "strategy_runtime_core_lib": PREFIX + "strategy-runtime-core-lib-all-features.txt",
    "runtime_durable_service_lib": PREFIX + "runtime-durable-service-lib-all-features.txt",
    "strategy_runtime_core_doc": PREFIX + "strategy-runtime-core-doc-all-features.txt",
    "runtime_durable_service_doc": PREFIX + "runtime-durable-service-doc-all-features.txt",
    "strict_clippy": PREFIX + "strict-clippy-two-crates.txt",
    "p1d4_negative": PREFIX + "p1d4-source-negative-harness.txt",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, INDEX, REVIEW, *LOGS.values()}
REQUIRED = GENERATED | {
    "docs/stage-8/stage8b-p1e-i1-supervisor-foundation-review-boundary.md",
    "docs/stage-8/stage8b-p1e-redis-runtime-policy-v2.json",
    "docs/stage-8/stage8b-p1e-atomic-stale-consumer-delete-v1.lua",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.md",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v1.schema.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-acceptance-matrix-v1.csv",
    "docs/stage-8/stage8b-p1e-i1a-implementation-scope-v1.json",
    "scripts/stage8b_p1e_i1_foundation_r1_check.py",
    "scripts/stage8b_p1e_i1_foundation_r1_negative_harness.py",
    "scripts/stage8b_p1e_i1_foundation_r1_gate.sh",
    "scripts/stage8b_p1e_i1a_design_check.py",
    "scripts/stage8b_p1e_i1a_design_negative_harness.py",
    "scripts/stage8b_p1e_i1a_design_gate.sh",
}


def fail(message: str) -> None:
    raise ValueError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def git_object_id(kind: str, raw: bytes) -> str:
    return hashlib.sha1(f"{kind} {len(raw)}\0".encode() + raw).hexdigest()


def validate_member_name(name: str) -> None:
    path = PurePosixPath(name)
    require(
        bool(name)
        and not name.startswith("/")
        and "\\" not in name
        and all(part not in {"", ".", ".."} for part in path.parts),
        f"unsafe archive member: {name!r}",
    )
    require(
        not any(part in {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"} for part in path.parts),
        f"forbidden archive path: {name}",
    )
    basename = path.name
    require(
        basename != ".env" and not (basename.startswith(".env.") and basename != ".env.example"),
        f"secret-bearing env member: {name}",
    )
    require(
        not basename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")),
        f"secret/runtime artifact: {name}",
    )


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker key")
        result[key] = value
    require(
        set(result)
        == {"stage", "source_short_ref", "source_ref", "source_tree", "source_branch", "parent_ref", "design_ref", "foundation_ref", "archive_name"},
        "marker key inventory drift",
    )
    return result


def build_tree_oid(entries: list[dict[str, Any]], payloads: dict[str, bytes]) -> str:
    root: dict[str, Any] = {}
    for entry in entries:
        cursor = root
        parts = entry["path"].split("/")
        for part in parts[:-1]:
            child = cursor.setdefault(part, {})
            require(isinstance(child, dict), f"source tree collision at {entry['path']}")
            cursor = child
        require(parts[-1] not in cursor, f"duplicate source path: {entry['path']}")
        cursor[parts[-1]] = (entry["mode"], payloads[entry["path"]])

    def encode(node: dict[str, Any]) -> str:
        records: list[tuple[bytes, bytes]] = []
        for name, value in node.items():
            if isinstance(value, dict):
                mode = "40000"
                oid = encode(value)
                sort_key = name.encode() + b"/"
            else:
                mode, body = value
                oid = git_object_id("blob", body)
                sort_key = name.encode() + b"\0"
            record = mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(oid)
            records.append((sort_key, record))
        return git_object_id("tree", b"".join(record for _, record in sorted(records)))

    return encode(root)


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        require(len(names) == len(set(names)), "duplicate archive members")
        missing = REQUIRED - set(names)
        require(not missing, f"missing required members: {sorted(missing)}")
        for item in infos:
            validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name binding mismatch")
        require(marker["design_ref"] == DESIGN_REF, "design ref drift")
        require(marker["foundation_ref"] == FOUNDATION_REF, "foundation ref drift")
        require(marker["parent_ref"] == DESIGN_REF, "handoff tooling is not direct child of design")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(git_object_id("commit", commit_raw) == marker["source_ref"], "commit object id mismatch")
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree header mismatch")
        require(f"parent {DESIGN_REF}" in commit_lines, "commit parent header mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 3, "manifest version drift")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source ref mismatch")
        require(manifest["source_tree"] == marker["source_tree"], "manifest tree mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        tracked_payloads: dict[str, bytes] = {}
        by_name = {item.filename: item for item in infos}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            body = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(body) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(body) == entry["sha256"], f"manifest digest mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            tracked_payloads[name] = body
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(build_tree_oid(entries, tracked_payloads) == marker["source_tree"], "reconstructed Git tree mismatch")

        require(sha256(files[REVIEW]) == REVIEW_SHA256, "review digest mismatch")
        index = json.loads(files[INDEX])
        require(index["source_ref"] == marker["source_ref"], "verification source ref mismatch")
        require(index["source_tree"] == marker["source_tree"], "verification source tree mismatch")
        require(index["foundation_ref"] == FOUNDATION_REF, "verification foundation ref mismatch")
        require(index["design_ref"] == DESIGN_REF, "verification design ref mismatch")
        require(index["all_passed"] is True, "verification not all-pass")
        require(set(index["checks"]) == set(LOGS), "verification check inventory drift")
        for name, log_path in LOGS.items():
            record = index["checks"][name]
            require(record["exit_code"] == 0, f"failed retained check: {name}")
            require(record["log_path"] == log_path, f"log path mismatch: {name}")
            require(record["log_sha256"] == sha256(files[log_path]), f"log digest mismatch: {name}")
            log = files[log_path]
            require(f"source_ref={marker['source_ref']}".encode() in log, f"log source ref missing: {name}")
            require(f"source_tree={marker['source_tree']}".encode() in log, f"log source tree missing: {name}")
            require(b"exit_code=0" in log, f"log exit code missing: {name}")

        markers = {
            "foundation_gate": b"stage8b-p1e-i1-foundation-r1-gate: ok",
            "i1a_design_gate": b"stage8b-p1e-i1a-design-gate: ok",
            "fmt": b"exit_code=0",
            "strategy_runtime_core_lib": b"test result: ok.",
            "runtime_durable_service_lib": b"test result: ok.",
            "strategy_runtime_core_doc": b"test result: ok.",
            "runtime_durable_service_doc": b"test result: ok.",
            "strict_clippy": b"Finished",
            "p1d4_negative": b"PASS stage8b-p1d4-source-negative-harness 60/60",
        }
        for name, expected in markers.items():
            require(expected in files[LOGS[name]], f"retained marker missing: {name}")

        exact_false = {
            "redis_db0_vps_activation": False,
            "redis_db15_schedule_activation": False,
            "schedule_private_key_installation": False,
            "finam_post_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
            "generation_2_execution_activation": False,
        }
        require(index["closed_surfaces"] == exact_false, "closed surface inventory drift")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "retained_checks": len(LOGS),
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1a_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1a-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1a-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
