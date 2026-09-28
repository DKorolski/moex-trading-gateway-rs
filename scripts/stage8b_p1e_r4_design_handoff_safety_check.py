#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-e R4 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1e-r4-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1e-r4-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
R3_REVIEW = "handoff-evidence/P1e_R3_design_engineering_review_913424b.md"
R4_ASSIGNMENT = "handoff-evidence/P1e_R4_design_correction_assignment.md"
R3_REVIEW_SHA256 = "e34d3992d40bdc0a488ef3c491d96c05e3793763464f676780f2594a5e66427b"
R4_ASSIGNMENT_SHA256 = "ee3c4c32e8b56a58084007beef8cbb23fb6fbb5189f5643be38070eb5baa8c20"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, R3_REVIEW, R4_ASSIGNMENT}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r4.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r4-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-active-acceptance-contract-v4.json",
    "docs/stage-8/stage8b-p1e-semantic-authority-registry-v4.json",
    "docs/stage-8/stage8b-p1e-deployment-identity-v1.json",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v3.json",
    "docs/stage-8/stage8b-p1e-first-boot-receipt-v2.json",
    "docs/stage-8/stage8b-p1e-derived-digests-v1.json",
    "docs/stage-8/stage8b-p1e-derived-digests-v1-golden.json",
    "docs/stage-8/stage8b-p1e-acquisition-model-v2.json",
    "docs/stage-8/stage8b-p1e-source-timer-precedence-v1.json",
    "docs/stage-8/stage8b-p1e-operational-pretransition-matrix-v4.csv",
    "docs/stage-8/stage8b-p1e-authenticated-restart-package-v2.json",
    "scripts/stage8b_p1e_r4_design_check.py",
    "scripts/stage8b_p1e_r4_design_negative_harness.py",
    "scripts/stage8b_p1e_r4_design_gate.sh",
}


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
            if member.parts and member.parts[0] in {".git", "target", "tmp", "reports", "__MACOSX"}:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith((".log", ".sqlite", ".sqlite3", ".pem", ".key", ".ed25519")):
                raise ValueError(f"runtime/key artifact: {info.filename}")

        marker = dict(
            line.split("=", 1) for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("status") != "R4_DESIGN_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("parent") != "913424b73c5a83df2131a1f9a2901b78035bfc4c":
            raise ValueError("parent mismatch")
        if evidence.get("active_rows") != 182 or evidence.get("operational_rows") != 52:
            raise ValueError("contract inventory mismatch")
        if evidence.get("semantic_authority_keys") != 14 or evidence.get("negative_cases") != 48:
            raise ValueError("semantic/negative inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("source_implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        if sha256(archive.read(R3_REVIEW)) != R3_REVIEW_SHA256 or evidence.get("bundled_r3_review_sha256") != R3_REVIEW_SHA256:
            raise ValueError("R3 review binding")
        if sha256(archive.read(R4_ASSIGNMENT)) != R4_ASSIGNMENT_SHA256 or evidence.get("bundled_r4_assignment_sha256") != R4_ASSIGNMENT_SHA256:
            raise ValueError("R4 assignment binding")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1e-r4-design-scope files=20",
            b"PASS stage8b-p1e-r4-design-negative-harness 48/48 redigested=true",
            b"PASS stage8b-p1e-r4-active-contract rows=182",
            b"PASS stage8b-p1e-r4-semantic-authority keys=14 conflicts=0",
            b"PASS stage8b-p1e-r4-design-gate",
        ):
            if expected not in gate:
                raise ValueError(f"gate marker missing: {expected!r}")
        if sha256(gate) != evidence.get("gate_sha256"):
            raise ValueError("gate digest mismatch")
        if sha256(archive.read(MANIFEST)) != evidence.get("manifest_sha256"):
            raise ValueError("manifest digest mismatch")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")

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
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or archive_mode(by_name[name]) != entry["mode"]:
                raise ValueError(f"manifest content mismatch: {name}")
        if set(names) - tracked != GENERATED:
            raise ValueError("generated member inventory mismatch")
        return {
            "archive_members": len(names), "tracked_members_verified": len(tracked),
            "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": source_ref, "stage": "Stage 8B-P1-e R4 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_r4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r4-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-r4-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
