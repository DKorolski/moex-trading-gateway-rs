#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2A2 handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r2a2-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2a2-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
BINARY = "handoff-evidence/stage8b-readonly-preflight-aarch64-apple-darwin"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, BINARY}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2A2_ACCEPTANCE_MATRIX_2026-08-25.csv",
    "docs/stage-8/STAGE8B_P_R2A2_NEGATIVE_INVENTORY_2026-08-25.md",
    "docs/stage-8/STAGE8B_P_R2A2_SEMANTIC_PROVENANCE_QUALIFICATION_2026-08-25.md",
    "docs/stage-8/stage8b-p-r2a2-build-evidence.json",
    "docs/stage-8/stage8b-p-r2a2-semantic-provenance-authority.json",
    "tools/stage8b-readonly-preflight/Cargo.toml",
    "tools/stage8b-readonly-preflight/Cargo.lock",
    "tools/stage8b-readonly-preflight/src/lib.rs",
    "tools/stage8b-readonly-preflight/src/main.rs",
    "tools/stage8b-readonly-preflight/src/r2a2.rs",
    "scripts/launch_stage8b_p_r2a2_qualified.sh",
    "scripts/stage8b_p_r2a2_semantic_provenance_check.py",
    "scripts/stage8b_p_r2a2_semantic_provenance_gate.sh",
    "scripts/stage8b_p_r2a2_semantic_provenance_negative_harness.py",
    "scripts/make_stage8b_p_r2a2_handoff.py",
    "scripts/stage8b_p_r2a2_handoff_safety_check.py",
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
            mode = (info.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {info.filename}")
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
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
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "8B-P" or evidence.get("revision") != "R2A2":
            raise ValueError("stage/revision mismatch")
        if evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("authorization issued")
        if evidence.get("new_negative_mutations") != 26 or evidence.get("inherited_negative_mutations") != 134:
            raise ValueError("coverage mismatch")
        if evidence.get("controlled_tests") != 22:
            raise ValueError("controlled test count mismatch")
        for key in (
            "credential_used", "real_auth_request_sent", "real_broker_get_sent",
            "operator_arm_issued", "dispatch_attempt_appended", "effect_transport_entered",
            "finam_order_post_delete_sent", "broker_effect", "r2b_authorized",
            "redis_execution", "runtime_live", "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read(GATE)
        if b"stage8b-p-r2a2-gate: PASS" not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")
        if sha(archive.read(BINARY)) != evidence.get("helper_executable_sha256"):
            raise ValueError("helper binary mismatch")
        if sha(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")

        tracked: set[str] = set()
        for entry in manifest.get("entries", []):
            name = entry["path"]
            if name in tracked or name not in by_name:
                raise ValueError(f"manifest member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            if len(data) != entry["size"] or sha(data) != entry["sha256"] or mode != entry["mode"]:
                raise ValueError(f"manifest content mismatch: {name}")
        if manifest.get("entry_count") != len(tracked) or set(names) - tracked != GENERATED:
            raise ValueError("member inventory mismatch")
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
        raise SystemExit("usage: stage8b_p_r2a2_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-r2a2-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p-r2a2-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
