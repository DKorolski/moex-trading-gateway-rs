#!/usr/bin/env python3
"""Validate immutable Stage 8A-4 I1 handoff structure and evidence binding."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

REQUIRED = {
    "handoff-commit.txt",
    "handoff-evidence/stage8a4-durable-composition-i1-evidence.json",
    "handoff-evidence/stage8a4-durable-composition-i1-full-gate.txt",
    "handoff-evidence/source-tree-manifest.json",
    "handoff-evidence/gate-artifacts/stage8a4-durable-composition-i1-gate-summary.json",
    "docs/stage-8/stage8a4-durable-composition-i1-authority.json",
    "fixtures/stage8a4-i1/canonical-golden-sha256.json",
}


def check(archive_path: str) -> dict[str, object]:
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate archive members")
        missing = REQUIRED - set(names)
        if missing:
            raise ValueError(f"missing required members: {sorted(missing)}")
        for info in infos:
            path = PurePosixPath(info.filename)
            if path.is_absolute() or ".." in path.parts or "" in path.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if info.external_attr >> 16 & 0o170000 == 0o120000:
                raise ValueError(f"symlink forbidden: {info.filename}")
            if path.parts and path.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden archive root: {info.filename}")
            if any(part == ".env" for part in path.parts) or info.filename.endswith(".log"):
                raise ValueError(f"secret/log surface forbidden: {info.filename}")

        marker = archive.read("handoff-commit.txt").decode("utf-8")
        fields = dict(line.split("=", 1) for line in marker.splitlines() if "=" in line)
        evidence = json.loads(archive.read("handoff-evidence/stage8a4-durable-composition-i1-evidence.json"))
        if fields.get("source_ref") != evidence.get("source_ref"):
            raise ValueError("source ref binding mismatch")
        if fields.get("archive_name") != PurePosixPath(archive_path).name:
            raise ValueError("archive name binding mismatch")
        gate = archive.read("handoff-evidence/stage8a4-durable-composition-i1-full-gate.txt")
        if hashlib.sha256(gate).hexdigest() != evidence.get("full_gate_sha256"):
            raise ValueError("full gate hash mismatch")
        manifest = archive.read("handoff-evidence/source-tree-manifest.json")
        if hashlib.sha256(manifest).hexdigest() != evidence.get("source_tree_manifest_sha256"):
            raise ValueError("source manifest hash mismatch")
        return {
            "archive_members": len(names),
            "unique_members": len(set(names)),
            "duplicates": 0,
            "unsafe_paths": 0,
            "symlinks": 0,
            "forbidden_members": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_durable_composition_i1_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8a4-durable-composition-i1-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-durable-composition-i1-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
