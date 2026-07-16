#!/usr/bin/env python3
"""Fail-closed source/archive safety checks for review handoffs."""

from __future__ import annotations

import argparse
import json
import re
import stat
import zipfile
from pathlib import Path, PurePosixPath


EXCLUDED_PARTS = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
FORBIDDEN_NAME_PATTERNS = (
    re.compile(r"^\.env(?:\..*)?$"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
)
FORBIDDEN_CONTENT = re.compile(
    rb"(75" rb"02[A-Z0-9]*|190" rb"9892|63" rb"170[A-Z0-9/]*|"
    rb"tapi_[sa]k_[A-Za-z0-9_-]+|"
    rb"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)


def path_is_excluded(path: PurePosixPath) -> bool:
    return any(part in EXCLUDED_PARTS for part in path.parts) or any(
        pattern.fullmatch(path.name) for pattern in FORBIDDEN_NAME_PATTERNS
    ) or path.name == ".DS_Store"


def check_payload(name: str, payload: bytes) -> None:
    if b"\x00" in payload:
        return
    match = FORBIDDEN_CONTENT.search(payload)
    if match:
        raise SystemExit(f"handoff safety: forbidden live-like literal in {name}")


def check_source_tree(root: Path) -> None:
    for path in root.rglob("*"):
        relative = PurePosixPath(path.relative_to(root).as_posix())
        if path_is_excluded(relative):
            continue
        if path.is_symlink():
            raise SystemExit(f"handoff safety: included symlink in source tree: {relative}")
        if path.is_file():
            check_payload(str(relative), path.read_bytes())
    print("handoff-source-safety: ok")


def check_archive(path: Path) -> None:
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise SystemExit("handoff safety: duplicate ZIP entries")
        for info in archive.infolist():
            pure = PurePosixPath(info.filename)
            if pure.is_absolute() or ".." in pure.parts:
                raise SystemExit(f"handoff safety: unsafe ZIP path: {info.filename}")
            if path_is_excluded(pure):
                raise SystemExit(f"handoff safety: excluded artifact in ZIP: {info.filename}")
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise SystemExit(f"handoff safety: symlink in ZIP: {info.filename}")
            if not info.is_dir():
                check_payload(info.filename, archive.read(info))

        required = {"handoff-commit.txt", "handoff-manifest.json"}
        missing = sorted(required - set(names))
        if missing:
            raise SystemExit(f"handoff safety: missing generated markers: {missing}")
        manifest = json.loads(archive.read("handoff-manifest.json"))
        marker = archive.read("handoff-commit.txt").decode().splitlines()
        expected_marker = [
            f"source_commit={manifest['source_commit']}",
            f"source_ref={manifest['source_ref']}",
            f"archive_name={manifest['archive_name']}",
        ]
        if marker != expected_marker or manifest["archive_name"] != path.name:
            raise SystemExit("handoff safety: provenance marker/manifest mismatch")
    print("handoff-archive-safety: ok")


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--source-tree", type=Path)
    group.add_argument("--archive", type=Path)
    args = parser.parse_args()
    if args.source_tree:
        check_source_tree(args.source_tree.resolve())
    else:
        check_archive(args.archive.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
