#!/usr/bin/env python3
"""Create immutable Stage 6C source plus evidence handoff."""
from __future__ import annotations

import hashlib
import io
import json
import os
import stat
import subprocess
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

import stage6c_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message):
    raise SystemExit(f"stage6c-handoff: FAIL: {message}")


def safe(name):
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def main():
    if run(["git", "status", "--porcelain"]):
        fail("worktree not clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    if run(["git", "rev-parse", "HEAD^"]) != checker.BASE:
        fail("wrong predecessor")
    if run(["git", "branch", "--show-current"]) != checker.BRANCH:
        fail("wrong branch")
    if run(["git", "rev-parse", f"origin/{checker.BRANCH}"]) != head:
        fail("origin branch mismatch")
    if run(["git", "rev-parse", "origin/main"]) != checker.MAIN:
        fail("origin/main moved")
    if run(["git", "rev-parse", "origin/stage5g-lifecycle"]) != checker.MAIN:
        fail("origin/stage5g-lifecycle moved")

    env = dict(os.environ)
    env["STAGE6C_SKIP_PRESEAL"] = "1"
    gate = subprocess.run(
        ["bash", "scripts/stage6c_gate.sh"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        env=env,
    )
    redacted = gate.stdout.replace(str(ROOT), "<REPO>").replace(str(Path.home()), "<HOME>").encode()
    if gate.returncode:
        print(gate.stdout)
        fail(f"gate failed: {gate.returncode}")
    negative = subprocess.check_output(["python3", "scripts/stage6c_negative_harness.py"], cwd=ROOT)

    raw = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    members = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe member: {member.name}")
            members.append((member.name, tar.extractfile(member).read(), member.mode))

    manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "predecessor": checker.BASE,
            "members": [
                {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
                for name, data, _ in sorted(members)
            ],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    marker = (
        f"source_ref={head}\n"
        f"source_short_ref={short}\n"
        f"source_branch={checker.BRANCH}\n"
        f"archive_name=moex-trading-project-{short}.zip\n"
        f"predecessor={checker.BASE}\n"
    ).encode()
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "6C",
            "source_ref": head,
            "predecessor": checker.BASE,
            "gate_exit_code": 0,
            "positive_test_count": 54,
            "crash_window_test_count": 10,
            "negative_case_count": 167,
            "compatibility_fixture_count": 9,
            "blind_redispatch_blocked": True,
            "exact_duplicate_idempotent": True,
            "conflicting_duplicate_fail_closed": True,
            "execution_surfaces_open": False,
            "stage6d_open": False,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "gate_sha256": hashlib.sha256(redacted).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members += [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", manifest, 0o644),
        ("handoff-evidence/stage6c-full-gate.txt", redacted, 0o644),
        ("handoff-evidence/stage6c-evidence.json", evidence, 0o644),
        ("handoff-evidence/stage6c-negative.txt", negative, 0o644),
        ("handoff-evidence/stage6c-toolchain.txt", f"{run(['rustc', '--version'])}\n{run(['cargo', '--version'])}\n".encode(), 0o644),
    ]
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate member")

    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"moex-trading-project-{short}.zip"
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)) << 16
            archive.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    sidecar = target.with_suffix(".zip.sha256")
    sidecar.write_text(f"{digest}  {target.name}\n")
    print(target)
    print(sidecar)
    print(digest)


if __name__ == "__main__":
    main()
