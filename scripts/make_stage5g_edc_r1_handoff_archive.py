#!/usr/bin/env python3
"""Create the immutable Stage 5G-e-d-c R1 review handoff."""

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

import stage5g_edc_r1_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-edc-r1-handoff: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (not path.is_absolute() and ".." not in path.parts
            and not any(p in {".git", "target", "tmp", "reports", "__MACOSX"} for p in path.parts)
            and path.name != ".env" and path.suffix != ".log")


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    if run(["git", "rev-parse", "HEAD^"]) != checker.BASE:
        fail("wrong predecessor")
    if run(["git", "branch", "--show-current"]) != checker.BRANCH:
        fail("wrong branch")
    if run(["git", "rev-parse", "origin/stage5g-lifecycle"]) != head:
        fail("origin/stage5g-lifecycle must equal HEAD")
    gate = subprocess.run(["bash", "scripts/stage5g_edc_r1_gate.sh"], cwd=ROOT,
                          text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    redacted = gate.stdout.replace(str(ROOT), "<REPO>").replace(str(Path.home()), "<HOME>").encode()
    if gate.returncode:
        print(redacted.decode(), end="")
        fail(f"gate failed: {gate.returncode}")
    archive = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as source:
        for member in source.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe member: {member.name}")
            handle = source.extractfile(member)
            assert handle is not None
            members.append((member.name, handle.read(), member.mode))
    source_manifest = json.dumps({
        "schema_version": 1, "source_ref": head, "predecessor": checker.BASE,
        "members": [{"path": n, "sha256": hashlib.sha256(d).hexdigest()}
                    for n, d, _ in sorted(members)]
    }, indent=2, sort_keys=True).encode() + b"\n"
    marker = (f"source_ref={head}\nsource_short_ref={short}\nsource_branch={checker.BRANCH}\n"
              f"archive_name=moex-trading-project-{short}.zip\npredecessor={checker.BASE}\n").encode()
    evidence = json.dumps({
        "schema_version": 1, "stage": "Stage 5G-e-d-c R1", "source_ref": head,
        "predecessor": checker.BASE, "gate_exit_code": 0,
        "negative_cases": {"predecessor": 364, "current": 60, "aggregate": 424},
        "grst_witnesses": 12, "failure_boundaries": 14,
        "exact_replay_policy": "PolicyBExactReplayDisabled",
        "closed_surfaces": ["callback", "redis", "finam", "http_post_delete",
                            "broker_dispatch", "runtime_live", "real_orders", "stage6"],
        "gate_sha256": hashlib.sha256(redacted).hexdigest(),
        "source_manifest_sha256": hashlib.sha256(source_manifest).hexdigest(),
    }, indent=2, sort_keys=True).encode() + b"\n"
    members += [
        ("handoff-commit.txt", marker, 0o644),
        ("handoff-evidence/stage5g-edc-r1-full-gate.txt", redacted, 0o644),
        ("handoff-evidence/stage5g-edc-r1-source-manifest.json", source_manifest, 0o644),
        ("handoff-evidence/stage5g-edc-r1-evidence-manifest.json", evidence, 0o644),
        ("handoff-evidence/stage5g-edc-r1-toolchain.txt",
         f"{run(['rustc', '--version'])}\n{run(['cargo', '--version'])}\n".encode(), 0o644),
    ]
    if len({n for n, _, _ in members}) != len(members):
        fail("duplicate member")
    OUT.mkdir(parents=True, exist_ok=True)
    destination = OUT / f"moex-trading-project-{short}.zip"
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)) << 16
            target.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    sidecar = destination.with_suffix(".zip.sha256")
    sidecar.write_text(f"{digest}  {destination.name}\n")
    os.chmod(destination, 0o644); os.chmod(sidecar, 0o644)
    print(destination); print(sidecar); print(digest)


if __name__ == "__main__":
    main()
