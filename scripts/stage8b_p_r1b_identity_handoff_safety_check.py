#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R1B identity-correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r1b-identity-evidence.json"
GATE = "handoff-evidence/stage8b-p-r1b-identity-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R1B_ACCEPTANCE_MATRIX_2026-08-25.csv",
    "docs/stage-8/STAGE8B_P_R1B_IDENTITY_CORRECTION_2026-08-25.md",
    "docs/stage-8/STAGE8B_P_R1B_NEGATIVE_INVENTORY_2026-08-25.md",
    "docs/stage-8/stage8b-p-r1b-authorization-authority.json",
    "docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json",
    "docs/stage-8/stage8b-p-r1b-run-identity-authority.json",
    "scripts/stage8b_p_r1b_identity_check.py",
    "scripts/stage8b_p_r1b_identity_gate.sh",
    "scripts/stage8b_p_r1b_identity_negative_harness.py",
    "scripts/make_stage8b_p_r1b_identity_handoff.py",
    "scripts/stage8b_p_r1b_identity_handoff_safety_check.py",
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
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "8B-P" or evidence.get("revision") != "R1B":
            raise ValueError("stage/revision mismatch")
        if evidence.get("status") != "design_only_identity_correction_candidate":
            raise ValueError("candidate status mismatch")
        if evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("authorization unexpectedly issued")
        if evidence.get("accepted_predecessor_ref") != "16a59bca74f94881c70d9fa39bbdf1c357e65f95":
            raise ValueError("predecessor mismatch")
        if evidence.get("r1a_candidate_ref") != "f922ad65f7221488fcfc591d641b822f635b1993":
            raise ValueError("R1A lineage mismatch")
        if evidence.get("acceptance_rows") != 40 or evidence.get("r1b_negative_mutations") != 36:
            raise ValueError("R1B coverage mismatch")
        if evidence.get("inherited_negative_mutations") != 98 or evidence.get("total_negative_mutations") != 134:
            raise ValueError("total coverage mismatch")
        if evidence.get("endpoint_golden_vectors") != 2 or evidence.get("run_golden_vectors") != 2:
            raise ValueError("golden vector count mismatch")
        for key in (
            "production_source_changed", "account_credential_used", "broker_readonly_get",
            "operator_arm_issued", "dispatch_attempt_recorded", "transport_entered",
            "finam_post_delete", "broker_effect", "stage8b_p", "stage8b_xe",
            "redis_execution", "broker_dispatch", "runtime_live", "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")

        gate = archive.read(GATE)
        if b"stage8b-p-r1b-identity-gate: PASS rows=40 endpoint_goldens=2 run_goldens=2 new_negatives=36 inherited=98 total=134" not in gate:
            raise ValueError("R1B gate marker missing")
        if b"stage8b-p-r1a-authorization-negative: PASS 50/50 inherited_r1=48 total=98" not in gate:
            raise ValueError("inherited R1A marker missing")
        if sha(gate) != evidence.get("gate_sha256") or sha(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("embedded evidence digest mismatch")

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
        return {
            "archive_members": len(names), "tracked_members_verified": len(tracked),
            "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": source_ref, "authorization_status": "NOT_ISSUED", "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r1b_identity_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-r1b-identity-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p-r1b-identity-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
