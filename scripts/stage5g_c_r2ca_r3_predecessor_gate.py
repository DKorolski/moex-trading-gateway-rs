#!/usr/bin/env python3
"""Run the rejected-but-pinned R2 gates against the exact R3 predecessor."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

BASE_COMMIT = "3d995af48e88588909e11505fdefc826ff8f66ce"
BASE_STAGE5C_SHA256 = "541b3dfffc838bd939790210c0a63e988a1c1d4a66f69bba52914a494b4cc3ea"
BASE_RUNTIME_SHA256 = "fda7593117c41797d2a98e534937b53ead18451e6a3c89c5196eace0207959f3"
STAGE5C = "crates/strategy-runtime-core/src/stage5c_paper_host.rs"
RUNTIME = "crates/strategy-runtime-core/src/hybrid_intraday_runtime.rs"
R2_CHECKER = "scripts/stage5g_c_r2ca_r2_authority_check.py"
R2_SNAPSHOT = "scripts/stage5g_c_r2ca_r2_snapshot_gate.py"


def sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def git(root: Path, *args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=root)


def materialize_commit(root: Path, destination: Path) -> None:
    for record in git(root, "ls-tree", "-r", "-z", BASE_COMMIT).split(b"\0"):
        if not record:
            continue
        metadata, path_raw = record.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise ValueError(f"unsupported predecessor entry: {metadata.decode()}")
        target = destination / path_raw.decode("utf-8")
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
            f"detached R2 gate failed: {relative}: "
            f"{completed.stdout.strip()} {completed.stderr.strip()}"
        )


def check(root: Path) -> None:
    if not (root / ".git").exists():
        raise ValueError("predecessor proof requires the source Git object database")
    resolved = git(root, "rev-parse", f"{BASE_COMMIT}^{{commit}}").decode().strip()
    if resolved != BASE_COMMIT:
        raise ValueError("R2 predecessor does not resolve exactly")
    for relative, expected in (
        (STAGE5C, BASE_STAGE5C_SHA256),
        (RUNTIME, BASE_RUNTIME_SHA256),
    ):
        if sha256(git(root, "show", f"{BASE_COMMIT}:{relative}")) != expected:
            raise ValueError(f"R2 predecessor bytes drift: {relative}")
    with tempfile.TemporaryDirectory(prefix="stage5g-r2ca-r3-predecessor-") as raw:
        detached = Path(raw) / "repo"
        detached.mkdir()
        materialize_commit(root, detached)
        run_gate(detached, R2_CHECKER)
        run_gate(detached, R2_SNAPSHOT)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        check(args.root.resolve())
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"stage5g-c-r2ca-r3-predecessor-gate: FAIL: {error}", file=sys.stderr)
        return 1
    print("stage5g-c-r2ca-r3-predecessor-gate: PASS")
    print(f"detached_predecessor: {BASE_COMMIT}")
    print("r2_authority_and_snapshot: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
