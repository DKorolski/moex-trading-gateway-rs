#!/usr/bin/env python3
"""Create an immutable Transition Gate 7->8 review handoff."""

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

import transition_gate_7_to_8_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"transition-gate-7-to-8-handoff: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def redacted(data: bytes) -> bytes:
    return data.replace(str(ROOT).encode(), b"<REPO>").replace(str(Path.home()).encode(), b"<HOME>")


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    branch = run(["git", "branch", "--show-current"])
    if branch != checker.BRANCH:
        fail("wrong branch")
    if run(["git", "merge-base", "HEAD", checker.PREDECESSOR]) != checker.PREDECESSOR:
        fail("closure predecessor is not an ancestor")
    if run(["git", "rev-parse", f"origin/{branch}"]) != head:
        fail("origin branch must equal HEAD")

    gate = subprocess.run(
        ["bash", "scripts/transition_gate_7_to_8.sh"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    gate_output = redacted(gate.stdout.encode())
    if gate.returncode:
        print(gate_output.decode(errors="replace"), end="")
        fail(f"gate failed: {gate.returncode}")
    if b"transition-gate-7-to-8: PASS rows=45 negatives=20 stage8-implementation=closed" not in gate_output:
        fail("full gate completion marker missing")

    raw = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    source_members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe source member: {member.name}")
            payload = archive.extractfile(member)
            if payload is None:
                fail(f"missing source payload: {member.name}")
            source_members.append((member.name, payload.read(), member.mode))

    source_manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "source_branch": branch,
            "predecessor": checker.PREDECESSOR,
            "accepted_stage7b_ref": checker.ACCEPTED_STAGE7B,
            "members": [
                {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
                for name, data, _ in sorted(source_members)
            ],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    archive_name = f"moex-trading-project-{short}.zip"
    marker = (
        f"source_ref={head}\nsource_short_ref={short}\nsource_branch={branch}\n"
        f"archive_name={archive_name}\npredecessor={checker.PREDECESSOR}\n"
        f"accepted_stage7b_ref={checker.ACCEPTED_STAGE7B}\n"
        "candidate_gate=Transition Gate 7->8\n"
        "candidate_status=independent_acceptance_pending\n"
        "stage8_implementation_authorized=false\n"
        "real_finam_execution_authorized=false\n"
    ).encode()
    toolchain = f"{run(['rustc', '--version'])}\n{run(['cargo', '--version'])}\n".encode()
    evidence = json.dumps(
        {
            "schema_version": 1,
            "gate": "Transition Gate 7->8",
            "status": "independent_acceptance_pending",
            "source_ref": head,
            "source_branch": branch,
            "predecessor": checker.PREDECESSOR,
            "accepted_stage7b_ref": checker.ACCEPTED_STAGE7B,
            "acceptance_rows": 45,
            "negative_cases": 20,
            "full_gate_exit_code": 0,
            "stage8_implementation_authorized": False,
            "finam_post_delete_authorized": False,
            "broker_dispatch_authorized": False,
            "runtime_live_authorized": False,
            "real_strategy_orders_authorized": False,
            "source_manifest_sha256": hashlib.sha256(source_manifest).hexdigest(),
            "full_gate_sha256": hashlib.sha256(gate_output).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members = list(source_members)
    members.extend(
        [
            ("handoff-commit.txt", marker, 0o644),
            ("source-tree-manifest.json", source_manifest, 0o644),
            ("handoff-evidence/transition-gate-7-to-8-full-gate.txt", gate_output, 0o644),
            ("handoff-evidence/transition-gate-7-to-8-evidence.json", evidence, 0o644),
            ("handoff-evidence/transition-gate-7-to-8-descriptor.json", (ROOT / checker.DESCRIPTOR).read_bytes(), 0o644),
            ("handoff-evidence/transition-gate-7-to-8-toolchain.txt", toolchain, 0o644),
        ]
    )
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate archive member")

    OUT.mkdir(parents=True, exist_ok=True)
    destination = OUT / archive_name
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)) << 16
            target.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    sidecar = destination.with_suffix(".zip.sha256")
    sidecar.write_text(f"{digest}  {destination.name}\n")
    os.chmod(destination, 0o644)
    os.chmod(sidecar, 0o644)
    print(destination)
    print(sidecar)
    print(digest)


if __name__ == "__main__":
    main()
