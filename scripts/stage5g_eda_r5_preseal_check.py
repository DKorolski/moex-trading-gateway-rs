#!/usr/bin/env python3
"""Fail-closed tracked-tree/archive safety preseal for the R5 handoff."""

from __future__ import annotations

import io
import subprocess
import tarfile
from pathlib import PurePosixPath


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-eda-r5-preseal: FAIL: {message}")


def safe_path(name: str) -> bool:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or name.startswith("/"):
        return False
    if any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts):
        return False
    return path.name != ".env" and path.suffix != ".log"


def main() -> None:
    index = subprocess.check_output(["git", "ls-files", "-s"], text=True)
    tracked: set[str] = set()
    for line in index.splitlines():
        metadata, path = line.split("\t", 1)
        mode = metadata.split(" ", 1)[0]
        if mode not in {"100644", "100755"}:
            fail(f"non-regular tracked mode {mode}: {path}")
        if not safe_path(path):
            fail(f"unsafe tracked path: {path}")
        if path in tracked:
            fail(f"duplicate tracked path: {path}")
        tracked.add(path)

    archive_bytes = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"])
    archived: set[str] = set()
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile():
                fail(f"non-regular archive member: {member.name}")
            if not safe_path(member.name):
                fail(f"unsafe archive member: {member.name}")
            if member.name in archived:
                fail(f"duplicate archive member: {member.name}")
            archived.add(member.name)
    if archived != tracked:
        missing = sorted(tracked - archived)
        extra = sorted(archived - tracked)
        fail(f"archive/index mismatch missing={missing[:3]} extra={extra[:3]}")
    print(f"stage5g-eda-r5-preseal: PASS ({len(tracked)}/{len(archived)})")


if __name__ == "__main__":
    main()
