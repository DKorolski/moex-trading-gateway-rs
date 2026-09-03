#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-c source implementation handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1c-evidence.json"
GATE = "handoff-evidence/stage8b-p1c-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
REQUIRED = GENERATED | {
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "docs/stage-8/stage8b-p1c-real-redis-command-publication.md",
    "docs/stage-8/stage8b-p1c-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1c-evidence.json",
    "scripts/stage8b_p1c_check.py",
    "scripts/stage8b_p1c_negative_harness.py",
    "scripts/stage8b_p1c_gate.sh",
    "scripts/make_stage8b_p1c_handoff.py",
    "scripts/stage8b_p1c_handoff_safety_check.py",
}
PREDECESSOR = "ed6d98cb2bbc70c36e1033c6215d64dd6218cedf"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def archive_mode(info: zipfile.ZipInfo) -> str:
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
            mode = (info.external_attr >> 16) & 0o177777
            if member.is_absolute() or ".." in member.parts or "" in member.parts:
                raise ValueError(f"unsafe path: {info.filename}")
            if mode & 0o170000 == 0o120000:
                raise ValueError(f"symlink: {info.filename}")
            if mode & 0o170000 not in {0, 0o100000, 0o040000}:
                raise ValueError(f"special file: {info.filename}")
            if member.parts and member.parts[0] in {
                ".git", "target", "tmp", "reports", "__MACOSX"
            }:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith(
                (".log", ".sqlite", ".sqlite3", ".pem", ".key", ".ed25519")
            ):
                raise ValueError(f"runtime/key artifact: {info.filename}")

        marker = dict(
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "Stage 8B-P1-c":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_predecessor_ref") != PREDECESSOR:
            raise ValueError("predecessor mismatch")

        positive = evidence.get("positive_evidence", {})
        if positive.get("real_redis_scenarios") != "8/8 PASS":
            raise ValueError("real Redis evidence mismatch")
        if positive.get("command_response_loss_exactly_once") != "PASS":
            raise ValueError("response-loss evidence mismatch")
        negative = evidence.get("negative_evidence", {})
        if negative.get("source_scope_mutations") != "10/10 PASS":
            raise ValueError("negative evidence mismatch")
        verification = evidence.get("verification", {})
        expected_verification = {
            "format": "PASS",
            "source_scope_checker": "PASS",
            "negative_harness": "10/10 PASS",
            "real_redis_integration": "8/8 PASS",
            "p1b_inherited_negative_harness": "PASS",
            "strategy_runtime_core_lib": "1216/1216 PASS",
            "runtime_durable_service_lib": (
                "109 PASS; 3 intentional crash children ignored"
            ),
            "runtime_durable_service_doctests": "29/29 PASS",
            "strict_targeted_clippy": "PASS",
            "aggregate_gate": "PASS",
        }
        for key, expected in expected_verification.items():
            value = verification.get(key)
            if value != expected:
                raise ValueError(f"verification mismatch: {key}={value}")
        for key, value in evidence.get("closed_surfaces", {}).items():
            if value is not False:
                raise ValueError(f"closed surface opened: {key}")
        if len(evidence.get("closed_surfaces", {})) != 11:
            raise ValueError("closed surface inventory mismatch")

        gate = archive.read(GATE)
        for marker_text in (
            b"PASS stage8b-p1c-source-scope",
            b"PASS stage8b-p1c-negative-harness 10/10",
            b"PASS stage8b-p1c-gate",
        ):
            if marker_text not in gate:
                raise ValueError(f"gate marker missing: {marker_text!r}")
        if sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
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
            if (
                len(data) != entry["size"]
                or sha256(data) != entry["sha256"]
                or archive_mode(by_name[name]) != entry["mode"]
            ):
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
            "stage": "Stage 8B-P1-c",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1c_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1c-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1c-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
