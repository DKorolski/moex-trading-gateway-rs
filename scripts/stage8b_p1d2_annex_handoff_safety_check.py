#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-d2 design-annex handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath


EVIDENCE = "handoff-evidence/stage8b-p1d2-annex-evidence.json"
GATE = "handoff-evidence/stage8b-p1d2-annex-gate.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
GENERATED = {"handoff-commit.txt", EVIDENCE, GATE, MANIFEST}
PREDECESSOR = "4abb2fd9807adeb47f164a4025c7ac44d33679f6"
REQUIRED = GENERATED | {
    "docs/stage-8/stage8b-p1d0-deterministic-paper-execution-policy.md",
    "docs/stage-8/stage8b-p1d1-market-provider-core.md",
    "docs/stage-8/stage8b-p1d2-projection-field-timestamp-annex.md",
    "docs/stage-8/stage8b-p1d2-projection-annex-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1d2-projection-annex-evidence.json",
    "docs/current-status.md",
    "docs/roadmap.md",
    "crates/broker-core/src/operational_snapshot.rs",
    "crates/broker-core/src/command.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d1_paper_provider.rs",
    "scripts/stage8b_p1d2_annex_check.py",
    "scripts/stage8b_p1d2_annex_negative_harness.py",
    "scripts/stage8b_p1d2_annex_gate.sh",
    "scripts/make_stage8b_p1d2_annex_handoff.py",
    "scripts/stage8b_p1d2_annex_handoff_safety_check.py",
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
            if member.parts and member.parts[0] in {
                ".git",
                "target",
                "tmp",
                "reports",
                "__MACOSX",
            }:
                raise ValueError(f"forbidden root: {info.filename}")
            if any(part == ".env" for part in member.parts):
                raise ValueError(f"secret path: {info.filename}")
            if info.filename.endswith(
                (".log", ".sqlite", ".sqlite3", ".pem", ".key", ".ed25519")
            ):
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
        if marker.get("archive_name") != PurePosixPath(path).name:
            raise ValueError("archive name mismatch")
        if evidence.get("stage") != "Stage 8B-P1-d2 projection annex":
            raise ValueError("stage mismatch")
        if evidence.get("status") != "DESIGN_ONLY_REVIEW_CANDIDATE":
            raise ValueError("candidate status mismatch")
        if evidence.get("accepted_p1d1_closure_ref") != PREDECESSOR:
            raise ValueError("predecessor mismatch")
        if evidence.get("acceptance_rows") != 60:
            raise ValueError("acceptance count mismatch")
        if evidence.get("negative_cases") != 30:
            raise ValueError("negative count mismatch")
        if evidence.get("design_only") is not True:
            raise ValueError("design-only marker missing")
        if evidence.get("implementation_authorized") is not False:
            raise ValueError("source opened early")
        if any(value is not False for value in evidence.get("closed_surfaces", {}).values()):
            raise ValueError("closed surface opened")
        verification = evidence.get("verification", {})
        if not verification or any(value != "PASS" for value in verification.values()):
            raise ValueError("verification incomplete")

        gate = archive.read(GATE)
        for expected in (
            b"PASS stage8b-p1d2-annex-scope",
            b"PASS stage8b-p1d2-annex-negative-harness 30/30",
            b"PASS stage8b-p1d2-annex-gate",
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
            "stage": "Stage 8B-P1-d2 projection annex",
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1d2_annex_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1d2-annex-handoff-safety: FAIL {error}", file=sys.stderr)
        raise SystemExit(1)
    print("stage8b-p1d2-annex-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
