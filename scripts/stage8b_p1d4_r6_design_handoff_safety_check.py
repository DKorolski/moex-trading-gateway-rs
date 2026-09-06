#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 R6 design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-r6-design-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-r6-design-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
R5_REF = "b377c0275f1ce5f01cfe9b223724bf1542f985e2"
HASHES = {
    "docs/stage-8/stage8b-p1d4-crash-replay-acceptance-matrix.csv": "e2a376fda504a691b3dffa5160542c5f184bf59ff9f224ae4159b5bbaf8c06de",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv": "f4125113d4f7f3ceafd849c1d32f1097c313e15eccbb353c8bd89a19434edac6",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v2.csv": "e6a284372cc99fab3bfeadd9afa0d8e1ddec0070966f79505a5cb09dbb627561",
    "docs/stage-8/stage8b-p1d4-r6-acceptance-amendment.csv": "ca381b8e6891840303d9c602ca27e379a072a3cf344526f14ee1ce170e5f7f53",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r6.md": "99d5696ac3346f5c12bd4c5189a5c318ac2aaeadd329d392ae440014a3e1d7e2",
    "docs/stage-8/stage8b-p1d4-source-discovery-r6.md": "22e70bb86a5e4b228ed4f731b1aea2d0f196fcb611abeed2fe7d03f0edb7f77e",
    "docs/stage-8/stage8b-p1d4-source-shape-r6.json": "08890b48633918b52623c28222d9e02445e1820e7b4343f9b208bdffd6df1917",
}
SOURCE_HASHES = {
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs": "17edbe4c41315aef97faa8574eedd884e75ad90cb70e9aa01812bc6e197a7295",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs": "ef113596c8f9835669987853ea1ce4bdb976c73d507cf8014dabcff23a923ef8",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs": "619a6f7e4aff4d2d6ba2e0c86036da7df96383e3de020afbb6eaa3f907cb8478",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs": "2e9630533aa0470ad73294fb0afff6e155bf0a738eba2dae51eca9d0f7387a2b",
    "crates/strategy-runtime-core/src/stage8b_p1d2_market_feedback.rs": "376b0af099cdb95a31c48a4ab1b23272aa34dc436b19dd61898ddc078bde6480",
}
REQUIRED = GENERATED | set(HASHES) | set(SOURCE_HASHES) | {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r5.md",
    "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r6.json",
    "scripts/make_stage8b_p1d4_r6_design_handoff.py",
    "scripts/stage8b_p1d4_r6_design_check.py",
    "scripts/stage8b_p1d4_r6_design_gate.sh",
    "scripts/stage8b_p1d4_r6_design_handoff_safety_check.py",
    "scripts/stage8b_p1d4_r6_design_negative_harness.py",
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
        if marker.get("parent_ref") != R5_REF or evidence.get("parent_ref") != R5_REF:
            raise ValueError("R5 parent mismatch")
        if marker.get("source_tree") != evidence.get("source_tree"):
            raise ValueError("source tree mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if marker.get("branch") != "stage8b-paper-shadow-resumption":
            raise ValueError("source branch mismatch")

        if evidence.get("stage") != "Stage 8B-P1-d4 crash/replay design R6 correction":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_R6_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("active_positive_cells") != 105:
            raise ValueError("active proof inventory mismatch")
        if evidence.get("targeted_negative_cases") != 48 or evidence.get("total_contract_negative_cases") != 176:
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
        if composition.get("generated_market_journal_version") != "V1":
            raise ValueError("Market journal shape drifted")
        if composition.get("partial_v1_suffix_lengths") != [1, 2, 3, 4]:
            raise ValueError("partial frontier inventory drifted")
        if composition.get("command_publication_binding") != "Stage8bP1d4CommandPublicationBindingV1":
            raise ValueError("publication binding missing")

        for name, expected in HASHES.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"content hash mismatch: {name}")
        for name, expected in SOURCE_HASHES.items():
            if sha256(archive.read(name)) != expected:
                raise ValueError(f"committed source-shape mismatch: {name}")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")
        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r6-design-scope",
            b"PASS stage8b-p1d4-r6-design-negative-harness 48/48",
            b"PASS stage8b-p1d4-r6-matrices",
            b"PASS stage8b-p1d4-r6-design-gate",
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
            "stage": "Stage 8B-P1-d4 R6 design correction",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_r6_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-r6-design-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-r6-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
