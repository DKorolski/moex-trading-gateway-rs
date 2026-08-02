#!/usr/bin/env python3
"""Run the accepted R1 gates against the exact detached R2 predecessor."""

from __future__ import annotations

import argparse
import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path

BASE_COMMIT = "d1b3116ef0b2bdcedbcfd1888f78b2d301a3c654"
BASE_STAGE5C_SHA256 = "4670090bb6046d9c70310ef07dfee2eafaa87f7873627db9de240ee5ab568d40"
BASE_RUNTIME_SHA256 = "aa514c2479a2720a585ce0c386ab91674e125582e013912fba49fe529f8bdd2d"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
RUNTIME = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
R1_CHECKER = "scripts/stage5g_c_r2ca_r1_authority_check.py"
R1_SNAPSHOT = "scripts/stage5g_c_r2ca_r1_snapshot_gate.py"


def sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=root)


def materialize_commit(root: Path, destination: Path) -> None:
    raw = git(root, "ls-tree", "-r", "-z", BASE_COMMIT)
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, path_raw = record.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError(f"unsupported predecessor entry: {metadata.decode()}")
        relative = path_raw.decode("utf-8")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git(root, "cat-file", "blob", object_id))
        if mode == "100755":
            target.chmod(target.stat().st_mode | 0o111)


def run_gate(detached: Path, relative: str) -> None:
    completed = subprocess.run(
        [sys.executable, str(detached / relative), "--root", str(detached)],
        cwd=detached,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise ValueError(
            f"detached predecessor gate failed: {relative}: "
            f"{completed.stdout.strip()} {completed.stderr.strip()}"
        )


def check(root: Path) -> None:
    if not (root / ".git").exists():
        raise ValueError("predecessor proof requires the source Git object database")
    resolved = git(root, "rev-parse", f"{BASE_COMMIT}^{{commit}}").decode().strip()
    if resolved != BASE_COMMIT:
        raise ValueError("R1 predecessor does not resolve exactly")
    if sha256(git(root, "show", f"{BASE_COMMIT}:{STAGE5C}")) != BASE_STAGE5C_SHA256:
        raise ValueError("R1 predecessor Stage 5C bytes drift")
    if sha256(git(root, "show", f"{BASE_COMMIT}:{RUNTIME}")) != BASE_RUNTIME_SHA256:
        raise ValueError("R1 predecessor runtime bytes drift")

    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r2-predecessor-") as raw:
        detached = Path(raw) / "repo"
        detached.mkdir()
        materialize_commit(root, detached)
        run_gate(detached, R1_CHECKER)
        run_gate(detached, R1_SNAPSHOT)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2ca-r2-predecessor-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r2-predecessor-gate: PASS")
    print(f"detached_predecessor: {BASE_COMMIT}")
    print("r1_authority_and_snapshot: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
