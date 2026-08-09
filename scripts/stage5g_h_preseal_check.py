#!/usr/bin/env python3
"""One-commit source/archive seal for Stage 5G-h."""

from __future__ import annotations

import argparse
import io
import subprocess
import tarfile
from pathlib import Path, PurePosixPath

import stage5g_h_check as checker

EXPECTED = sorted([
    "crates/strategy-runtime-core/src/bin/stage5g_g_lifecycle_artifact.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_lifecycle_freeze.rs",
    "docs/current-status.md",
    "docs/stage-5/accepted-stage5g-g-lifecycle-artifact.json",
    "docs/stage-5/accepted-stage5g-g-lifecycle-artifact.sha256",
    "docs/stage-5/stage5g-authority-inventory.json",
    "docs/stage-5/stage5g-closure-descriptor.json",
    "docs/stage-5/stage5g-g-lifecycle-matrix-freeze.md",
    "docs/stage-5/stage5g-h-aggregate-closure.md",
    "scripts/make_stage5g_h_handoff_archive.py",
    "scripts/stage5g_h_check.py",
    "scripts/stage5g_h_closed_surface_check.py",
    "scripts/stage5g_h_gate.sh",
    "scripts/stage5g_h_negative_harness.py",
    "scripts/stage5g_h_preseal_check.py",
])


def output(*args: str, root: Path) -> str:
    return subprocess.check_output(args, cwd=root, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-h-preseal: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and not any(
        part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts
    ) and path.name != ".env" and path.suffix != ".log"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--require-origin", action="store_true")
    args = parser.parse_args()
    root = args.root.resolve()
    if output("git", "rev-parse", "HEAD^", root=root) != checker.BASE:
        fail("HEAD is not the direct successor to accepted Stage 5G-g")
    if output("git", "branch", "--show-current", root=root) != checker.BRANCH:
        fail("wrong branch")
    delta = output("git", "diff", "--name-only", f"{checker.BASE}..HEAD", root=root).splitlines()
    if sorted(delta) != EXPECTED:
        fail(f"changed-path allowlist drift: {sorted(delta)}")
    if output("git", "status", "--porcelain", root=root):
        fail("worktree must be clean")
    if args.require_origin and output("git", "rev-parse", "origin/stage5g-lifecycle", root=root) != output("git", "rev-parse", "HEAD", root=root):
        fail("origin/stage5g-lifecycle must equal HEAD")
    tracked = {line.split("\t", 1)[1] for line in output("git", "ls-files", "-s", root=root).splitlines()}
    data = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=root)
    archived: set[str] = set()
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name) or member.name in archived:
                fail(f"unsafe archive member: {member.name}")
            archived.add(member.name)
    if tracked != archived:
        fail("index/archive mismatch")
    checker.check(root, None, None)
    print(f"stage5g-h-preseal: PASS delta={len(delta)} archive={len(archived)}")


if __name__ == "__main__":
    main()
