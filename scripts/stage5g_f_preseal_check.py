#!/usr/bin/env python3
"""One-commit source and archive seal for Stage 5G-f."""

from __future__ import annotations

import io
import subprocess
import tarfile
from pathlib import PurePosixPath

import stage5g_f_check as checker

EXPECTED = sorted([
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_protective_completion.rs",
    "docs/stage-5/stage5g-f-protective-completion-contract.json",
    "docs/stage-5/stage5g-f-protective-completion-contract.md",
    "scripts/make_stage5g_f_handoff_archive.py",
    "scripts/stage5g_f_check.py",
    "scripts/stage5g_f_negative_harness.py",
    "scripts/stage5g_f_preseal_check.py",
    "scripts/stage5g_f_r6_gate.sh",
])


def output(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-f-preseal: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(p in {".git", "target", "tmp", "reports", "__MACOSX"} for p in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def main() -> None:
    if output("git", "rev-parse", "HEAD^") != checker.BASE:
        fail(f"HEAD is not the direct successor to {checker.BASE}")
    if output("git", "branch", "--show-current") != checker.BRANCH:
        fail("wrong branch")
    delta = output("git", "diff", "--name-only", f"{checker.BASE}..HEAD").splitlines()
    if sorted(delta) != EXPECTED:
        fail(f"changed-path allowlist drift: {sorted(delta)}")
    if output("git", "status", "--porcelain"):
        fail("worktree must be clean")
    tracked = {line.split("\t", 1)[1] for line in output("git", "ls-files", "-s").splitlines()}
    data = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"])
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
    print(f"stage5g-f-preseal: PASS delta={len(delta)} archive={len(archived)}")


if __name__ == "__main__":
    main()
