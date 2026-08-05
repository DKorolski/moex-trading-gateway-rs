#!/usr/bin/env python3
"""Git-backed delta and archive preseal for Stage 5G-e-d-a R6."""

from __future__ import annotations

import io
import subprocess
import tarfile
from pathlib import PurePosixPath


BASE_REF = "c84ee07c2700f04b5c070eab713598777d5195b6"
EXPECTED_DELTA = [
    ("M", "docs/current-status.md"),
    ("M", "docs/reviewer-onboarding-and-roadmap.md"),
    ("A", "docs/stage-5/stage5g-e-d-a-r6-protected-tree-freeze.json"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.json"),
    ("M", "docs/stage-5/stage5g-e-d-fresh-broker-truth-reconciliation.md"),
    ("M", "scripts/make_stage5g_ed_handoff_archive.py"),
    ("A", "scripts/stage5g_eda_r6_check.py"),
    ("A", "scripts/stage5g_eda_r6_gate.sh"),
    ("A", "scripts/stage5g_eda_r6_negative_harness.py"),
    ("A", "scripts/stage5g_eda_r6_preseal_check.py"),
]


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-eda-r6-preseal: FAIL: {message}")


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
    output = run_text(["git", "diff", "--name-status", f"{BASE_REF}..HEAD"])
    rows: list[tuple[str, str]] = []
    for line in output.splitlines():
        fields = line.split("\t")
        if len(fields) != 2 or fields[0] not in {"A", "M"}:
            fail(f"unsupported changed-path row: {line}")
        rows.append((fields[0], fields[1]))
    return rows


def main() -> None:
    parent = run_text(["git", "rev-parse", "HEAD^"])
    if parent != BASE_REF:
        fail(f"HEAD parent must be {BASE_REF}; got {parent}")
    delta = exact_delta()
    if delta != EXPECTED_DELTA:
        fail(f"exact R6 changed-path allowlist drifted: {delta}")

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
        fail(
            f"archive/index mismatch missing={sorted(tracked - archived)[:3]} "
            f"extra={sorted(archived - tracked)[:3]}"
        )
    print(
        f"stage5g-eda-r6-preseal: PASS "
        f"delta={len(delta)}/{len(EXPECTED_DELTA)} archive={len(tracked)}/{len(archived)}"
    )


if __name__ == "__main__":
    main()
