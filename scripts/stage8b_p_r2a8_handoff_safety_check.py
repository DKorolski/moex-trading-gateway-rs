#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P R2A8 source/build handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

EVIDENCE = "handoff-evidence/stage8b-p-r2a8-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2a8-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
BINARIES = {
    "production_adapter": "handoff-evidence/linux-amd64/production/stage8b-r2a7-source-adapter",
    "production_issuer": "handoff-evidence/linux-amd64/production/stage8b-r2a8-current-manifest-issuer",
    "controlled_adapter": "handoff-evidence/linux-amd64/controlled/stage8b-r2a7-source-adapter",
    "controlled_seeder": "handoff-evidence/linux-amd64/controlled/stage8b-r2a7-controlled-seeder",
    "controlled_issuer": "handoff-evidence/linux-amd64/controlled/stage8b-r2a8-current-manifest-issuer",
    "authority_producer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-authority-producer",
    "authority_issuer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-authority-issuer",
    "package_issuer": "handoff-evidence/linux-amd64/tools/stage8b-r2a5-package-issuer",
    "accepted_helper": "handoff-evidence/linux-amd64/accepted/stage8b-readonly-preflight",
    "accepted_launcher": "handoff-evidence/linux-amd64/accepted/stage8b-r2a5-launcher",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, *BINARIES.values()}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2A8_TRUSTED_CURRENT_SOURCE_2026-08-27.md",
    "docs/stage-8/stage8b-p-r2a8-build-evidence.json",
    "docs/stage-8/stage8b-p-r2a8-status.json",
    "crates/finam-gateway/src/bin/stage8b-r2a8-current-manifest-issuer.rs",
    "crates/finam-gateway/src/stage8b_r2a7_source_adapter.rs",
    "deploy/stage8b-r2a5/stage8b-r2a8-current-manifest-issuer.service",
    "scripts/stage8b_p_r2a8_gate.sh",
    "scripts/stage8b_p_r2a8_negative_harness.py",
    "scripts/stage8b_p_r2a8_review_closure_check.py",
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        if len(names) != len(set(names)):
            raise ValueError("duplicate members")
        if missing := REQUIRED - set(names):
            raise ValueError(f"missing members: {sorted(missing)}")
        for item in infos:
            member = PurePosixPath(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {item.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {item.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {item.filename}")
            if member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {item.filename}")
            if ".env" in member.parts or item.filename.endswith((".log", ".pem", ".key")):
                raise ValueError(f"secret/runtime artifact: {item.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        build = json.loads(archive.read("docs/stage-8/stage8b-p-r2a8-build-evidence.json"))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("revision") != "R2A8" or evidence.get("authorization_status") != "NOT_ISSUED":
            raise ValueError("stage/authorization mismatch")
        if evidence.get("negative_mutations") != 13:
            raise ValueError("negative coverage mismatch")
        if evidence.get("controlled_place_full_chain") is not True or evidence.get("controlled_cancel_full_chain") is not True:
            raise ValueError("full-chain coverage mismatch")
        for closed in (
            "credential_used",
            "finam_network_accessed",
            "operator_arm_issued",
            "dispatch_entered",
            "effect_transport_entered",
            "finam_order_post_delete_sent",
            "r2b_authorized",
            "runtime_live",
            "real_orders",
        ):
            if evidence.get(closed) is not False:
                raise ValueError(f"closed surface opened: {closed}")
        gate = archive.read(GATE)
        if b"stage8b-p-r2a8-gate: PASS" not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")

        expected_hashes = {
            "production_adapter": build["production_binaries"]["stage8b-r2a7-source-adapter"]["build_a_sha256"],
            "production_issuer": build["production_binaries"]["stage8b-r2a8-current-manifest-issuer"]["build_a_sha256"],
            "controlled_adapter": build["controlled_linux_amd64_binaries"]["stage8b-r2a7-source-adapter"],
            "controlled_seeder": build["controlled_linux_amd64_binaries"]["stage8b-r2a7-controlled-seeder"],
            "controlled_issuer": build["controlled_linux_amd64_binaries"]["stage8b-r2a8-current-manifest-issuer"],
            "authority_producer": build["controlled_linux_amd64_binaries"]["stage8b-r2a5-authority-producer"],
            "authority_issuer": build["controlled_linux_amd64_binaries"]["stage8b-r2a5-authority-issuer"],
            "package_issuer": build["controlled_linux_amd64_binaries"]["stage8b-r2a5-package-issuer"],
            "accepted_helper": build["controlled_linux_amd64_binaries"]["accepted-stage8b-readonly-preflight"],
            "accepted_launcher": build["controlled_linux_amd64_binaries"]["accepted-stage8b-r2a5-launcher"],
        }
        for name, expected in expected_hashes.items():
            if sha(archive.read(BINARIES[name])) != expected:
                raise ValueError(f"binary mismatch: {name}")
        if sha(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")
        by_name = {item.filename: item for item in infos}
        tracked: set[str] = set()
        for entry in manifest["entries"]:
            name = entry["path"]
            data = archive.read(name)
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            if name in tracked or len(data) != entry["size"] or sha(data) != entry["sha256"] or mode != entry["mode"]:
                raise ValueError(f"manifest mismatch: {name}")
            tracked.add(name)
        if manifest["entry_count"] != len(tracked) or set(names) - tracked != GENERATED:
            raise ValueError("member inventory mismatch")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "linux_binaries_verified": len(BINARIES),
            "source_ref": source_ref,
            "authorization_status": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2a8_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2a8-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2a8-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
