#!/usr/bin/env python3
"""Verify a Stage 8A-0 immutable handoff archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import stat
import zipfile
from pathlib import Path, PurePosixPath

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
            raise SystemExit("stage8a0-handoff-safety: FAIL duplicate entries")
        for info in infos:
            if not safe(info.filename):
                raise SystemExit(f"stage8a0-handoff-safety: FAIL unsafe path: {info.filename}")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode) or (mode and not stat.S_ISREG(mode)):
                raise SystemExit(f"stage8a0-handoff-safety: FAIL special member: {info.filename}")
        required = {
            "handoff-commit.txt",
            "source-tree-manifest.json",
            "handoff-evidence/stage8a0-full-gate.txt",
            "handoff-evidence/stage8a0-proof-map.json",
            "handoff-evidence/stage8a0-evidence.json",
            "handoff-evidence/stage8a0-preseal-safety.json",
        }
        required.update({
            f"handoff-evidence/gate-artifacts/{name}"
            for name in (
                "contract-check.txt", "closed-surface.txt", "negative.txt",
                "proof-map.json", "python-compile.txt", "fmt.txt", "test.txt",
                "doctest.txt", "clippy.txt", "diff-check.txt", "toolchain.txt",
                "timing-flake-evidence.json",
            )
        })
        missing = sorted(required - set(names))
        if missing:
            raise SystemExit(f"stage8a0-handoff-safety: FAIL missing: {missing}")
        marker = archive.read("handoff-commit.txt").decode()
        source_ref = next((line.split("=", 1)[1] for line in marker.splitlines() if line.startswith("source_ref=")), "")
        if len(source_ref) != 40:
            raise SystemExit("stage8a0-handoff-safety: FAIL source_ref")
        manifest = json.loads(archive.read("source-tree-manifest.json"))
        if manifest["source_ref"] != source_ref:
            raise SystemExit("stage8a0-handoff-safety: FAIL source binding")
        for member in manifest["members"]:
            payload = archive.read(member["path"])
            if hashlib.sha256(payload).hexdigest() != member["sha256"]:
                raise SystemExit(f"stage8a0-handoff-safety: FAIL source hash: {member['path']}")
        evidence = json.loads(archive.read("handoff-evidence/stage8a0-evidence.json"))
        if evidence["source_ref"] != source_ref or evidence["candidate_status"] != "independent_acceptance_pending":
            raise SystemExit("stage8a0-handoff-safety: FAIL evidence binding/status")
        artifact_hashes = evidence.get("gate_artifact_sha256")
        if set(artifact_hashes or {}) != {
            "contract-check.txt", "closed-surface.txt", "negative.txt",
            "proof-map.json", "python-compile.txt", "fmt.txt", "test.txt",
            "doctest.txt", "clippy.txt", "diff-check.txt", "toolchain.txt",
            "timing-flake-evidence.json",
        }:
            raise SystemExit("stage8a0-handoff-safety: FAIL artifact hash inventory")
        for name, digest in artifact_hashes.items():
            payload = archive.read(f"handoff-evidence/gate-artifacts/{name}")
            if hashlib.sha256(payload).hexdigest() != digest:
                raise SystemExit(f"stage8a0-handoff-safety: FAIL artifact hash: {name}")
        if evidence.get("final_serialized_gate_passed") is not True:
            raise SystemExit("stage8a0-handoff-safety: FAIL serialized gate evidence")
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
        print(f"stage8a0-handoff-safety: PASS members={result['member_count']} source_ref={result['source_ref']}")


if __name__ == "__main__":
    main()
