#!/usr/bin/env python3
"""Verify a Stage 8A-4 implementation R2 immutable handoff archive."""

from __future__ import annotations

import hashlib
import json
import stat
import sys
import zipfile
from pathlib import PurePosixPath

BASE = "cc58c10d22db312cd83640f1c1e7fd86861a4594"
REQUIRED = {
    "handoff-commit.txt",
    "source-tree-manifest.json",
    "handoff-evidence/stage8a4-implementation-evidence.json",
    "handoff-evidence/stage8a4-implementation-full-gate.txt",
    "handoff-evidence/gate-artifacts/stage8a4-implementation-gate-summary.json",
    "handoff-evidence/gate-artifacts/proof-map.stdout.txt",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def verify(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        duplicates = len(names) - len(set(names))
        unsafe = [name for name in names if not safe(name)]
        symlinks = [info.filename for info in infos if stat.S_ISLNK((info.external_attr >> 16) & 0xFFFF)]
        special = [
            info.filename for info in infos
            if not (stat.S_ISREG((info.external_attr >> 16) & 0xFFFF) or (info.external_attr >> 16) == 0)
        ]
        missing = sorted(REQUIRED - set(names))
        if duplicates or unsafe or symlinks or special or missing:
            raise ValueError("unsafe/incomplete archive")
        marker = archive.read("handoff-commit.txt").decode()
        evidence = json.loads(archive.read("handoff-evidence/stage8a4-implementation-evidence.json"))
        manifest_bytes = archive.read("source-tree-manifest.json")
        manifest = json.loads(manifest_bytes)
        if f"source_ref={evidence['source_ref']}" not in marker:
            raise ValueError("source marker mismatch")
        if evidence["accepted_stage8a4_design_ref"] != BASE:
            raise ValueError("accepted design mismatch")
        if evidence["stage"] != "8A-4-implementation-R2":
            raise ValueError("implementation stage mismatch")
        if evidence["rejected_implementation_r1_ref"] != "245fea18f3f22bd4233eed4f9207445efd0a6d46":
            raise ValueError("R1 lineage mismatch")
        if not evidence["all_required_gates_passed"] or evidence["retry_or_send_authorized"]:
            raise ValueError("gate/send evidence mismatch")
        if sha256(manifest_bytes) != evidence["source_manifest_sha256"]:
            raise ValueError("manifest evidence mismatch")
        for member in manifest["members"]:
            if sha256(archive.read(member["path"])) != member["sha256"]:
                raise ValueError(f"source hash mismatch: {member['path']}")
        for name, expected in evidence["gate_artifact_sha256"].items():
            if sha256(archive.read(f"handoff-evidence/gate-artifacts/{name}")) != expected:
                raise ValueError(f"gate artifact mismatch: {name}")
    return {
        "result": "PASS", "members": len(names), "unique_members": len(set(names)),
        "duplicates": duplicates, "unsafe_paths": len(unsafe), "symlinks": len(symlinks),
        "special_files": len(special), "missing_required": len(missing),
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_implementation_handoff_safety_check.py ARCHIVE")
    try:
        result = verify(sys.argv[1])
    except (ValueError, KeyError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"stage8a4-implementation-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
