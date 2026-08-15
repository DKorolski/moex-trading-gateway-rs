#!/usr/bin/env python3
"""Verify a Stage 8A-1 immutable handoff archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import stat
import zipfile
from pathlib import Path, PurePosixPath

ARTIFACTS = {
    "contract-check.txt",
    "closed-surface.txt",
    "negative.txt",
    "proof-map.json",
    "python-compile.txt",
    "fmt.txt",
    "focused-test.txt",
    "focused-doctest.txt",
    "focused-clippy.txt",
    "workspace-test.txt",
    "workspace-doctest.txt",
    "workspace-clippy.txt",
    "diff-check.txt",
    "toolchain.txt",
}
FORBIDDEN_PARTS = {".git", "target", "tmp", "reports", "__MACOSX"}


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        bool(name)
        and not path.is_absolute()
        and ".." not in path.parts
        and not any(part in FORBIDDEN_PARTS for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def verify(archive_path: Path) -> dict:
    with zipfile.ZipFile(archive_path) as archive:
        infos = archive.infolist()
        names = [info.filename for info in infos]
        if len(names) != len(set(names)):
            raise SystemExit("stage8a1-handoff-safety: FAIL duplicate entries")
        for info in infos:
            if not safe(info.filename):
                raise SystemExit(f"stage8a1-handoff-safety: FAIL unsafe path: {info.filename}")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode) or (mode and not stat.S_ISREG(mode)):
                raise SystemExit(f"stage8a1-handoff-safety: FAIL special member: {info.filename}")
        required = {
            "handoff-commit.txt",
            "source-tree-manifest.json",
            "handoff-evidence/stage8a1-full-gate.txt",
            "handoff-evidence/stage8a1-evidence.json",
            "handoff-evidence/stage8a1-preseal-safety.json",
            *(f"handoff-evidence/gate-artifacts/{name}" for name in ARTIFACTS),
        }
        missing = sorted(required - set(names))
        if missing:
            raise SystemExit(f"stage8a1-handoff-safety: FAIL missing: {missing}")
        marker = archive.read("handoff-commit.txt").decode()
        source_ref = next(
            (line.split("=", 1)[1] for line in marker.splitlines() if line.startswith("source_ref=")),
            "",
        )
        if len(source_ref) != 40 or "candidate_stage=Stage 8A-1" not in marker:
            raise SystemExit("stage8a1-handoff-safety: FAIL source/stage marker")
        manifest = json.loads(archive.read("source-tree-manifest.json"))
        if manifest["source_ref"] != source_ref:
            raise SystemExit("stage8a1-handoff-safety: FAIL source binding")
        for member in manifest["members"]:
            if hashlib.sha256(archive.read(member["path"])).hexdigest() != member["sha256"]:
                raise SystemExit(f"stage8a1-handoff-safety: FAIL source hash: {member['path']}")
        evidence = json.loads(archive.read("handoff-evidence/stage8a1-evidence.json"))
        if evidence["source_ref"] != source_ref:
            raise SystemExit("stage8a1-handoff-safety: FAIL evidence source")
        if evidence["candidate_status"] != "independent_acceptance_pending":
            raise SystemExit("stage8a1-handoff-safety: FAIL candidate status")
        hashes = evidence.get("gate_artifact_sha256")
        if set(hashes or {}) != ARTIFACTS:
            raise SystemExit("stage8a1-handoff-safety: FAIL artifact inventory")
        for name, digest in hashes.items():
            payload = archive.read(f"handoff-evidence/gate-artifacts/{name}")
            if hashlib.sha256(payload).hexdigest() != digest:
                raise SystemExit(f"stage8a1-handoff-safety: FAIL artifact hash: {name}")
        if evidence.get("all_required_gates_passed") is not True:
            raise SystemExit("stage8a1-handoff-safety: FAIL gate evidence")
        return {
            "schema_version": 1,
            "result": "PASS",
            "archive_name": archive_path.name,
            "archive_sha256": hashlib.sha256(archive_path.read_bytes()).hexdigest(),
            "source_ref": source_ref,
            "member_count": len(names),
            "unique_member_count": len(set(names)),
            "duplicate_entries": 0,
            "unsafe_paths": 0,
            "symlinks": 0,
            "special_files": 0,
        }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = verify(args.archive)
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(
            f"stage8a1-handoff-safety: PASS members={result['member_count']} "
            f"source_ref={result['source_ref']}"
        )


if __name__ == "__main__":
    main()
