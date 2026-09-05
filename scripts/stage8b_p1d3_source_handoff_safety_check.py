#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d3 source handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d3-source-evidence.json"
GATE = "handoff-evidence/stage8b-p1d3-source-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
ACCEPTED_DESIGN = "df330b2424199739ceb7c261321a5e5ee381c332"
R1_CORRECTION_PARENT = "77f6887e98ab8f2be81ca195adac12ae4a7d82ed"
ACCEPTED_P1D2 = "bcd8db546104968dd0e48ab041e02acf6869d224"
BRANCH = "stage8b-paper-shadow-resumption"
REQUIRED = GENERATED | {
    "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "docs/stage-8/stage8b-p1d3-working-limit-cancel-source.md",
    "docs/stage-8/stage8b-p1d3-source-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d3-source-evidence.json",
    "fixtures/stage8b-p1d3/outcome-golden-v1.json",
    "fixtures/stage8b-p1d3/projection-golden-v1.json",
    "scripts/stage8b_p1d3_source_check.py",
    "scripts/stage8b_p1d3_source_negative_harness.py",
    "scripts/stage8b_p1d3_source_gate.sh",
    "scripts/make_stage8b_p1d3_source_handoff.py",
    "scripts/stage8b_p1d3_source_handoff_safety_check.py",
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
        source_parent = marker.get("source_parent")
        if source_parent != R1_CORRECTION_PARENT or evidence.get("source_parent") != source_parent:
            raise ValueError("source parent mismatch")
        if evidence.get("r1_correction_parent") != R1_CORRECTION_PARENT:
            raise ValueError("R1 correction parent mismatch")
        if evidence.get("accepted_design_parent") != ACCEPTED_DESIGN:
            raise ValueError("accepted design parent mismatch")
        if evidence.get("review_target") != source_ref:
            raise ValueError("review target mismatch")
        source_tree = marker.get("source_tree")
        if not source_tree or evidence.get("source_tree") != source_tree:
            raise ValueError("source tree mismatch")
        if marker.get("branch") != BRANCH or evidence.get("branch") != BRANCH:
            raise ValueError("branch mismatch")
        if evidence.get("worktree_clean") is not True:
            raise ValueError("worktree cleanliness missing")
        if evidence.get("pushed_to_origin") is not False:
            raise ValueError("unexpected push status")
        if manifest.get("source_ref") != source_ref:
            raise ValueError("manifest source binding mismatch")
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "Stage 8B-P1-d3 working LIMIT/CANCEL/expiry source":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "SOURCE_IMPLEMENTATION_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_design_ref") != ACCEPTED_DESIGN:
            raise ValueError("accepted design mismatch")
        if evidence.get("accepted_p1d2_closure_ref") != ACCEPTED_P1D2:
            raise ValueError("accepted P1-d2 mismatch")
        if evidence.get("acceptance_rows") != 56 or evidence.get("negative_cases") != 40:
            raise ValueError("acceptance inventory mismatch")
        if evidence.get("unexpected_protected_path_changes") != []:
            raise ValueError("protected path changed")
        changed_paths = evidence.get("changed_paths")
        if not isinstance(changed_paths, list) or not changed_paths:
            raise ValueError("changed path inventory missing")
        changed_paths_bytes = ("\n".join(changed_paths) + "\n").encode()
        if sha256(changed_paths_bytes) != evidence.get("changed_paths_sha256"):
            raise ValueError("changed path digest mismatch")
        cumulative_changed_paths = evidence.get("cumulative_changed_paths_from_design")
        if (
            not isinstance(cumulative_changed_paths, list)
            or "fixtures/stage8b-p1d3/projection-golden-v1.json" not in cumulative_changed_paths
        ):
            raise ValueError("cumulative design delta inventory missing")
        implementation = evidence.get("implementation", {})
        if (
            implementation.get("complete_projection_golden_shapes") != 8
            or implementation.get("cancel_dcid_tcid_inequality_enforced") is not True
        ):
            raise ValueError("R1 source hardening evidence missing")
        if evidence.get("next_stage_authorized") is not False:
            raise ValueError("next stage opened early")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        verification = evidence.get("verification", {})
        if not verification or any(
            not isinstance(value, str) or not value.startswith("PASS")
            for value in verification.values()
        ):
            raise ValueError("verification is incomplete")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d3-source-scope",
            b"PASS stage8b-p1d3-source-negative-harness 40/40",
            b"PASS stage8b-p1d3-source-gate",
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
            "source_parent": source_parent,
            "source_tree": source_tree,
            "branch": BRANCH,
            "stage": "Stage 8B-P1-d3 working LIMIT/CANCEL/expiry source",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d3_source_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d3-source-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d3-source-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
