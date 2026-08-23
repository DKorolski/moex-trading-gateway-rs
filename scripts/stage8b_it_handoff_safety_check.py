#!/usr/bin/env python3
"""Validate provenance, tracked bytes and closed surfaces for Stage 8B-IT."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-it-evidence.json"
GATE = "handoff-evidence/stage8b-it-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
REQUIRED = {
    "handoff-commit.txt",
    EVIDENCE,
    GATE,
    MANIFEST,
    "crates/finam-gateway/src/stage8b_no_send/stage8b_adapter.rs",
    "docs/stage-8/STAGE8B_IT_IMPLEMENTATION_2026-08-23.md",
    "docs/stage-8/STAGE8B_IT_ACCEPTANCE_MATRIX_2026-08-23.csv",
    "docs/stage-8/STAGE8B_IT_NEGATIVE_INVENTORY_2026-08-23.md",
    "docs/stage-8/stage8b-it-authority.json",
    "scripts/stage8b_it_check.py",
    "scripts/stage8b_it_negative_harness.py",
    "scripts/stage8b_it_external_compile_fail.sh",
    "scripts/stage8b_it_internal_compile_fail.sh",
    "scripts/stage8b_it_predecessor_replay.sh",
    "scripts/stage8b_it_gate.sh",
}
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
    return f"{(info.external_attr >> 16) & 0o177777:06o}"


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        info_by_name = {info.filename: info for info in infos}
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
            if member.parts and member.parts[0] in {
                ".git", "target", "tmp", "reports", "__MACOSX",
            }:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3")):
                raise ValueError(f"runtime artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        if evidence.get("stage") != "8B-IT":
            raise ValueError("stage mismatch")
        if evidence.get("revision") != "R2":
            raise ValueError("revision mismatch")
        if (
            evidence.get("source_ref") != marker.get("source_ref")
            or manifest.get("source_ref") != marker.get("source_ref")
        ):
            raise ValueError("source mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive mismatch")
        if evidence.get("acceptance_rows") != 72:
            raise ValueError("acceptance count mismatch")
        if evidence.get("negative_cases") != 60:
            raise ValueError("negative count mismatch")
        if evidence.get("external_compile_fail_negative_cases") != 12:
            raise ValueError("external compile-fail count mismatch")
        if evidence.get("internal_compile_fail_negative_cases") != 4:
            raise ValueError("internal compile-fail count mismatch")
        for key in (
            "adapter_qualified",
            "request_parts_module_private",
            "adapter_parent_only",
            "single_consuming_transition",
            "mandatory_classifier_inside_adapter",
            "classified_only_result",
            "canonical_full_regression",
            "accepted_predecessor_replay",
            "controlled_loopback_only",
            "single_transport_attempt",
            "accepted_builder_bridge",
            "accepted_classifier_bridge",
        ):
            if evidence.get(key) is not True:
                raise ValueError(f"qualification evidence missing: {key}")
        if evidence.get("controlled_tls_qualification") != "blocking_stage8b_p_precondition":
            raise ValueError("controlled TLS precondition drift")
        gate = archive.read(GATE)
        if b"stage8b-it-gate: PASS revision=R2 rows=72 negatives=60" not in gate:
            raise ValueError("R2 gate marker missing")
        if f"current-tree-ci-gate: PASS source_ref={evidence['source_ref']} ".encode() not in gate:
            raise ValueError("gate is not exact-commit bound")
        if b"stage8b-i-full-regression: PASS canonical_ci=true" not in gate:
            raise ValueError("canonical full regression missing")
        for key in (
            "production_endpoint_authority",
            "production_operator_arm",
            "broker_effect",
            "finam_network_send",
            "redis_execution",
            "broker_dispatch",
            "runtime_live",
            "real_orders",
            "stage8b_p",
            "stage8b_xe",
            "stage12",
        ):
            if evidence.get(key) is not False:
                raise ValueError(f"closed surface opened: {key}")
        if (
            sha256(archive.read(GATE)) != evidence.get("gate_sha256")
            or sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256")
        ):
            raise ValueError("generated evidence hash mismatch")

        entries = manifest.get("entries", [])
        if manifest.get("entry_count") != len(entries):
            raise ValueError("manifest count mismatch")
        tracked: set[str] = set()
        for entry in entries:
            name = entry["path"]
            if name in tracked:
                raise ValueError(f"duplicate manifest path: {name}")
            tracked.add(name)
            info = info_by_name.get(name)
            if info is None:
                raise ValueError(f"missing tracked member: {name}")
            data = archive.read(name)
            if (
                len(data) != entry["size"]
                or sha256(data) != entry["sha256"]
                or archive_mode(info) != entry["mode"]
            ):
                raise ValueError(f"manifest mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("unexpected generated member inventory")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": evidence["source_ref"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_it_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-it-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-it-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
