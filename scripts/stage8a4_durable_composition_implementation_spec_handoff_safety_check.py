#!/usr/bin/env python3
"""Verify an immutable Stage 8A-4 implementation-spec handoff."""

from __future__ import annotations

import hashlib
import json
import stat
import sys
import zipfile
from pathlib import PurePosixPath

BASE = "6ddf54ef9d7f740dc59cd2450e78301be3d068cb"
REQUIRED = {
    "handoff-commit.txt", "source-tree-manifest.json",
    "handoff-evidence/stage8a4-durable-composition-implementation-spec-evidence.json",
    "handoff-evidence/stage8a4-durable-composition-implementation-spec-full-gate.txt",
    "handoff-evidence/gate-artifacts/stage8a4-durable-composition-implementation-spec-gate-summary.json",
    "handoff-evidence/gate-artifacts/proof-map.stdout.txt",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and not any(
        part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts
    ) and path.name != ".env" and path.suffix != ".log"


def verify(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        duplicates = len(names) - len(set(names))
        unsafe = [name for name in names if not safe(name)]
        symlinks = [item.filename for item in infos if stat.S_ISLNK((item.external_attr >> 16) & 0xFFFF)]
        special = [item.filename for item in infos if not (stat.S_ISREG((item.external_attr >> 16) & 0xFFFF) or (item.external_attr >> 16) == 0)]
        missing = sorted(REQUIRED - set(names))
        if duplicates or unsafe or symlinks or special or missing:
            raise ValueError("unsafe/incomplete archive")
        marker = archive.read("handoff-commit.txt").decode()
        evidence = json.loads(archive.read("handoff-evidence/stage8a4-durable-composition-implementation-spec-evidence.json"))
        manifest_bytes = archive.read("source-tree-manifest.json")
        manifest = json.loads(manifest_bytes)
        if f"source_ref={evidence['source_ref']}" not in marker or evidence["accepted_durable_design_ref"] != BASE:
            raise ValueError("source/design marker mismatch")
        if evidence["stage"] != "8A-4-durable-composition-implementation-spec-R2":
            raise ValueError("stage mismatch")
        if evidence["acceptance_rows"] != 105 or evidence["negative_cases"] != 57:
            raise ValueError("R2 coverage mismatch")
        if not evidence["all_required_gates_passed"] or evidence["durable_apply_authorized"]:
            raise ValueError("gate/apply mismatch")
        if digest(manifest_bytes) != evidence["source_manifest_sha256"]:
            raise ValueError("manifest hash mismatch")
        for member in manifest["members"]:
            if digest(archive.read(member["path"])) != member["sha256"]:
                raise ValueError(f"source hash mismatch: {member['path']}")
        for name, expected in evidence["gate_artifact_sha256"].items():
            if digest(archive.read(f"handoff-evidence/gate-artifacts/{name}")) != expected:
                raise ValueError(f"gate artifact mismatch: {name}")
    return {
        "result": "PASS", "members": len(names), "unique_members": len(set(names)),
        "duplicates": duplicates, "unsafe_paths": len(unsafe), "symlinks": len(symlinks),
        "special_files": len(special), "missing_required": len(missing),
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_durable_composition_implementation_spec_handoff_safety_check.py ARCHIVE")
    try:
        print(json.dumps(verify(sys.argv[1]), indent=2, sort_keys=True))
    except (ValueError, KeyError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"stage8a4-durable-composition-implementation-spec-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
