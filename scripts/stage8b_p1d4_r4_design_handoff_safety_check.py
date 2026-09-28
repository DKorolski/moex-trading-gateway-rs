#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 R4 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-r4-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-r4-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
R3_REF = "e1ce6d3baec3974d8dfd05c2f3de00110e0605bf"
R3_DESIGN_SHA256 = "d4c844498b47b53a09fc916e7a110ce856725ad421e4dee1c4b641e85dc70e7a"
R3_MATRIX_SHA256 = "8c3122af016e860e3fa54a6f4143c50b9258c2a5a4bd2d15f9847a118a8582cc"
R4_DESIGN_SHA256 = "7e6c5a65eab957b92b61a58a4b6180c050f95305c8f35568e8e35140780b9b39"
R4_DISCOVERY_SHA256 = "37c0ca38f0098880beabbb343725d5010da9459b2e21041c26e0196a2c1c21a6"
R4_MATRIX_SHA256 = "74fc128b06d188942008449f05977d8eb46630c3ccfc364659a75d77d0e5810f"
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-source.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r4.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r4.json",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv",
    "docs/stage-8/stage8b-p1d4-source-discovery-r4.md",
    "scripts/make_stage8b_p1d4_r4_design_handoff.py",
    "scripts/stage8b_p1d4_r4_design_check.py",
    "scripts/stage8b_p1d4_r4_design_gate.sh",
    "scripts/stage8b_p1d4_r4_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r4_design_negative_harness.py",
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
        if not source_ref or evidence.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if evidence.get("parent_ref") != R3_REF:
            raise ValueError("R3 parent mismatch")
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if marker.get("branch") != "stage8b-paper-shadow-resumption":
            raise ValueError("source branch mismatch")

        if evidence.get("stage") != "Stage 8B-P1-d4 crash/replay design R4 correction":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_R4_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_r3_design_ref") != R3_REF:
            raise ValueError("accepted R3 mismatch")
        if evidence.get("design_r3_sha256") != R3_DESIGN_SHA256:
            raise ValueError("R3 design binding mismatch")
        if evidence.get("historical_r3_matrix_sha256") != R3_MATRIX_SHA256:
            raise ValueError("R3 matrix binding mismatch")
        if evidence.get("design_r4_sha256") != R4_DESIGN_SHA256:
            raise ValueError("R4 design binding mismatch")
        if evidence.get("discovery_r4_sha256") != R4_DISCOVERY_SHA256:
            raise ValueError("R4 discovery binding mismatch")
        if evidence.get("scenario_frontier_matrix_v4_sha256") != R4_MATRIX_SHA256:
            raise ValueError("R4 matrix binding mismatch")
        if evidence.get("acceptance_rows") != 88 or evidence.get("scenario_frontier_matrix_rows") != 92:
            raise ValueError("matrix inventory mismatch")
        if evidence.get("targeted_negative_cases") != 18 or evidence.get("total_negative_contract_cases") != 146:
            raise ValueError("negative inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("source_wip_included") is not False or evidence.get("new_persistence_schema_authorized") is not False:
            raise ValueError("source/schema boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")

        exact_hashes = {
            "docs/stage-8/stage8b-p1d4-crash-replay-design-r3.md": R3_DESIGN_SHA256,
            "docs/stage-8/stage8b-p1d4-crash-replay-design-r4.md": R4_DESIGN_SHA256,
            "docs/stage-8/stage8b-p1d4-source-discovery-r4.md": R4_DISCOVERY_SHA256,
            "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v3.csv": R3_MATRIX_SHA256,
            "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v4.csv": R4_MATRIX_SHA256,
        }
        for name, expected in exact_hashes.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"content hash mismatch: {name}")

        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")
        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r4-design-scope",
            b"PASS stage8b-p1d4-r4-design-negative-harness 18/18",
            b"PASS stage8b-p1d4-r4-general-matrix rows=88",
            b"PASS stage8b-p1d4-r4-cell-matrix cells=92",
            b"PASS stage8b-p1d4-r4-design-gate",
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
            "stage": "Stage 8B-P1-d4 R4 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_r4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-r4-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-r4-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
