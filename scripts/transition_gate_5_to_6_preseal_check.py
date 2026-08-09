#!/usr/bin/env python3
"""One-commit archive seal for Transition Gate 5->6."""

from __future__ import annotations

import argparse
import io
import subprocess
import tarfile
from pathlib import Path, PurePosixPath

import transition_gate_5_to_6_check as checker

EXPECTED = sorted([
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "docs/roadmap.md",
    "docs/stage-6/stage5g-h-acceptance-reference.json",
    "docs/stage-6/stage6-crash-window-matrix.md",
    "docs/stage-6/stage6-durable-identity-contract.md",
    "docs/stage-6/stage6-persistence-ownership.md",
    "docs/stage-6/stage6-slice-plan.md",
    "docs/stage-6/transition-5-to-6-authority-inventory.json",
    "docs/stage-6/transition-5-to-6-descriptor.json",
    "scripts/make_transition_gate_5_to_6_handoff_archive.py",
    "scripts/transition_gate_5_to_6.sh",
    "scripts/transition_gate_5_to_6_check.py",
    "scripts/transition_gate_5_to_6_closed_surface_check.py",
    "scripts/transition_gate_5_to_6_negative_harness.py",
    "scripts/transition_gate_5_to_6_preseal_check.py",
])


def output(*args: str, root: Path) -> str:
    return subprocess.check_output(args, cwd=root, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"transition-gate-5-to-6-preseal: FAIL: {message}")


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
        fail("HEAD is not the direct successor to accepted Stage 5 closure")
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
    checker.check(root)
    print(f"transition-gate-5-to-6-preseal: PASS delta={len(delta)} archive={len(archived)}")


if __name__ == "__main__":
    main()
