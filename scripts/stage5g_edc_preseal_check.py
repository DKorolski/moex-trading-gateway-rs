#!/usr/bin/env python3
"""Git/archive preseal for the one-commit Stage 5G-e-d-c handoff."""

from __future__ import annotations

import io
import subprocess
import tarfile
from pathlib import PurePosixPath

import stage5g_edc_check as checker


EXPECTED_PATHS = sorted([
    "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_clean_restart.rs",
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth.rs",
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/application.rs",
    "crates/strategy-runtime-core/src/stage5g_fresh_broker_truth/reducer.rs",
    "crates/strategy-runtime-core/src/stage5g_order_position.rs",
    "docs/current-status.md",
    "docs/reviewer-onboarding-and-roadmap.md",
    "docs/stage-5/stage5g-e-d-c-application-contract.json",
    "docs/stage-5/stage5g-e-d-c-application-contract.md",
    "scripts/make_stage5g_edc_handoff_archive.py",
    "scripts/stage5g_edc_check.py",
    "scripts/stage5g_edc_gate.sh",
    "scripts/stage5g_edc_negative_harness.py",
    "scripts/stage5g_edc_preseal_check.py",
])


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-edc-preseal: FAIL: {message}")


def output(command: list[str]) -> str:
    return subprocess.check_output(command, text=True).strip()


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
    if output(["git", "rev-parse", "HEAD^"]) != checker.BASE_REF:
        fail("HEAD is not one direct successor to accepted R5")
    if output(["git", "branch", "--show-current"]) != "stage5g-lifecycle":
        fail("wrong branch")
    delta = output(["git", "diff", "--name-only", f"{checker.BASE_REF}..HEAD"]).splitlines()
    if sorted(delta) != EXPECTED_PATHS:
        fail(f"changed-path allowlist drift: {sorted(delta)}")
    if output(["git", "status", "--porcelain"]):
        fail("worktree must be clean")

    tracked: set[str] = set()
    for row in output(["git", "ls-files", "-s"]).splitlines():
        metadata, name = row.split("\t", 1)
        mode = metadata.split(" ", 1)[0]
        if mode not in {"100644", "100755"} or not safe(name) or name in tracked:
            fail(f"unsafe tracked member: {mode} {name}")
        tracked.add(name)
    archive_bytes = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"])
    archived: set[str] = set()
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name) or member.name in archived:
                fail(f"unsafe archive member: {member.name}")
            archived.add(member.name)
    if tracked != archived:
        fail("git index/archive mismatch")
    print(f"stage5g-edc-preseal: PASS delta={len(delta)} archive={len(archived)}")


if __name__ == "__main__":
    main()
