#!/usr/bin/env python3
"""Verify and freshly extract the exact independently reviewed handoff."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import zipfile
from pathlib import Path

import stage8b_p_r2b_generation2_full_transaction_native_r0_handoff_safety_check as safety


HEX64 = re.compile(r"[0-9a-f]{64}")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def marker(path: Path) -> dict[str, str]:
    return {
        key: value
        for line in path.read_text(encoding="utf-8").splitlines()
        if "=" in line
        for key, value in [line.split("=", 1)]
    }


def verify_and_extract(
    archive_path: Path,
    expected_archive_sha256: str,
    reviewer_acceptance_sha256: str,
    extraction_root: Path,
) -> dict[str, object]:
    require(HEX64.fullmatch(expected_archive_sha256) is not None, "accepted archive digest grammar drift")
    require(HEX64.fullmatch(reviewer_acceptance_sha256) is not None, "review acceptance digest grammar drift")
    metadata = os.lstat(archive_path)
    require(stat.S_ISREG(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode), "review archive custody drift")
    actual_archive_sha256 = digest(archive_path)
    require(actual_archive_sha256 == expected_archive_sha256, "review archive SHA-256 mismatch")
    require(extraction_root.is_dir() and not any(extraction_root.iterdir()), "fresh extraction root must be empty")

    package = safety.check(str(archive_path))
    with zipfile.ZipFile(archive_path) as archive:
        archive.extractall(extraction_root)
        for info in archive.infolist():
            target = extraction_root / info.filename
            mode = (info.external_attr >> 16) & 0o777
            if target.exists() and not target.is_dir():
                os.chmod(target, mode)

    binding = marker(extraction_root / "handoff-commit.txt")
    manifest = json.loads((extraction_root / safety.MANIFEST).read_text(encoding="utf-8"))
    require(binding.get("source_ref") == manifest.get("source_ref"), "extracted source-ref binding drift")
    require(binding.get("archive_name") == archive_path.name, "extracted archive-name binding drift")
    require(binding.get("source_tree") is not None, "extracted source-tree binding missing")

    tracked: set[str] = set()
    for entry in manifest.get("entries", []):
        relative = entry["path"]
        require(relative not in tracked, "duplicate manifest path")
        tracked.add(relative)
        target = extraction_root / relative
        target_metadata = os.lstat(target)
        require(stat.S_ISREG(target_metadata.st_mode), f"extracted non-file source member: {relative}")
        require(target.stat().st_size == entry["size"], f"extracted source size drift: {relative}")
        require(digest(target) == entry["sha256"], f"extracted source digest drift: {relative}")
        require(f"{stat.S_IFREG | stat.S_IMODE(target_metadata.st_mode):06o}" == entry["mode"], f"extracted source mode drift: {relative}")

    return {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B Generation-2 reviewed archive extraction",
        "result": "PASS",
        "source_ref": binding["source_ref"],
        "source_tree": binding["source_tree"],
        "archive_name": archive_path.name,
        "archive_sha256": actual_archive_sha256,
        "reviewer_acceptance_sha256": reviewer_acceptance_sha256,
        "fresh_extraction": True,
        "source_manifest_verified": True,
        "additional_members_rejected": True,
        "tracked_members_verified": len(tracked),
        "archive_members_verified": package["archive_members"],
        "private_material_members": 0,
        "native_execution": False,
        "authorization": "NOT_ISSUED",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--reviewer-acceptance-sha256", required=True)
    parser.add_argument("--extraction-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.output.exists() or not arguments.output.parent.is_dir():
        raise SystemExit("stage8b-generation2-review-archive: FAIL unsafe output")
    try:
        result = verify_and_extract(
            arguments.archive.resolve(strict=True),
            arguments.expected_sha256,
            arguments.reviewer_acceptance_sha256,
            arguments.extraction_root.resolve(strict=True),
        )
    except (KeyError, OSError, RuntimeError, ValueError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-review-archive: FAIL {error}") from None
    arguments.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print("stage8b-generation2-review-archive: PASS fresh=true manifest=true authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
