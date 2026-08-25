#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R1 design-only authorization handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r1-authorization-evidence.json"
GATE = "handoff-evidence/stage8b-p-r1-authorization-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R1_ACCEPTANCE_MATRIX_2026-08-24.csv",
    "docs/stage-8/STAGE8B_P_R1_AUTHORIZATION_PACKAGE_2026-08-24.md",
    "docs/stage-8/STAGE8B_P_R1_NEGATIVE_INVENTORY_2026-08-24.md",
    "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json",
    "docs/stage-8/stage8b-p-r1-authorization-authority.json",
    "scripts/stage8b_p_contract_refresh.py",
    "scripts/stage8b_p_r1_authorization_check.py",
    "scripts/stage8b_p_r1_authorization_gate.sh",
    "scripts/stage8b_p_r1_authorization_negative_harness.py",
    "scripts/make_stage8b_p_r1_authorization_handoff.py",
    "scripts/stage8b_p_r1_authorization_handoff_safety_check.py",
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        by_name = {info.filename: info for info in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        missing = REQUIRED - set(names)
        if missing:
            raise ValueError(f"missing members: {sorted(missing)}")
        for info in infos:
            member = PurePosixPath(info.filename)
            file_mode = (info.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if file_mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if file_mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {info.filename}")
            if member.parts and member.parts[0] in {
                ".git", "target", "tmp", "reports", "__MACOSX",
            }:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3", ".pem", ".key")):
                raise ValueError(f"runtime/key artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "8B-P" or evidence.get("revision") != "R1":
            raise ValueError("stage/revision mismatch")
        if evidence.get("status") != "design_only_authorization_candidate":
            raise ValueError("candidate status mismatch")
        if evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("authorization unexpectedly issued")
        if evidence.get("accepted_predecessor_ref") != (
            "16a59bca74f94881c70d9fa39bbdf1c357e65f95"
        ):
            raise ValueError("predecessor mismatch")
        if evidence.get("acceptance_rows") != 55 or evidence.get("negative_mutations") != 48:
            raise ValueError("coverage count mismatch")
        for key in (
            "broker_readonly_get", "operator_arm_issued", "dispatch_attempt_recorded",
            "transport_entered", "finam_post_delete", "broker_effect", "stage8b_p",
            "stage8b_xe", "redis_execution", "broker_dispatch", "runtime_live",
            "real_orders", "stage11_execution_promotion", "stage12",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read(GATE)
        if b"stage8b-p-r1-authorization-gate: PASS rows=55 negatives=48" not in gate:
            raise ValueError("R1 gate marker missing")
        if b"stage8b-p-contract-refresh: PASS responses=7 material_drift=false" not in gate:
            raise ValueError("fresh contract marker missing")
        if b"stage8b-p-governance-refresh: PASS ruleset=20111805 enforcement=active" not in gate:
            raise ValueError("live governance marker missing")
        if sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")
        manifest_bytes = archive.read(MANIFEST)
        if sha(manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        tracked: set[str] = set()
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in by_name:
                raise ValueError(f"manifest member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if (
                len(data) != entry["size"]
                or sha(data) != entry["sha256"]
                or mode(by_name[name]) != entry["mode"]
            ):
                raise ValueError(f"manifest content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": source_ref,
            "authorization_status": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r1_authorization_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-r1-authorization-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        "stage8b-p-r1-authorization-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
