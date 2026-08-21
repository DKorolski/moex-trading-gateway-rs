#!/usr/bin/env python3
"""Validate immutable Stage 8A-5 aggregate handoff structure and binding."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

REQUIRED = {
    "handoff-commit.txt",
    "handoff-evidence/stage8a5-evidence.json",
    "handoff-evidence/stage8a5-full-gate.txt",
    "handoff-evidence/source-tree-manifest.json",
    "handoff-evidence/gate-artifact-manifest.json",
    "handoff-evidence/gate-artifacts/stage8a5-aggregate-acceptance-result.json",
    "handoff-evidence/gate-artifacts/stage8a5-inherited-stage8-result.json",
    "docs/stage-8/stage8a5-aggregate-acceptance-authority.json",
    "docs/stage-8/STAGE8A5_AGGREGATE_ACCEPTANCE_2026-08-21.md",
    "scripts/stage8a5_gate.sh",
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
            mode = info.external_attr >> 16 & 0o170000
            if mode == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts) or info.filename.endswith((".log", ".sqlite")):
                raise ValueError(f"secret/runtime artifact: {info.filename}")
        fields = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read("handoff-evidence/stage8a5-evidence.json"))
        if (
            evidence.get("stage") != "8A-5-aggregate-acceptance"
            or evidence.get("acceptance_rows") != 30
            or evidence.get("negative_cases") != 20
            or evidence.get("inherited_stage8_negative_cases") != 544
            or evidence.get("current_i4_negative_cases") != 28
        ):
            raise ValueError("Stage8A5 evidence mismatch")
        if fields.get("source_ref") != evidence.get("source_ref") or fields.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("provenance mismatch")
        for key in ("aggregate_only", "inherited_stage7b_gate_passed", "workspace_debug_release_passed"):
            if evidence.get(key) is not True:
                raise ValueError(f"required evidence disabled: {key}")
        for key in (
            "production_rust_changed", "cargo_or_lock_changed", "workflow_changed",
            "stage8b_authorized", "redis_live_consumer_enabled", "finam_post_delete_enabled",
            "broker_dispatch_enabled", "runtime_live_enabled", "real_orders_enabled",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read("handoff-evidence/stage8a5-full-gate.txt")
        source_manifest = archive.read("handoff-evidence/source-tree-manifest.json")
        artifact_manifest = archive.read("handoff-evidence/gate-artifact-manifest.json")
        if hashlib.sha256(gate).hexdigest() != evidence.get("full_gate_sha256"):
            raise ValueError("full gate hash mismatch")
        if hashlib.sha256(source_manifest).hexdigest() != evidence.get("source_tree_manifest_sha256"):
            raise ValueError("source manifest hash mismatch")
        if hashlib.sha256(artifact_manifest).hexdigest() != evidence.get("gate_artifact_manifest_sha256"):
            raise ValueError("gate artifact manifest hash mismatch")
        result = json.loads(archive.read("handoff-evidence/gate-artifacts/stage8a5-aggregate-acceptance-result.json"))
        if result.get("result") != "PASS" or result.get("source_ref") != evidence.get("source_ref"):
            raise ValueError("commit-bound aggregate result mismatch")
        return {
            "archive_members": len(names),
            "duplicates": 0,
            "unsafe_paths": 0,
            "symlinks": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8a5_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8a5-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8a5-handoff-safety: PASS " + json.dumps(result, sort_keys=True))
