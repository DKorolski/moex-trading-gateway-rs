#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2A6 source/build handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r2a6-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2a6-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
BINARIES = {
    "stage8b-r2a6-source-adapter": "handoff-evidence/linux-amd64/stage8b-r2a6-source-adapter",
    "stage8b-r2a5-authority-producer": "handoff-evidence/linux-amd64/stage8b-r2a5-authority-producer",
    "stage8b-r2a5-authority-issuer": "handoff-evidence/linux-amd64/stage8b-r2a5-authority-issuer",
    "stage8b-r2a5-package-issuer": "handoff-evidence/linux-amd64/stage8b-r2a5-package-issuer",
    "stage8b-r2a5-controlled-layout": "handoff-evidence/linux-amd64/stage8b-r2a5-controlled-layout",
    "stage8b-r2a5-controlled-server": "handoff-evidence/linux-amd64/stage8b-r2a5-controlled-server",
    "stage8b-readonly-preflight": "handoff-evidence/linux-amd64/stage8b-readonly-preflight",
    "stage8b-r2a5-launcher": "handoff-evidence/linux-amd64/stage8b-r2a5-launcher",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, *BINARIES.values()}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2A6_ACCEPTANCE_MATRIX_2026-08-27.csv",
    "docs/stage-8/STAGE8B_P_R2A6_SOURCE_ADAPTER_INTEGRATION_2026-08-27.md",
    "docs/stage-8/stage8b-p-r2a6-build-evidence.json",
    "docs/stage-8/stage8b-p-r2a6-status.json",
    "deploy/stage8b-r2a5/stage8b-r2a6-source-adapter@.service",
    "deploy/stage8b-r2a5/stage8b-r2a6.tmpfiles",
    "scripts/stage8b_p_r2a6_gate.sh",
    "scripts/stage8b_p_r2a6_linux_rehearsal.sh",
    "scripts/stage8b_p_r2a6_negative_harness.py",
    "scripts/stage8b_p_r2a6_review_closure_check.py",
    "crates/finam-gateway/src/bin/stage8b-r2a6-source-adapter.rs",
    "crates/finam-gateway/src/stage8a1_execution_capability.rs",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
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
        build = json.loads(archive.read("docs/stage-8/stage8b-p-r2a6-build-evidence.json"))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("revision") != "R2A6" or evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("stage or authorization mismatch")
        if evidence.get("r2a6_negative_mutations") != 19 or evidence.get("controlled_tests") != 65:
            raise ValueError("coverage mismatch")
        for key in (
            "credential_used", "real_auth_request_sent", "real_broker_get_sent",
            "operator_arm_issued", "dispatch_attempt_appended", "effect_transport_entered",
            "finam_order_post_delete_sent", "broker_effect", "r2b_authorized",
            "redis_execution", "runtime_live", "real_orders",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        gate = archive.read(GATE)
        if b"stage8b-p-r2a6-gate: PASS" not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")

        expected = {
            "stage8b-r2a6-source-adapter": build["adapter"]["build_a_sha256"],
            "stage8b-readonly-preflight": build["accepted_r2a5_helper"]["executable_sha256"],
            "stage8b-r2a5-launcher": build["accepted_r2a5_helper"]["launcher_sha256"],
            **{
                name: digest
                for name, digest in build["r2a6_downstream_tools"].items()
                if name not in {"cargo_command", "reproducible"}
            },
        }
        for name, member in BINARIES.items():
            if sha(archive.read(member)) != expected[name]:
                raise ValueError(f"Linux binary mismatch: {name}")
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
            "linux_binaries_verified": len(BINARIES),
            "source_ref": source_ref,
            "authorization_status": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2a6_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p-r2a6-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p-r2a6-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
