#!/usr/bin/env python3
"""Git-backed delta and archive preseal for Stage 5G-e-d-b."""

from __future__ import annotations

import io
import subprocess
import tarfile
from pathlib import PurePosixPath

import stage5g_edb_check as checker


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-edb-preseal: FAIL: {message}")


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, text=True).strip()


def safe_path(name: str) -> bool:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or name.startswith("/"):
        return False
    if any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts):
        return False
    return path.name != ".env" and path.suffix != ".log"


def exact_delta() -> list[tuple[str, str]]:
    output = run_text(["git", "diff", "--name-status", f"{checker.BASE_REF}..HEAD"])
    rows: list[tuple[str, str]] = []
    for line in output.splitlines():
        fields = line.split("\t")
        if len(fields) != 2 or fields[0] not in {"A", "M"}:
            fail(f"unsupported changed-path row: {line}")
        rows.append((fields[0], fields[1]))
    return rows


def main() -> None:
    parent = run_text(["git", "rev-parse", "HEAD^"])
    if parent != checker.BASE_REF:
        fail(f"HEAD parent must be {checker.BASE_REF}; got {parent}")
    delta = exact_delta()
    if delta != checker.EXPECTED_DELTA:
        fail(f"exact e-d-b changed-path allowlist drifted: {delta}")

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
            if not member.isfile() or not safe_path(member.name) or member.name in archived:
                fail(f"invalid archive member: {member.name}")
            archived.add(member.name)
    if archived != tracked:
        fail("archive/index mismatch")
    print(
        f"stage5g-edb-preseal: PASS delta={len(delta)}/{len(checker.EXPECTED_DELTA)} "
        f"archive={len(tracked)}/{len(archived)}"
    )


if __name__ == "__main__":
    main()
