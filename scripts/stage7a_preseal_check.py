#!/usr/bin/env python3
"""Direct-successor source/archive seal for Stage 7A-R2b."""
from __future__ import annotations

import argparse
import io
import subprocess
import tarfile
from pathlib import Path, PurePosixPath

import stage7a_check as checker
import stage7a_closed_surface_check as closed


def output(*args: str, root: Path) -> str:
    return subprocess.check_output(args, cwd=root, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7a-preseal: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--require-origin", action="store_true")
    args = parser.parse_args()
    root = Path.cwd().resolve()
    if output("git", "rev-parse", "HEAD^", root=root) != checker.R2B_PREDECESSOR:
        fail("HEAD is not the direct successor of rejected Stage 7A-R2a candidate")
    if output("git", "branch", "--show-current", root=root) != checker.BRANCH:
        fail("wrong branch")
    if output("git", "status", "--porcelain", root=root):
        fail("worktree not clean")
    changed = set(output("git", "diff", "--name-only", f"{checker.BASE}..HEAD", root=root).splitlines())
    if changed != closed.EXACT:
        fail(f"changed-path drift missing={sorted(closed.EXACT-changed)} extra={sorted(changed-closed.EXACT)}")
    if args.require_origin:
        head = output("git", "rev-parse", "HEAD", root=root)
        if output("git", "rev-parse", f"origin/{checker.BRANCH}", root=root) != head:
            fail("origin branch mismatch")
    tracked = {line.split("\t", 1)[1] for line in output("git", "ls-files", "-s", root=root).splitlines()}
    archive = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=root)
    archived: set[str] = set()
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name) or member.name in archived:
                fail(f"unsafe archive member: {member.name}")
            archived.add(member.name)
    if tracked != archived:
        fail("index/archive mismatch")
    checker.check(root)
    print(f"stage7a-preseal: PASS delta={len(changed)} archive={len(archived)}")


if __name__ == "__main__":
    main()
