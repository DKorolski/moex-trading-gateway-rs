#!/usr/bin/env python3
"""Self-contained byte/mode/provenance validation for GOV-CI-1B handoffs."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/gov-ci-1b-evidence.json"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMANDS = "handoff-evidence/commands.json"
REQUIRED_SOURCE = {
    "handoff-commit.txt",
    EVIDENCE,
    MANIFEST,
    COMMANDS,
    "docs/stage-8/GOV_CI_1_CURRENT_TREE_AUTHORITY_2026-08-21.md",
    "docs/stage-8/GOV_CI_1_ACCEPTANCE_MATRIX_2026-08-21.csv",
    "docs/stage-8/GOV_CI_1_NEGATIVE_INVENTORY_2026-08-21.md",
    "docs/stage-8/gov-ci-1-authority.json",
    ".github/workflows/ci.yml",
    ".github/workflows/stage5f-base-authority.yml",
}
REQUIRED_COMMANDS = {
    "current-tree-authority-gate",
    "cargo-fmt",
    "cargo-debug",
    "cargo-release",
    "cargo-doc",
    "cargo-clippy",
    "no-redis-smoke",
    "redis-shadow-smoke",
    "runtime-bridge-dry-smoke",
    "git-diff-check",
}
GENERATED_PREFIXES = (
    "handoff-evidence/logs/",
    "handoff-evidence/current-tree-gate-artifacts/",
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        info_by_name = {info.filename: info for info in infos}
        missing = REQUIRED_SOURCE - set(names)
        if missing:
            raise ValueError(f"missing members: {sorted(missing)}")
        for info in infos:
            member = PurePosixPath(info.filename)
            mode = (info.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3")):
                raise ValueError(f"runtime artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        commands = json.loads(archive.read(COMMANDS))
        if evidence.get("stage") != "GOV-CI-1B":
            raise ValueError("stage mismatch")
        if evidence.get("source_ref") != marker.get("source_ref"):
            raise ValueError("source mismatch")
        if manifest.get("source_ref") != marker.get("source_ref"):
            raise ValueError("manifest source mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive mismatch")
        if evidence.get("acceptance_rows") != 30 or evidence.get("negative_cases") != 27:
            raise ValueError("count mismatch")
        if evidence.get("accepted_predecessor") != "1dea519cbf2affc3d99866fdae66bbddbafefa24":
            raise ValueError("predecessor mismatch")
        if evidence.get("governance_only") is not True:
            raise ValueError("governance-only marker missing")
        for key in (
            "stage8b_s_authorized",
            "finam_post_delete",
            "broker_execution",
            "redis_live_consumer",
            "redis_xadd_xack",
            "runtime_live",
            "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")

        tracked_names: set[str] = set()
        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("source manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked_names:
                raise ValueError(f"duplicate source manifest path: {name}")
            tracked_names.add(name)
            info = info_by_name.get(name)
            if info is None:
                raise ValueError(f"manifest member missing: {name}")
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"]:
                raise ValueError(f"manifest byte mismatch: {name}")
            if archive_mode(info) != entry["mode"]:
                raise ValueError(f"manifest mode mismatch: {name}")

        generated_names = set(names) - tracked_names
        expected_fixed = {"handoff-commit.txt", EVIDENCE, MANIFEST, COMMANDS}
        unexpected = {
            name
            for name in generated_names - expected_fixed
            if not name.startswith(GENERATED_PREFIXES)
        }
        if unexpected:
            raise ValueError(f"unexpected generated members: {sorted(unexpected)}")

        command_rows = commands.get("commands", [])
        command_ids = {row.get("id") for row in command_rows}
        if command_ids != REQUIRED_COMMANDS:
            raise ValueError(f"command inventory mismatch: {command_ids}")
        for row in command_rows:
            if row.get("exit_code") != 0:
                raise ValueError(f"command failed: {row.get('id')}")
            log_path = row.get("log_path")
            if log_path not in info_by_name:
                raise ValueError(f"command log missing: {log_path}")
            if sha256(archive.read(log_path)) != row.get("log_sha256"):
                raise ValueError(f"command log hash mismatch: {row.get('id')}")
        for item in evidence.get("gate_artifacts", []):
            artifact_path = item.get("path")
            if artifact_path not in info_by_name:
                raise ValueError(f"gate artifact missing: {artifact_path}")
            if sha256(archive.read(artifact_path)) != item.get("sha256"):
                raise ValueError(f"gate artifact hash mismatch: {artifact_path}")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked_names),
            "command_logs_verified": len(command_rows),
            "gate_artifacts_verified": len(evidence.get("gate_artifacts", [])),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: gov_ci_1_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"gov-ci-1b-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("gov-ci-1b-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
