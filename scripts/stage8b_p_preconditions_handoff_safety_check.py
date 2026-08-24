#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P preconditions design-only handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8b-p-preconditions-evidence.json"
GATE = "handoff-evidence/stage8b-p-preconditions-gate.txt"
BUILD = "handoff-evidence/stage8b-p-build-repro.json"
BINARY = "handoff-evidence/broker-cli-aarch64-apple-darwin"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, BUILD, BINARY, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_PRECONDITIONS_REFRESH_2026-08-23.md",
    "docs/stage-8/STAGE8B_P_PRECONDITIONS_ACCEPTANCE_MATRIX_2026-08-23.csv",
    "docs/stage-8/STAGE8B_P_PRECONDITIONS_NEGATIVE_INVENTORY_2026-08-23.md",
    "docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-23.json",
    "docs/stage-8/stage8b-p-build-identity-2026-08-23.json",
    "docs/stage-8/stage8b-p-governance-observation-2026-08-23.json",
    "docs/stage-8/stage8b-p-preconditions-authority.json",
    "scripts/stage8b_p_preconditions_check.py",
    "scripts/stage8b_p_preconditions_negative_harness.py",
    "scripts/stage8b_p_contract_refresh.py",
    "scripts/stage8b_p_build_repro.py",
    "scripts/stage8b_p_governance_refresh.py",
    "scripts/stage8b_p_preconditions_gate.sh",
    "scripts/make_stage8b_p_preconditions_handoff.py",
    "scripts/stage8b_p_preconditions_handoff_safety_check.py",
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
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3", ".pem", ".key")):
                raise ValueError(f"runtime/key artifact: {info.filename}")

        marker = dict(line.split("=", 1) for line in archive.read("handoff-commit.txt").decode().splitlines() if "=" in line)
        evidence = json.loads(archive.read(EVIDENCE))
        build = json.loads(archive.read(BUILD))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "8B-P-PRECONDITIONS" or evidence.get("revision") != "R2":
            raise ValueError("stage/revision mismatch")
        if evidence.get("contract_accepted") is not True or evidence.get("build_accepted") is not True:
            raise ValueError("technical prerequisite evidence missing")
        if evidence.get("governance_ready") is not True or evidence.get("all_prerequisites_accepted") is not False:
            raise ValueError("governance fail-closed status missing")
        for key in ("stage8b_p", "stage8b_xe", "finam_post_delete", "broker_effect", "redis_execution", "broker_dispatch", "runtime_live", "real_orders", "stage12"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read(GATE)
        if b"stage8b-p-preconditions-gate: PASS revision=R3 rows=48 negatives=57" not in gate:
            raise ValueError("gate marker missing")
        if b"stage8b-p-contract-refresh: PASS responses=7 material_drift=false" not in gate:
            raise ValueError("fresh contract marker missing")
        if b"stage8b-p-build-repro: PASS builds=2" not in gate:
            raise ValueError("reproducible build marker missing")
        if b"stage8b-p-governance-refresh: PASS ruleset=20111805 enforcement=active" not in gate:
            raise ValueError("live governance marker missing")
        if b"stage8b-p-full-regression: PASS current-tree=true debug=true release=true doc=true clippy=true redis-shadow=true runtime-bridge=true" not in gate:
            raise ValueError("full regression marker missing")
        binary = archive.read(BINARY)
        if sha(binary) != build.get("executable_sha256") or len(binary) != build.get("executable_size"):
            raise ValueError("executable evidence mismatch")
        if build.get("all_hashes_identical") is not True or build.get("executable_invoked") is not False:
            raise ValueError("build report mismatch")
        if sha(gate) != evidence.get("gate_sha256") or sha(archive.read(BUILD)) != evidence.get("build_report_sha256") or sha(binary) != evidence.get("executable_sha256") or sha(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("generated evidence hash mismatch")

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
            if len(data) != entry["size"] or sha(data) != entry["sha256"] or mode(by_name[name]) != entry["mode"]:
                raise ValueError(f"manifest content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {"archive_members": len(names), "tracked_members_verified": len(tracked), "duplicates": 0, "symlinks": 0, "unsafe_paths": 0, "source_ref": source_ref, "result": "PASS"}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_preconditions_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-preconditions-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p-preconditions-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
