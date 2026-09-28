#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-e R8 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1e-r8-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1e-r8-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
R7_REVIEW = "handoff-evidence/P1e_R7_design_engineering_review_7ebdcef.md"
R7_REVIEW_SHA256 = "ec2fcaa9cb43813421109f6d1ab46200228bdf4cecc81cfe84fd733f941e4f35"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, R7_REVIEW}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r8.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r8-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-active-acceptance-contract-v8.json",
    "docs/stage-8/stage8b-p1e-semantic-authority-registry-v8.json",
    "docs/stage-8/stage8b-p1e-latch-aware-source-seam-v2.json",
    "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v3.csv",
    "docs/stage-8/stage8b-p1e-latch-route-transition-matrix-v1.json",
    "docs/stage-8/stage8b-p1e-shutdown-intent-v1.json",
    "docs/stage-8/stage8b-p1e-i0-regression-gate-v1.json",
    "scripts/stage8b_p1e_r8_design_check.py",
    "scripts/stage8b_p1e_r8_design_negative_harness.py",
    "scripts/stage8b_p1e_r8_design_gate.sh",
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
        if evidence.get("status") != "R8_DESIGN_REVIEW_CANDIDATE" or evidence.get("parent") != "7ebdcef45c1c55f4783bd6b2b1502ea78d4d97d7":
            raise ValueError("candidate lineage mismatch")
        expected_counts = (279, 31, 15, 5, 19, 11, 30, 25, 9, 51, 45)
        actual_counts = tuple(evidence.get(key) for key in (
            "active_rows", "semantic_authority_keys", "reclaim_logical_routes",
            "terminal_logical_routes", "reclaim_route_variants", "terminal_route_variants",
            "route_transition_rows", "supervisor_event_rows", "shutdown_cross_product_cases",
            "operational_composed_rows_inherited", "negative_cases",
        ))
        if actual_counts != expected_counts:
            raise ValueError("contract inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("source_modified") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("i0_source_seam_authorized_now") is not False or evidence.get("supervisor_source_implementation_authorized") is not False:
            raise ValueError("source boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        if sha256(archive.read(R7_REVIEW)) != R7_REVIEW_SHA256 or evidence.get("bundled_r7_review_sha256") != R7_REVIEW_SHA256:
            raise ValueError("R7 review binding")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1e-r8-design-scope files=17",
            b"PASS stage8b-p1e-r8-design-negative-harness 45/45 redigested=true",
            b"PASS stage8b-p1e-r8-active-contract rows=279",
            b"PASS stage8b-p1e-r8-terminal-protocol",
            b"PASS stage8b-p1e-r8-route-transitions rows=30",
            b"PASS stage8b-p1e-r8-shutdown-intent cases=9",
            b"PASS stage8b-p1e-r8-i0-regression p1d4_sigkill=105x2",
            b"PASS stage8b-p1e-r8-design-gate",
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
            "source_ref": source_ref, "stage": "Stage 8B-P1-e R8 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_r8_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r8-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-r8-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
