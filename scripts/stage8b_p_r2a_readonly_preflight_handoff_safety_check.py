#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2A handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r2a-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2a-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2A_ACCEPTANCE_MATRIX_2026-08-25.csv",
    "docs/stage-8/STAGE8B_P_R2A_NEGATIVE_INVENTORY_2026-08-25.md",
    "docs/stage-8/STAGE8B_P_R2A_READONLY_PREFLIGHT_CONTRACT_2026-08-25.md",
    "docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json",
    "scripts/stage8b_p_r2a_prepare.py",
    "scripts/stage8b_p_r2a_readonly_preflight_check.py",
    "scripts/stage8b_p_r2a_readonly_preflight_gate.sh",
    "scripts/stage8b_p_r2a_readonly_preflight_negative_harness.py",
    "scripts/make_stage8b_p_r2a_readonly_preflight_handoff.py",
    "scripts/stage8b_p_r2a_readonly_preflight_handoff_safety_check.py",
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
        if evidence.get("stage") != "8B-P" or evidence.get("revision") != "R2A":
            raise ValueError("stage/revision mismatch")
        if evidence.get("status") != "get_only_preflight_contract_candidate":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_predecessor_ref") != "f1070a428c884f846ed3a2007e38f2401b62e5ce":
            raise ValueError("predecessor mismatch")
        if evidence.get("accepted_r1b_ref") != "b9a423c4ffd96bf4a5f69027aa4fef4dcc503830":
            raise ValueError("R1B lineage mismatch")
        if evidence.get("acceptance_rows") != 48 or evidence.get("r2a_negative_mutations") != 40:
            raise ValueError("R2A coverage mismatch")
        if evidence.get("inherited_negative_mutations") != 134:
            raise ValueError("inherited coverage mismatch")
        if evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("authorization unexpectedly issued")
        for key in (
            "production_source_changed", "operator_selection_present", "account_credential_used",
            "token_details_get_sent", "broker_readonly_get", "operator_arm_issued",
            "dispatch_attempt_recorded", "effect_transport_entered", "finam_post_delete",
            "broker_effect", "stage8b_xe", "redis_execution", "broker_dispatch",
            "runtime_live", "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        if evidence.get("readonly_http_request_count") != 0:
            raise ValueError("read-only request unexpectedly sent")

        gate = archive.read(GATE)
        if b"stage8b-p-r2a-gate: PASS rows=48 negatives=40 inherited=134 plan_only=true" not in gate:
            raise ValueError("R2A gate marker missing")
        if b"stage8b-p-r1b-identity-gate: PASS rows=40 endpoint_goldens=2 run_goldens=2 new_negatives=36 inherited=98 total=134" not in gate:
            raise ValueError("inherited R1B marker missing")
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
        raise SystemExit("usage: stage8b_p_r2a_readonly_preflight_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-r2a-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p-r2a-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
