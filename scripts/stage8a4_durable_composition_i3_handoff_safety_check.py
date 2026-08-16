#!/usr/bin/env python3
"""Validate immutable Stage 8A-4 I3 handoff structure and evidence binding."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

REQUIRED = {
    "handoff-commit.txt",
    "handoff-evidence/stage8a4-durable-composition-i3-evidence.json",
    "handoff-evidence/stage8a4-durable-composition-i3-full-gate.txt",
    "handoff-evidence/source-tree-manifest.json",
    "handoff-evidence/gate-artifacts/stage8a4-durable-composition-i3-gate-summary.json",
    "docs/stage-8/stage8a4-durable-composition-i3-authority.json",
    "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I3_IMPLEMENTATION_2026-08-16.md",
}


def check(archive_path: str) -> dict[str, object]:
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate archive members")
        missing = REQUIRED - set(names)
        if missing:
            raise ValueError(f"missing members: {sorted(missing)}")
        for info in infos:
            path = PurePosixPath(info.filename)
            if path.is_absolute() or ".." in path.parts or "" in path.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            mode = info.external_attr >> 16
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink forbidden: {info.filename}")
            if path.parts and path.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in path.parts) or info.filename.endswith(".log"):
                raise ValueError(f"secret/log forbidden: {info.filename}")
        fields = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(
            archive.read("handoff-evidence/stage8a4-durable-composition-i3-evidence.json")
        )
        if fields.get("source_ref") != evidence.get("source_ref"):
            raise ValueError("source ref mismatch")
        if fields.get("archive_name") != PurePosixPath(archive_path).name:
            raise ValueError("archive name mismatch")
        gate = archive.read("handoff-evidence/stage8a4-durable-composition-i3-full-gate.txt")
        if hashlib.sha256(gate).hexdigest() != evidence.get("full_gate_sha256"):
            raise ValueError("gate hash mismatch")
        manifest = archive.read("handoff-evidence/source-tree-manifest.json")
        if hashlib.sha256(manifest).hexdigest() != evidence.get("source_tree_manifest_sha256"):
            raise ValueError("manifest hash mismatch")
        return {
            "archive_members": len(names),
            "unique_members": len(set(names)),
            "duplicates": 0,
            "unsafe_paths": 0,
            "symlinks": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_durable_composition_i3_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i3-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-i3-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
