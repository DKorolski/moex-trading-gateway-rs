#!/usr/bin/env python3
"""Safety and provenance validation for an I4 Design R2 handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8a4-i4-design-evidence.json"
GATE = "handoff-evidence/stage8a4-i4-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
REQUIRED = {
    "handoff-commit.txt", EVIDENCE, GATE, MANIFEST,
    "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_2026-08-20.md",
    "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_ACCEPTANCE_MATRIX_2026-08-20.csv",
    "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_DESIGN_NEGATIVE_INVENTORY_2026-08-20.md",
    "docs/stage-8/stage8a4-durable-composition-i4-design-authority.json",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        missing = REQUIRED - set(names)
        if missing:
            raise ValueError(f"missing members: {sorted(missing)}")
        for info in infos:
            member = PurePosixPath(info.filename)
            mode = info.external_attr >> 16
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts) or info.filename.endswith(".log"):
                raise ValueError(f"secret/log: {info.filename}")
        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        if evidence.get("stage") != "8A-4-durable-composition-I4-design-R2":
            raise ValueError("stage mismatch")
        if evidence.get("source_ref") != marker.get("source_ref"):
            raise ValueError("source mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive mismatch")
        if evidence.get("acceptance_rows") != 56 or evidence.get("negative_cases") != 38:
            raise ValueError("count mismatch")
        if evidence.get("timestamp_model") != "timestamp_free_model_a":
            raise ValueError("timestamp model mismatch")
        if evidence.get("stable_ack_identity") != "reuse_exact_stage7b_terminal_request_ack_identity_sha256":
            raise ValueError("stable ACK identity mismatch")
        if evidence.get("implementation_enabled") is not False:
            raise ValueError("implementation opened")
        for key in ("ack_publication", "redis_xack", "redis_live", "finam_post_delete", "broker_dispatch", "runtime_live", "real_orders"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        if sha256(archive.read(GATE)) != evidence.get("gate_sha256"):
            raise ValueError("gate hash mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest hash mismatch")
        return {
            "members": len(names), "unique_members": len(set(names)),
            "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": evidence["source_ref"], "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_durable_composition_i4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8a4-i4-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-i4-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
