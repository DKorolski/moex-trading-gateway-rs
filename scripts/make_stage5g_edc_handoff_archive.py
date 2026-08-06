#!/usr/bin/env python3
"""Build a deterministic, self-identifying Stage 5G-e-d-c review archive."""

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

import stage5g_edc_check as checker


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-edc-handoff: FAIL: {message}")


def redact_local_paths(value: str) -> str:
    """Keep review evidence portable without exposing workstation paths."""
    return value.replace(str(ROOT), "<REPO>").replace(str(Path.home()), "<HOME>")


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
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    if run(["git", "rev-parse", "HEAD^"]) != checker.BASE_REF:
        fail("HEAD is not the direct accepted-R5 successor")
    if run(["git", "branch", "--show-current"]) != "stage5g-lifecycle":
        fail("wrong branch")
    remote = run(["git", "rev-parse", "origin/stage5g-lifecycle"])
    if remote != head:
        fail("origin/stage5g-lifecycle must equal HEAD")

    print("stage5g-edc-handoff: running full gate", flush=True)
    gate = subprocess.run(
        ["bash", "scripts/stage5g_edc_gate.sh"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    gate_output = redact_local_paths(gate.stdout).encode()
    if gate.returncode != 0:
        print(gate_output.decode(), end="")
        fail(f"full gate failed with exit code {gate.returncode}")
    archive = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as source:
        for member in source.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe git archive member: {member.name}")
            handle = source.extractfile(member)
            if handle is None:
                fail(f"cannot read member: {member.name}")
            members.append((member.name, handle.read(), member.mode))

    source_manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "accepted_predecessor": checker.BASE_REF,
            "members": [
                {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
                for name, data, _ in sorted(members)
            ],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    toolchain = redact_local_paths(
        f"{run(['rustc', '--version'])}\n{run(['cargo', '--version'])}\n"
    ).encode()
    handoff = (
        f"source_ref={head}\n"
        f"source_short_ref={short}\n"
        f"source_branch=stage5g-lifecycle\n"
        f"archive_name=moex-trading-project-{short}.zip\n"
        f"accepted_predecessor={checker.BASE_REF}\n"
    ).encode()
    evidence_manifest = json.dumps(
        {
            "schema_version": 1,
            "stage": "Stage 5G-e-d-c",
            "source_ref": head,
            "accepted_predecessor": checker.BASE_REF,
            "gate_exit_code": gate.returncode,
            "replay_policy": "PolicyBExactReplayDisabled",
            "focused_tests": 9,
            "compile_fail_witnesses": 10,
            "negative_cases": {
                "current": 88,
                "inherited": 276,
                "aggregate": 364,
            },
            "closed_surfaces": [
                "strategy_callback",
                "redis",
                "finam_transport",
                "http_post_delete",
                "broker_dispatch",
                "runtime_live",
                "real_orders",
                "stage6_durability",
            ],
            "artifacts": {
                "gate_output_sha256": hashlib.sha256(gate_output).hexdigest(),
                "source_manifest_sha256": hashlib.sha256(source_manifest).hexdigest(),
                "toolchain_sha256": hashlib.sha256(toolchain).hexdigest(),
                "handoff_commit_sha256": hashlib.sha256(handoff).hexdigest(),
            },
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members.extend([
        ("handoff-evidence/stage5g-edc-full-gate.txt", gate_output, 0o644),
        ("handoff-evidence/stage5g-edc-source-manifest.json", source_manifest, 0o644),
        ("handoff-evidence/stage5g-edc-toolchain.txt", toolchain, 0o644),
        ("handoff-evidence/stage5g-edc-evidence-manifest.json", evidence_manifest, 0o644),
    ])
    members.append(("handoff-commit.txt", handoff, 0o644))
    names = [name for name, _, _ in members]
    if len(names) != len(set(names)):
        fail("duplicate member")

    OUT.mkdir(parents=True, exist_ok=True)
    destination = OUT / f"moex-trading-project-{short}.zip"
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            permissions = stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)
            info.external_attr = permissions << 16
            target.writestr(info, data, compress_type=zipfile.ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    sidecar = destination.with_suffix(destination.suffix + ".sha256")
    sidecar.write_text(f"{digest}  {destination.name}\n")
    os.chmod(destination, 0o644)
    os.chmod(sidecar, 0o644)
    print(destination)
    print(sidecar)
    print(digest)


if __name__ == "__main__":
    main()
