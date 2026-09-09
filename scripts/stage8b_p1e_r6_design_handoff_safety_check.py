#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-e R6 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1e-r6-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1e-r6-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
R5_REVIEW = "handoff-evidence/P1e_R5_design_engineering_review_3232c24.md"
R5_REVIEW_SHA256 = "de1eb40aaadcab098170f839b0acbcfe1f28845b896bb473d106a118f059141f"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, R5_REVIEW}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r6.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r6-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-active-acceptance-contract-v6.json",
    "docs/stage-8/stage8b-p1e-semantic-authority-registry-v6.json",
    "docs/stage-8/stage8b-p1e-acquisition-model-v3.json",
    "docs/stage-8/stage8b-p1e-operational-pretransition-overlay-v6.json",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v5.json",
    "docs/stage-8/stage8b-p1e-source-timer-precedence-v3.json",
    "scripts/stage8b_p1e_r6_design_check.py",
    "scripts/stage8b_p1e_r6_design_negative_harness.py",
    "scripts/stage8b_p1e_r6_design_gate.sh",
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
        if evidence.get("status") != "R6_DESIGN_REVIEW_CANDIDATE" or evidence.get("parent") != "3232c2447fc8d6aec038ca518efa11dc4d7e959e":
            raise ValueError("candidate lineage mismatch")
        expected_counts = (242, 26, 15, 5, 51, 40)
        actual_counts = tuple(evidence.get(key) for key in (
            "active_rows", "semantic_authority_keys", "reclaim_required_routes",
            "terminal_logical_routes", "operational_composed_rows", "negative_cases",
        ))
        if actual_counts != expected_counts:
            raise ValueError("contract inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("source_implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        if sha256(archive.read(R5_REVIEW)) != R5_REVIEW_SHA256 or evidence.get("bundled_r5_review_sha256") != R5_REVIEW_SHA256:
            raise ValueError("R5 review binding")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1e-r6-design-scope files=16",
            b"PASS stage8b-p1e-r6-design-negative-harness 40/40 redigested=true",
            b"PASS stage8b-p1e-r6-active-contract rows=242",
            b"PASS stage8b-p1e-r6-acquisition reclaim_required=15 terminal=5 terminal_XAUTOCLAIM=0",
            b"PASS stage8b-p1e-r6-operational base=56 excluded=16 replacements=11 composed=51",
            b"PASS stage8b-p1e-r6-first-boot stale_receipt_temp=conflict-before-marker-mutation",
            b"PASS stage8b-p1e-r6-design-gate",
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
            "source_ref": source_ref, "stage": "Stage 8B-P1-e R6 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_r6_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r6-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-r6-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
