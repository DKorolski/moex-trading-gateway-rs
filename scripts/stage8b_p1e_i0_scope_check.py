#!/usr/bin/env python3
"""Fail-closed I0 delta checker relative to an independently accepted R9 ref."""

from __future__ import annotations

import pathlib
import re
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parents[1]
PRODUCTION_ALLOWLIST = {
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/lib.rs",
}
HELPER_PREFIXES = (
    "docs/stage-8/stage8b-p1e-i0-",
)
SHARED_FILES = {"docs/current-status.md", "docs/roadmap.md"}


class CheckFailure(RuntimeError):
    pass


def run(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CheckFailure(message)


def validate_ref(value: str) -> str:
    require(bool(re.fullmatch(r"[0-9a-f]{40}", value)), "accepted R9 ref must be a full lowercase commit SHA")
    resolved = run("git", "rev-parse", f"{value}^{{commit}}")
    require(resolved == value, "accepted R9 ref does not resolve exactly")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", value, "HEAD"], cwd=ROOT
    )
    require(ancestor.returncode == 0, "accepted R9 ref is not an ancestor of I0 HEAD")
    return value


def changed_files(base: str) -> set[str]:
    return {path for path in run("git", "diff", "--name-only", base, "HEAD", "--").splitlines() if path}


def allowed_helper(path: str) -> bool:
    return path in SHARED_FILES or path.startswith(HELPER_PREFIXES)


def validate(base: str) -> tuple[set[str], set[str]]:
    validate_ref(base)
    require(run("git", "rev-parse", "HEAD") != base, "I0 HEAD must be a new immutable commit")
    require(
        not run("git", "status", "--porcelain", "--untracked-files=all"),
        "acceptance source must be a clean immutable worktree including untracked files",
    )
    changed = changed_files(base)
    production = {path for path in changed if path.startswith("crates/")}
    require(production <= PRODUCTION_ALLOWLIST, f"I0 production allowlist escaped: {sorted(production - PRODUCTION_ALLOWLIST)}")
    require(not any(path in {"Cargo.toml", "Cargo.lock"} or path.endswith("/Cargo.toml") for path in changed), "Cargo change is closed")
    require(not any(path.startswith(".github/workflows/") for path in changed), "workflow change is closed")
    require(not any(path.startswith(("config/", "configs/", "deploy/", "deployment/", "systemd/")) for path in changed), "config/deployment change is closed")
    nonproduction = changed - production
    require(all(allowed_helper(path) for path in nonproduction), f"I0 helper scope escaped: {sorted(path for path in nonproduction if not allowed_helper(path))}")
    return changed, production


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i0_scope_check.py ACCEPTED_R9_COMMIT")
    try:
        changed, production = validate(sys.argv[1])
    except (CheckFailure, OSError, subprocess.CalledProcessError) as error:
        print(f"stage8b-p1e-i0-scope-check: FAIL {error}")
        raise SystemExit(1)
    print(
        "PASS stage8b-p1e-i0-scope-check "
        f"changed={len(changed)} production={len(production)} "
        "production_allowlist=3 clean=true immutable_head=true "
        "cargo=false workflow=false config=false deployment=false"
    )


if __name__ == "__main__":
    main()
