#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d4 source handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d4-source-evidence.json"
GATE = "handoff-evidence/stage8b-p1d4-source-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
ACCEPTED_DESIGN = "1a1ea05775f1d15b86fcc3495ad6863b851e9212"
ACCEPTED_P1D3 = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
BRANCH = "stage8b-paper-shadow-resumption"
REQUIRED = GENERATED | {
    "crates/strategy-runtime-core/src/stage8b_p1d4_generated_market.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "docs/stage-8/stage8b-p1d4-crash-replay-design-r7.md",
    "docs/stage-8/stage8b-p1d4-scenario-frontier-matrix-v5.csv",
    "docs/stage-8/stage8b-p1d4-generated-market-crash-submatrix-v3.csv",
    "docs/stage-8/stage8b-p1d4-generated-market-source.md",
    "docs/stage-8/stage8b-p1d4-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d4-source-evidence.json",
    "scripts/stage8b_p1d4_source_check.py",
    "scripts/stage8b_p1d4_source_negative_harness.py",
    "scripts/stage8b_p1d4_source_gate.sh",
    "scripts/make_stage8b_p1d4_source_handoff.py",
    "scripts/stage8b_p1d4_source_handoff_safety_check.py",
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
        source_parent = marker.get("source_parent")
        source_tree = marker.get("source_tree")
        if not source_ref or evidence.get("source_ref") != source_ref:
            raise ValueError("source binding mismatch")
        if source_parent != ACCEPTED_DESIGN or evidence.get("source_parent") != source_parent:
            raise ValueError("source parent mismatch")
        if evidence.get("accepted_design_ref") != ACCEPTED_DESIGN:
            raise ValueError("accepted design mismatch")
        if evidence.get("accepted_business_source_ref") != ACCEPTED_P1D3:
            raise ValueError("accepted P1-d3 mismatch")
        if evidence.get("review_target") != source_ref:
            raise ValueError("review target mismatch")
        if not source_tree or evidence.get("source_tree") != source_tree:
            raise ValueError("source tree mismatch")
        if marker.get("branch") != BRANCH or evidence.get("branch") != BRANCH:
            raise ValueError("branch mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("worktree_clean") is not True or evidence.get("pushed_to_origin") is not False:
            raise ValueError("worktree/push status mismatch")
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if evidence.get("stage") != "Stage 8B-P1-d4 generated-Market crash/replay source":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("acceptance_rows") != 20 or evidence.get("negative_cases") != 43:
            raise ValueError("acceptance inventory mismatch")
        if evidence.get("unexpected_protected_path_changes") != []:
            raise ValueError("protected path changed")
        if evidence.get("next_stage_authorized") is not False:
            raise ValueError("P1-e opened early")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        proof = evidence.get("crash_proof", {})
        if proof.get("positive_sigkill_cells") != 105 or proof.get("base_cells") != 92 or proof.get("generated_market_cells") != 13:
            raise ValueError("crash inventory mismatch")
        verification = evidence.get("verification", {})
        if not verification or any(
            not isinstance(value, str) or not value.startswith("PASS")
            for value in verification.values()
        ):
            raise ValueError("verification is incomplete")
        changed_paths = evidence.get("changed_paths")
        if not isinstance(changed_paths, list) or not changed_paths:
            raise ValueError("changed path inventory missing")
        if sha256(("\n".join(changed_paths) + "\n").encode()) != evidence.get("changed_paths_sha256"):
            raise ValueError("changed path digest mismatch")
        if evidence.get("unexpected_protected_path_changes"):
            raise ValueError("protected path changed")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d4-r7-design-baseline-integrity",
            b"PASS stage8b-p1d4-source-check",
            b"PASS stage8b-p1d4-source-negative-harness 43/43",
            b"PASS stage8b-p1d4-source-gate",
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
            "source_parent": source_parent,
            "source_tree": source_tree,
            "branch": BRANCH,
            "stage": "Stage 8B-P1-d4 generated-Market crash/replay source",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d4_source_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d4-source-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d4-source-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
