#!/usr/bin/env python3
"""Validate provenance, tracked bytes/modes and closed surfaces for 8B-D R2."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-design-evidence.json"
GATE = "handoff-evidence/stage8b-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
REQUIRED = {
    "handoff-commit.txt",
    EVIDENCE,
    GATE,
    MANIFEST,
    "docs/stage-8/STAGE8B_DESIGN_2026-08-21.md",
    "docs/stage-8/STAGE8B_DESIGN_ACCEPTANCE_MATRIX_2026-08-21.csv",
    "docs/stage-8/STAGE8B_DESIGN_NEGATIVE_INVENTORY_2026-08-21.md",
    "docs/stage-8/stage8b-design-authority.json",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        info_by_name = {info.filename: info for info in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        missing = REQUIRED - set(names)
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
        if evidence.get("stage") != "8B-D-R2":
            raise ValueError("stage mismatch")
        if evidence.get("source_ref") != marker.get("source_ref"):
            raise ValueError("source mismatch")
        if manifest.get("source_ref") != marker.get("source_ref"):
            raise ValueError("manifest source mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive mismatch")
        if evidence.get("acceptance_rows") != 70 or evidence.get("negative_cases") != 50:
            raise ValueError("count mismatch")
        if evidence.get("phase_count") != 5 or evidence.get("design_only") is not True:
            raise ValueError("design scope mismatch")
        for key in (
            "implementation_enabled",
            "stage8b_s_enabled",
            "stage8b_execution",
            "finam_post_delete",
            "redis_xadd_xack",
            "redis_live_consumer",
            "ack_readiness_publication",
            "broker_dispatch",
            "runtime_live",
            "real_orders",
            "stage12_strategy_live",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        if sha256(archive.read(GATE)) != evidence.get("gate_sha256"):
            raise ValueError("gate hash mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest hash mismatch")

        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        tracked: set[str] = set()
        for entry in entries:
            name = entry["path"]
            if name in tracked:
                raise ValueError(f"duplicate manifest path: {name}")
            tracked.add(name)
            info = info_by_name.get(name)
            if info is None:
                raise ValueError(f"missing tracked member: {name}")
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"]:
                raise ValueError(f"manifest byte mismatch: {name}")
            if archive_mode(info) != entry["mode"]:
                raise ValueError(f"manifest mode mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("unexpected generated member inventory")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
