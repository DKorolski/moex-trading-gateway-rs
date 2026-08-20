#!/usr/bin/env python3
"""Validate immutable Stage 8A-4 I4 handoff structure and binding."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

REQUIRED = {
    "handoff-commit.txt",
    "handoff-evidence/stage8a4-i4-evidence.json",
    "handoff-evidence/stage8a4-i4-full-gate.txt",
    "handoff-evidence/source-tree-manifest.json",
    "handoff-evidence/gate-artifacts/stage8a4-i4-gate-summary.json",
    "docs/stage-8/stage8a4-durable-composition-i4-authority.json",
    "docs/stage-8/STAGE8A4_DURABLE_COMPOSITION_I4_IMPLEMENTATION_2026-08-20.md",
}


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
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if info.external_attr >> 16 & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts) or info.filename.endswith(".log"):
                raise ValueError(f"secret/log: {info.filename}")
        fields = dict(line.split("=", 1) for line in archive.read("handoff-commit.txt").decode().splitlines() if "=" in line)
        evidence = json.loads(archive.read("handoff-evidence/stage8a4-i4-evidence.json"))
        if evidence.get("stage") != "8A-4-durable-composition-I4" or evidence.get("acceptance_rows") != 40 or evidence.get("negative_cases") != 12:
            raise ValueError("I4 evidence mismatch")
        if fields.get("source_ref") != evidence.get("source_ref") or fields.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("provenance mismatch")
        for key in ("read_only_no_effect", "terminal_authority_public_opaque", "ack_timestamp_free", "current_readiness_independent"):
            if evidence.get(key) is not True:
                raise ValueError(f"required evidence disabled: {key}")
        for key in ("seal_mutation", "ack_readiness_publication_enabled", "redis_mutation_enabled", "finam_post_delete_enabled", "runtime_live_enabled", "real_orders_enabled"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read("handoff-evidence/stage8a4-i4-full-gate.txt")
        manifest = archive.read("handoff-evidence/source-tree-manifest.json")
        if hashlib.sha256(gate).hexdigest() != evidence.get("full_gate_sha256") or hashlib.sha256(manifest).hexdigest() != evidence.get("source_tree_manifest_sha256"):
            raise ValueError("evidence hash mismatch")
        return {"archive_members": len(names), "duplicates": 0, "unsafe_paths": 0, "symlinks": 0, "source_ref": evidence["source_ref"], "result": "PASS"}


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a4_durable_composition_i4_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8a4-i4-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a4-i4-handoff-safety: PASS " + json.dumps(result, sort_keys=True))
