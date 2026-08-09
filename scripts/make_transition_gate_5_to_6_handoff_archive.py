#!/usr/bin/env python3
"""Create immutable Transition Gate 5->6 review handoff."""

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

import transition_gate_5_to_6_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"transition-gate-5-to-6-handoff: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and not any(
        part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts
    ) and path.name != ".env" and path.suffix != ".log"


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

    gate = subprocess.run(
        ["bash", "scripts/transition_gate_5_to_6.sh"], cwd=ROOT, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
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
        "schema_version": 1,
        "source_ref": head,
        "predecessor": checker.BASE,
        "members": [{"path": name, "sha256": hashlib.sha256(data).hexdigest()} for name, data, _ in sorted(members)],
    }, indent=2, sort_keys=True).encode() + b"\n"
    marker = (
        f"source_ref={head}\nsource_short_ref={short}\nsource_branch={checker.BRANCH}\n"
        f"archive_name=moex-trading-project-{short}.zip\npredecessor={checker.BASE}\n"
    ).encode()
    evidence = json.dumps({
        "schema_version": 1,
        "gate": "Transition Gate 5->6",
        "source_ref": head,
        "predecessor": checker.BASE,
        "gate_exit_code": 0,
        "negative_case_count": 76,
        "authority_count": 17,
        "crash_window_count": 10,
        "stage6_slice_count": 5,
        "stage6_open_pending_review": False,
        "execution_surfaces_open": False,
        "source_manifest_sha256": hashlib.sha256(source_manifest).hexdigest(),
        "gate_sha256": hashlib.sha256(redacted).hexdigest(),
    }, indent=2, sort_keys=True).encode() + b"\n"
    additions = [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", source_manifest, 0o644),
        ("handoff-evidence/transition-5-to-6-full-gate.txt", redacted, 0o644),
        ("handoff-evidence/transition-5-to-6-evidence-manifest.json", evidence, 0o644),
        ("handoff-evidence/transition-5-to-6-descriptor.json", (ROOT / checker.DESCRIPTOR).read_bytes(), 0o644),
        ("handoff-evidence/transition-5-to-6-authority-inventory.json", (ROOT / checker.INVENTORY).read_bytes(), 0o644),
        ("handoff-evidence/stage5g-h-acceptance-reference.json", (ROOT / checker.ACCEPTANCE).read_bytes(), 0o644),
        ("handoff-evidence/transition-5-to-6-toolchain.txt", f"{run(['rustc','--version'])}\n{run(['cargo','--version'])}\n".encode(), 0o644),
    ]
    members.extend(additions)
    if len({name for name, _, _ in members}) != len(members):
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
