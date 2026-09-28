#!/usr/bin/env python3
"""Validate immutable Stage 8B-P1-e R2 corrected-design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1e-r2-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1e-r2-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
R1_REVIEW = "handoff-evidence/P1e_R1_design_engineering_review_693fab3.md"
R1_REVIEW_SHA256 = "eb451af76c9b4030705d900ae0b96fabf62452dde47810e4b007cd4410d58cca"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST, R1_REVIEW}
REQUIRED = GENERATED | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r1.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-design-r2.md",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r1-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-deployable-supervisor-r2-design-evidence.json",
    "docs/stage-8/stage8b-p1e-first-boot-transaction-v1.json",
    "docs/stage-8/stage8b-p1e-redis-runtime-policy-v1.json",
    "docs/stage-8/stage8b-p1e-restart-continuation-matrix-v2.csv",
    "docs/stage-8/stage8b-p1e-operational-continuation-matrix-v2.csv",
    "docs/stage-8/stage8b-p1e-supervisor-event-matrix-v1.csv",
    "deploy/paper-shadow/moex-finam-paper-runtime.service",
    "deploy/paper-shadow/moex-finam-paper-ws.service",
    "scripts/stage8b_p1e_r2_design_check.py",
    "scripts/stage8b_p1e_r2_design_negative_harness.py",
    "scripts/stage8b_p1e_r2_design_gate.sh",
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
        if evidence.get("status") != "R2_DESIGN_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("inherited_r1_acceptance_rows") != 88 or evidence.get("r2_acceptance_rows") != 48:
            raise ValueError("acceptance inventory mismatch")
        if evidence.get("restart_outer_rows") != 22 or evidence.get("operational_continuation_rows") != 54:
            raise ValueError("matrix inventory mismatch")
        if evidence.get("r2_negative_cases") != 40:
            raise ValueError("negative inventory mismatch")
        if evidence.get("design_only") is not True or evidence.get("implementation_authorized") is not False:
            raise ValueError("design boundary opened")
        if evidence.get("operational_activation_authorized") is not False:
            raise ValueError("activation opened")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        if sha256(archive.read(R1_REVIEW)) != R1_REVIEW_SHA256:
            raise ValueError("R1 review digest mismatch")
        if evidence.get("bundled_r1_review_sha256") != R1_REVIEW_SHA256:
            raise ValueError("R1 review evidence mismatch")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1e-r2-design-scope",
            b"PASS stage8b-p1e-r2-design-negative-harness 40/40 redigested=true",
            b"PASS stage8b-p1e-r2-matrices inherited_acceptance=88 r2_acceptance=48 restart=22 operational=54 events=24",
            b"PASS stage8b-p1e-r2-p0-units-unchanged units=2",
            b"PASS stage8b-p1e-r2-design-gate",
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
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": source_ref,
            "stage": "Stage 8B-P1-e R2 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_r2_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-r2-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-r2-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
