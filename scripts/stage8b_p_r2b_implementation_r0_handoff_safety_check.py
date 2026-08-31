#!/usr/bin/env python3
"""Validate immutable Stage 8B-P R2B Implementation Package R0 handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p-r2b-implementation-r0-evidence.json"
GATE = "handoff-evidence/stage8b-p-r2b-implementation-r0-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_PACKAGE_R0_2026-08-29.md",
    "docs/stage-8/STAGE8B_P_R2B_IMPLEMENTATION_R0_ACCEPTANCE_MATRIX_2026-08-29.csv",
    "docs/stage-8/stage8b-p-r2b-implementation-r0-authority.json",
    "docs/stage-8/stage8b-p-r2b-implementation-r0-evidence.json",
    "scripts/stage8b_p_r2b_implementation_r0_check.py",
    "scripts/stage8b_p_r2b_implementation_r0_negative_harness.py",
    "scripts/stage8b_p_r2b_implementation_r0_gate.sh",
    "scripts/stage8b_p_r2b_issuance_systemd_check.py",
    "scripts/stage8b_p_r2b_issuance_target_systemd_verify.sh",
    "scripts/stage8b_p_r2b_implementation_r0_handoff_safety_check.py",
    "scripts/make_stage8b_p_r2b_implementation_r0_handoff.py",
    "tools/stage8b-readonly-preflight/src/r2a5.rs",
    "tools/stage8b-readonly-preflight/src/bin/stage8b-r2b-run-package-draft-builder.rs",
    "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target",
    "deploy/stage8b-r2b/moex-stage8b-r2b-run-package-draft-builder.service",
    "deploy/stage8b-r2b/moex-stage8b-r2b-package-issuer.service",
}
PREDECESSOR = "ebec9a100c92872134f3de91644cec50e2ed073a"


def sha(data: bytes) -> str:
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
        authority = json.loads(archive.read("docs/stage-8/stage8b-p-r2b-implementation-r0-authority.json"))
        source_manifest_bytes = archive.read(MANIFEST)
        source_manifest = json.loads(source_manifest_bytes)
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or source_manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive-name binding mismatch")
        if evidence.get("source_tree") != marker.get("source_tree"):
            raise ValueError("tree binding mismatch")
        if evidence.get("accepted_predecessor") != PREDECESSOR:
            raise ValueError("predecessor binding mismatch")
        if evidence.get("authorization") != "NOT_ISSUED":
            raise ValueError("authorization opened")
        if evidence.get("phase_count") != 6 or evidence.get("service_invocations") != 31:
            raise ValueError("transaction arithmetic drift")
        if evidence.get("negative_mutations") != 70 or evidence.get("acceptance_rows") != 52:
            raise ValueError("acceptance evidence drift")
        for key in ("installed", "enabled", "started", "operator_selected", "run_nonce_present", "credential_present", "unsigned_package_present", "signed_package_present", "finam_open", "runtime_live"):
            if evidence.get(key) is not False:
                raise ValueError(f"closed handoff surface opened: {key}")
        if authority["accepted_predecessor"]["source_ref"] != PREDECESSOR:
            raise ValueError("authority predecessor drift")
        if authority["authorization"]["r2b"] != "NOT_ISSUED":
            raise ValueError("authority status drift")
        for relative, expected in authority["implementation_artifacts"].items():
            if relative not in members or sha(archive.read(relative)) != expected:
                raise ValueError(f"implementation artifact mismatch: {relative}")

        gate = archive.read(GATE)
        gate_marker = b"stage8b-p-r2b-implementation-r0-gate: PASS predecessor=" + PREDECESSOR.encode()
        if gate_marker not in gate or sha(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate evidence mismatch")
        if sha(source_manifest_bytes) != evidence.get("manifest_sha256"):
            raise ValueError("source manifest digest mismatch")

        tracked: set[str] = set()
        entries = source_manifest.get("entries", [])
        if source_manifest.get("entry_count") != len(entries):
            raise ValueError("source manifest count mismatch")
        for entry in entries:
            name = entry["path"]
            if name in tracked or name not in members:
                raise ValueError(f"source member mismatch: {name}")
            tracked.add(name)
            data = archive.read(name)
            if len(data) != entry["size"] or sha(data) != entry["sha256"] or mode(members[name]) != entry["mode"]:
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
            "phase_count": 6,
            "service_invocations": 31,
            "authorization": "NOT_ISSUED",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p_r2b_implementation_r0_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (KeyError, OSError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-p-r2b-implementation-r0-handoff-safety: FAIL {error}") from error
    print("stage8b-p-r2b-implementation-r0-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
