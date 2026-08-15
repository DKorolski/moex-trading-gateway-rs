#!/usr/bin/env python3
"""Verify an immutable Stage 8A-4 design handoff archive."""

from __future__ import annotations

import hashlib
import json
import stat
import sys
import zipfile
from pathlib import PurePosixPath

REQUIRED = {
    "handoff-commit.txt",
    "source-tree-manifest.json",
    "handoff-evidence/stage8a4-design-evidence.json",
    "handoff-evidence/stage8a4-design-full-gate.txt",
    "handoff-evidence/gate-artifacts/stage8a4-design-gate-summary.json",
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
            info.filename
            for info in infos
            if not (stat.S_ISREG((info.external_attr >> 16) & 0xFFFF) or (info.external_attr >> 16) == 0)
        ]
        missing = sorted(REQUIRED - set(names))
        if duplicates or unsafe or symlinks or special or missing:
            raise ValueError("unsafe/incomplete archive")
        marker = archive.read("handoff-commit.txt").decode()
        evidence = json.loads(archive.read("handoff-evidence/stage8a4-design-evidence.json"))
        manifest = json.loads(archive.read("source-tree-manifest.json"))
        if f"source_ref={evidence['source_ref']}" not in marker:
            raise ValueError("source marker mismatch")
        if evidence["accepted_stage8a3_ref"] != "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d":
            raise ValueError("predecessor mismatch")
        if evidence["stage"] != "8A-4-design-R2":
            raise ValueError("candidate stage mismatch")
        if evidence["candidate_status"] != "design_r2_independent_acceptance_pending":
            raise ValueError("candidate status mismatch")
        if evidence["acceptance_rows"] != 92 or evidence["negative_cases"] != 68:
            raise ValueError("R2 evidence count mismatch")
        if not evidence["all_required_gates_passed"]:
            raise ValueError("gate evidence mismatch")
        if evidence["network_send_authorized"] or evidence["reconciliation_implemented"]:
            raise ValueError("design boundary opened")
        for member in manifest["members"]:
            if sha256(archive.read(member["path"])) != member["sha256"]:
                raise ValueError(f"source hash mismatch: {member['path']}")
    return {
        "result": "PASS",
        "members": len(names),
        "unique_members": len(set(names)),
        "duplicates": duplicates,
        "unsafe_paths": len(unsafe),
        "symlinks": len(symlinks),
        "special_files": len(special),
        "missing_required": len(missing),
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = verify(sys.argv[1])
    except (ValueError, KeyError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"stage8a4-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
