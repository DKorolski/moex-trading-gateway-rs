#!/usr/bin/env python3
"""Validate an immutable controlled-installation R0 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

PREDECESSOR = "6672819e357a3c2a2c1e73e5408c393da01913a1"
EVIDENCE = "handoff-evidence/stage8b-p-r2b-controlled-installation-r0-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-controlled-installation-r0-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_R0_2026-08-30.md",
    "docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_R0_ACCEPTANCE_MATRIX_2026-08-30.csv",
    "docs/stage-8/stage8b-p-r2b-preproduction-supersession.json",
    "docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json",
    "scripts/stage8b_p_r2b_controlled_installation_r0_check.py",
    "scripts/stage8b_p_r2b_controlled_installation_r0_negative_harness.py",
    "scripts/stage8b_p_r2b_controlled_installation_r0_gate.sh",
    "scripts/stage8b_p_r2b_controlled_installation_r0_handoff_safety_check.py",
    "scripts/make_stage8b_p_r2b_controlled_installation_r0_handoff.py",
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        members = {item.filename: item for item in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")
        for item in infos:
            member = PurePosixPath(item.filename)
            item_mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if item_mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if item_mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {item.filename}")
            if ".env" in member.parts or item.filename.endswith((".log", ".pem", ".key", ".sqlite", ".sqlite3")):
                raise ValueError(f"secret/runtime artifact: {item.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        authority = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json"))
        transaction = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json"))
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive-name binding mismatch")
        if evidence.get("source_tree") != marker.get("source_tree"):
            raise ValueError("tree binding mismatch")
        if evidence.get("accepted_predecessor") != PREDECESSOR:
            raise ValueError("predecessor binding mismatch")
        if evidence.get("authorization") != "NOT_ISSUED" or authority.get("authorization") != "NOT_ISSUED":
            raise ValueError("authorization opened")
        if transaction.get("service_invocation_count") != 31 or evidence.get("service_invocations") != 31:
            raise ValueError("transaction arithmetic drift")
        if evidence.get("negative_mutations") != 20:
            raise ValueError("negative inventory drift")
        for key in ("installed", "enabled", "started", "operator_selected", "real_credentials_materialized", "finam_open", "runtime_live"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed handoff surface opened: {key}")
        for relative, expected in authority["design_artifacts"].items():
            if relative not in members or sha256(archive.read(relative)) != expected:
                raise ValueError(f"design artifact mismatch: {relative}")
        gate = archive.read(GATE)
        if b"stage8b-p-r2b-controlled-installation-r0-gate: PASS" not in gate or sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")
        if sha256(manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        tracked: set[str] = set()
        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or mode(members[name]) != entry["mode"]:
                raise ValueError(f"source content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": source_ref,
            "service_invocations": 31,
            "authorization": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_controlled_installation_r0_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-r0-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-controlled-installation-r0-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
