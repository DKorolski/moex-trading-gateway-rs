#!/usr/bin/env python3
"""Validate an immutable controlled-installation Implementation R0 preflight handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8b-p-r2b-controlled-installation-impl-r0-preflight-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-controlled-installation-impl-r0-preflight-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_2026-08-30.md",
    "docs/stage-8/STAGE8B_P_R2B_CONTROLLED_INSTALLATION_IMPL_R0_PREFLIGHT_ACCEPTANCE_MATRIX_2026-08-30.csv",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-staging-inventory.json",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json",
    "docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-reset-uninstall.json",
    "deploy/stage8b-r2b-proof/stage8b-r2b-controlled-proof-trigger.service",
    "scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py",
    "scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_negative_harness.py",
    "scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_gate.sh",
    "scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_handoff_safety_check.py",
    "scripts/make_stage8b_p_r2b_controlled_installation_impl_r0_preflight_handoff.py",
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
        authority = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json"))
        inventory = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-staging-inventory.json"))
        ceremony = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json"))
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive-name binding mismatch")
        if evidence.get("source_tree") != marker.get("source_tree"):
            raise ValueError("tree binding mismatch")
        if evidence.get("accepted_design_ref") != "1e4db79288b0809fd5975edfdd0fc14740bcc8c6":
            raise ValueError("accepted design binding mismatch")
        if evidence.get("accepted_design_archive_sha256") != "5d55ccd8a585d6da780531aa237c9fba215328bce502b1099a8dc5aa3c22faea":
            raise ValueError("accepted design archive mismatch")
        if evidence.get("authorization") != "NOT_ISSUED" or authority.get("authorization") != "NOT_ISSUED":
            raise ValueError("authorization opened")
        if authority.get("status") != "PROOF_SEMANTICS_TRIGGER_CLEANUP_CLOSURE_REVIEW_REQUIRED_NOT_EXECUTED":
            raise ValueError("preflight status drift")
        if any(authority.get("execution_state", {}).values()):
            raise ValueError("preflight claims execution")
        if inventory.get("status") != "PLANNED_NOT_CREATED" or ceremony.get("status") != "PLANNED_NOT_MATERIALIZED":
            raise ValueError("planned state drift")
        for key in ("container_created", "installed", "enabled", "started", "ceremony_executed", "proof_executed", "finam_open", "runtime_live"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed handoff surface opened: {key}")
        if evidence.get("production_aggregate_expected_success") is not False:
            raise ValueError("production aggregate must be expected fail-closed")
        if evidence.get("outer_runner_expected_success") is not True:
            raise ValueError("outer runner must recognize the expected fail-closed result")
        if evidence.get("binary_count") != 12 or evidence.get("unit_target_count") != 19 or evidence.get("negative_mutations") != 40:
            raise ValueError("preflight inventory drift")
        gate = archive.read(GATE)
        if b"stage8b-p-r2b-controlled-installation-impl-r0-preflight-gate: PASS" not in gate or sha256(gate) != evidence.get("gate_sha256"):
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
            "archive_members": len(names), "tracked_members_verified": len(tracked),
            "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": source_ref, "binary_count": 12, "unit_target_count": 19,
            "execution": False, "authorization": "NOT_ISSUED", "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_controlled_installation_impl_r0_preflight_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-impl-r0-preflight-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-controlled-installation-impl-r0-preflight-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
