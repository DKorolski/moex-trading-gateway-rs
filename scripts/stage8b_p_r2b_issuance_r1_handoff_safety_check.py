#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2B issuance R0-R1 handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r2b-issuance-r0-r1-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-issuance-r0-r1-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_R1_ACCEPTANCE_MATRIX_2026-08-29.csv",
    "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json",
    "docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-evidence.json",
    "docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json",
    "docs/stage-8/stage8b-p-r2a3-finam-read-contract-snapshot.json",
    "scripts/stage8b_p_r2b_read_contract_refresh.py",
    "scripts/stage8b_p_r2b_issuance_r1_check.py",
    "scripts/stage8b_p_r2b_issuance_r1_negative_harness.py",
    "scripts/stage8b_p_r2b_issuance_r1_gate.sh",
    "scripts/make_stage8b_p_r2b_issuance_r1_handoff.py",
    "scripts/stage8b_p_r2b_issuance_r1_handoff_safety_check.py",
}
SNAPSHOT_SHA = "7c8e6bcd02f907af93ea1386499d03bff194da76a1eb2b19dd9c2ff1f97403c5"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def member_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        by_name = {info.filename: info for info in infos}
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
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
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if ".env" in member.parts or info.filename.endswith((".log", ".pem", ".key", ".sqlite", ".sqlite3")):
                raise ValueError(f"secret/runtime artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest_bytes = archive.read(MANIFEST)
        manifest = json.loads(manifest_bytes)
        authority = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-issuance-package-r0-r1-authority.json"))
        refresh = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-r0-r1-read-contract-refresh-evidence.json"))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "Stage 8B-P R2B Issuance Package R0-R1":
            raise ValueError("stage mismatch")
        if evidence.get("accepted_predecessor") != "f24f1044ac0b29c2f588853b817e519cfe8d3d8b":
            raise ValueError("accepted predecessor mismatch")
        if evidence.get("corrected_r0") != "928168ed47e5b9dd873cd73815fbccecde7a8981":
            raise ValueError("R0 correction binding mismatch")
        if evidence.get("read_contract_snapshot_sha256") != SNAPSHOT_SHA:
            raise ValueError("read contract binding mismatch")
        if evidence.get("read_document_count") != 6 or evidence.get("service_invocations") != 31:
            raise ValueError("contract/transaction count mismatch")
        if evidence.get("phase_count") != 6 or evidence.get("negative_mutations") != 25:
            raise ValueError("phase/negative count mismatch")
        if evidence.get("acceptance_rows") != 40:
            raise ValueError("acceptance count mismatch")
        if evidence.get("draft_builder_implemented") is not False or evidence.get("activation_target_implemented") is not False:
            raise ValueError("implementation unexpectedly present")
        if evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("authorization unexpectedly issued")
        for key in (
            "finam_credentials_accessed", "auth_service_called", "broker_account_get_sent",
            "order_post_sent", "order_delete_sent", "dispatch_attempt_recorded",
            "transport_entered", "redis_live_consumer", "broker_dispatch", "runtime_live",
            "strategy_live", "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        if authority["transaction"]["service_invocation_count"] != 31:
            raise ValueError("authority transaction drift")
        if authority["package_formation"]["selected_model"] != "SEPARATE_DRAFT_BUILDER_THEN_SIGNER":
            raise ValueError("authority builder model drift")
        if authority["authorization"]["r2b"] != "NOT_ISSUED":
            raise ValueError("authority status drift")
        if refresh["read_contract_snapshot_sha256"] != SNAPSHOT_SHA or refresh["document_count"] != 6:
            raise ValueError("refresh evidence drift")

        gate = archive.read(GATE)
        gate_marker = (
            b"stage8b-p-r2b-issuance-r1-gate: PASS revision=R0-R1 rows=40 "
            b"read_documents=6 snapshot_sha256=" + SNAPSHOT_SHA.encode() +
            b" services=31 phases=6 negative_mutations=25 builder=SEPARATE"
        )
        if gate_marker not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")
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
            if len(data) != entry["size"] or sha(data) != entry["sha256"] or member_mode(by_name[name]) != entry["mode"]:
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
            "read_document_count": 6,
            "service_invocations": 31,
            "activation_target_implemented": False,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_issuance_r1_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-issuance-r1-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-issuance-r1-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
