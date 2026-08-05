#!/usr/bin/env python3
"""Build an immutable, origin-bound Stage 5G-e-d-a R4 review handoff."""

from __future__ import annotations

import hashlib
import io
import json
import subprocess
import tarfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


ROOT = Path(__file__).resolve().parents[1]
HANDOFF_DIR = ROOT / "reports/handoff"
BRANCH = "stage5g-lifecycle"
STAGE = "5G-e-d-a-r4"
REQUIRED_PARENT = "2ebb097eab73708b142c0bc26da217f1404a81aa"


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage5g-ed-handoff: FAIL: {message}")


def validate_member(name: str) -> None:
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or name.startswith("/"):
        fail(f"unsafe archive path: {name}")
    if any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts):
        fail(f"forbidden archive member: {name}")
    if path.name == ".env" or path.suffix == ".log":
        fail(f"secret/log member forbidden: {name}")


def main() -> None:
    status = run_text(["git", "status", "--porcelain", "--untracked-files=all"])
    if status:
        fail(f"source tree is dirty:\n{status}")
    branch = run_text(["git", "branch", "--show-current"])
    if branch != BRANCH:
        fail(f"expected branch {BRANCH}, got {branch}")
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    parent_ref = run_text(["git", "rev-parse", "HEAD^"])
    if parent_ref != REQUIRED_PARENT:
        fail(f"R4 must be one clean successor to {REQUIRED_PARENT}; got parent {parent_ref}")
    origin_ref = run_text(["git", "rev-parse", f"origin/{BRANCH}"])
    if origin_ref != source_ref:
        fail(f"origin/{BRANCH} must equal HEAD before packaging")

    gate = subprocess.run(
        ["bash", "scripts/stage5g_eda_r4_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0:
        print(gate.stdout.decode(errors="replace"))
        fail("stage5g_eda_r4_gate.sh failed")

    archive_name = f"moex-trading-project-{source_commit}.zip"
    HANDOFF_DIR.mkdir(parents=True, exist_ok=True)
    archive_path = HANDOFF_DIR / archive_name
    sha_path = Path(f"{archive_path}.sha256")

    tar_bytes = subprocess.check_output(
        ["git", "archive", "--format=tar", source_ref], cwd=ROOT
    )
    payloads: dict[str, tuple[bytes, int]] = {}
    with tarfile.open(fileobj=io.BytesIO(tar_bytes), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile():
                fail(f"non-regular tracked member: {member.name}")
            validate_member(member.name)
            extracted = archive.extractfile(member)
            if extracted is None:
                fail(f"cannot read tracked member: {member.name}")
            payloads[member.name] = (extracted.read(), member.mode)

    source_manifest = {
        "schema_version": 1,
        "stage": STAGE,
        "source_ref": source_ref,
        "source_branch": branch,
        "members": [
            {
                "path": name,
                "mode": f"{mode:o}",
                "size": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }
            for name, (payload, mode) in sorted(payloads.items())
        ],
    }
    marker = (
        f"stage={STAGE}\n"
        f"source_ref={source_ref}\n"
        f"source_commit={source_commit}\n"
        f"source_branch={branch}\n"
        f"archive_name={archive_name}\n"
        f"parent_ref={parent_ref}\n"
        f"origin_ref={origin_ref}\n"
    ).encode()
    gate_result = json.dumps(
        {
            "schema_version": 1,
            "stage": STAGE,
            "source_ref": source_ref,
            "command": ["bash", "scripts/stage5g_eda_r4_gate.sh"],
            "exit_code": 0,
            "all_required_gates_passed": True,
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    generated = {
        "handoff-commit.txt": (marker, 0o644),
        "handoff-source-tree-manifest.json": (
            json.dumps(source_manifest, indent=2, sort_keys=True).encode() + b"\n",
            0o644,
        ),
        "stage5g-e-d-a-r4-gate-result.json": (gate_result, 0o644),
        "stage5g-e-d-a-r4-gate-output.txt": (gate.stdout, 0o644),
    }
    for name in generated:
        if name in payloads:
            fail(f"generated member collides with tracked member: {name}")
    payloads.update(generated)

    commit_epoch = int(run_text(["git", "show", "-s", "--format=%ct", source_ref]))
    commit_dt = datetime.fromtimestamp(commit_epoch, tz=timezone.utc)
    zip_dt = (
        max(commit_dt.year, 1980),
        commit_dt.month,
        commit_dt.day,
        commit_dt.hour,
        commit_dt.minute,
        commit_dt.second - commit_dt.second % 2,
    )
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, (payload, mode) in sorted(payloads.items()):
            info = zipfile.ZipInfo(name, date_time=zip_dt)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = (0o100000 | mode) << 16
            archive.writestr(info, payload)

    archive_sha256 = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    sha_path.write_text(f"{archive_sha256}  {archive_name}\n")

    with zipfile.ZipFile(archive_path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            fail("duplicate ZIP members")
        if archive.read("handoff-commit.txt") != marker:
            fail("commit marker verification failed")
        for name in names:
            validate_member(name)

    print(f"stage5g-ed-handoff: PASS")
    print(f"archive={archive_path}")
    print(f"sha256={archive_sha256}")
    print(f"sidecar={sha_path}")


if __name__ == "__main__":
    main()
