#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 design review handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
BASE = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R0 = "b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f"
CELL_MATRIX_SHA256 = "d1d765f1fa6db1dc948725273f58938c1d0cabd614d26076bf3ac2ddd1ad36ed"
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d0-deterministic-paper-execution-policy.md",
    "docs/stage-8/stage8b-p1d3-projection-recovery-annex-r1.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-lifecycle-design.md",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-source.md",
    "docs/stage-8/stage8b-p1d4-exhaustive-crash-replay-design.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r1.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json",
    "scripts/stage8b_p1d4_design_check.py",
    "scripts/stage8b_p1d4_design_negative_harness.py",
    "scripts/stage8b_p1d4_design_gate.sh",
    "scripts/make_stage8b_p1d4_design_handoff.py",
    "scripts/stage8b_p1d4_design_handoff_safety_check.py",
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
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "Stage 8B-P1-d4 exhaustive crash/replay closure design":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_R1_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_p1d3_closure_ref") != BASE:
            raise ValueError("accepted predecessor mismatch")
        if evidence.get("reviewed_r0_ref") != R0:
            raise ValueError("reviewed R0 mismatch")
        if evidence.get("acceptance_rows") != 72 or evidence.get("negative_cases") != 60:
            raise ValueError("matrix inventory mismatch")
        if evidence.get("scenario_frontier_matrix_rows") != 80:
            raise ValueError("cell matrix count mismatch")
        if evidence.get("scenario_frontier_matrix_sha256") != CELL_MATRIX_SHA256:
            raise ValueError("cell matrix binding mismatch")
        if sha256(archive.read("docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v1.csv")) != CELL_MATRIX_SHA256:
            raise ValueError("cell matrix content mismatch")
        if evidence.get("frontier_count") != 20:
            raise ValueError("frontier inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r1-design-scope",
            b"PASS stage8b-p1d4-r1-design-negative-harness 60/60",
            b"PASS stage8b-p1d4-design-matrix rows=72",
            b"PASS stage8b-p1d4-r1-cell-matrix cells=80",
            b"PASS stage8b-p1d4-r1-design-gate",
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
            "stage": "Stage 8B-P1-d4 R1 design",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-r1-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
