#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 R5 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-r5-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-r5-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
R4_REF = "ebede1d804f5eff50d6b4b9455edb08735e1be2c"
HASHES = {
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv": "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv": "74fc128b06d188942008449f05977d8eb46630c3ccfc364659a75d77d0e5810f",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv": "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v1.csv": "87168c9089def33072cae2561b5ad1c182c8d0d538b2abd6c1170e8ea577e3bd",
    "docs/stage-8/stage8b-p1d4-r5-acceptance-amendment.csv": "f6fcb398ce40ec96d21c87302806714429280d6737a7c8eb4e159dbb3e9437cb",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r5.md": "842e3a36ea348cb4c523adcdc3fd96e1f538599a5dd15f461bf55ae38ebf8ce9",
    "docs/stage-8/stage8b-p1d4-source-discovery-r5.md": "0c16ac5c2d6226e1d57a927e90c46a04ca7d44f535c9fe82a57c99b4dbd31d50",
}
REQUIRED = GENERATED | set(HASHES) | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r4.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r5.json",
    "scripts/make_stage8b_p1d4_r5_design_handoff.py",
    "scripts/stage8b_p1d4_r5_design_check.py",
    "scripts/stage8b_p1d4_r5_design_gate.sh",
    "scripts/stage8b_p1d4_r5_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r5_design_negative_harness.py",
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
            line.split("=", 1)
            for line in archive.read("handoff-commit.txt").decode().splitlines()
            if "=" in line
        )
        evidence = json.loads(archive.read(EVIDENCE))
        manifest = json.loads(archive.read(MANIFEST))
        source_ref = marker.get("source_ref")
        if not source_ref or evidence.get("source_ref") != source_ref or manifest.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if marker.get("parent_ref") != R4_REF or evidence.get("parent_ref") != R4_REF:
            raise ValueError("R4 parent mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if marker.get("branch") != "stage8b-paper-shadow-resumption":
            raise ValueError("source branch mismatch")

        if evidence.get("stage") != "Stage 8B-P1-d4 crash/replay design R5 correction":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_R5_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("active_positive_cells") != 102:
            raise ValueError("active proof inventory mismatch")
        if evidence.get("targeted_negative_cases") != 32 or evidence.get("total_contract_negative_cases") != 160:
            raise ValueError("negative inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("source_wip_included") is not False:
            raise ValueError("source WIP entered package")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        composition = evidence.get("composition_contract", {})
        if composition.get("option") != "A_RETAIN_SINGLE_SOURCE" or composition.get("source_m10_count") != 1:
            raise ValueError("retained-source choice drifted")
        if composition.get("source_absent_before_s_truth_allowed") is not False:
            raise ValueError("source-absent path opened")

        for name, expected in HASHES.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"content hash mismatch: {name}")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")
        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r5-design-scope",
            b"PASS stage8b-p1d4-r5-design-negative-harness 32/32",
            b"PASS stage8b-p1d4-r5-matrices",
            b"PASS stage8b-p1d4-r5-design-gate",
        ):
            if expected not in gate:
                raise ValueError(f"gate marker missing: {expected!r}")
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
            if len(data) != entry["size"] or sha256(data) != entry["sha256"] or archive_mode(by_name[name]) != entry["mode"]:
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
            "stage": "Stage 8B-P1-d4 R5 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_r5_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-r5-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-r5-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
