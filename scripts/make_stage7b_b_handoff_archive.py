#!/usr/bin/env python3
"""Create an immutable Stage 7B-b-R2 source and evidence handoff."""
from __future__ import annotations

import hashlib
import io
import json
import os
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage7b_b_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-b-handoff: FAIL: {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(
            part in {".git", "target", "tmp", "reports", "__MACOSX"}
            for part in path.parts
        )
        and path.name != ".env"
        and path.suffix != ".log"
    )


def redacted(data: bytes) -> bytes:
    return data.replace(str(ROOT).encode(), b"<REPO>").replace(
        str(Path.home()).encode(), b"<HOME>"
    )


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree not clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    branch = run(["git", "branch", "--show-current"])
    if run(["git", "merge-base", "HEAD", checker.BASE]) != checker.BASE:
        fail("wrong accepted Stage 7B-a-R1 predecessor")
    if branch != checker.BRANCH:
        fail("wrong branch")
    if os.environ.get("STAGE7B_REQUIRE_ORIGIN") == "1":
        if run(["git", "rev-parse", f"origin/{checker.BRANCH}"]) != head:
            fail("origin branch mismatch")

    with tempfile.TemporaryDirectory(prefix="stage7b-b-handoff-") as temp:
        artifact_dir = Path(temp)
        env = dict(os.environ)
        env["STAGE7B_B_ARTIFACT_DIR"] = str(artifact_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage7b_b_gate.sh"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        )
        gate_output = redacted(gate.stdout.encode())
        if gate.returncode:
            print(gate.stdout)
            fail(f"gate failed: {gate.returncode}")
        proof_map = json.loads(
            (ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text()
        )
        negative = (artifact_dir / "negative.txt").read_bytes()
        negative_count = sum(
            line.startswith(b"PASS ") for line in negative.splitlines()
        )
        artifact_members: list[tuple[str, bytes, int]] = []
        for path in sorted(artifact_dir.iterdir()):
            if path.is_file():
                artifact_members.append(
                    (
                        f"handoff-evidence/gate-artifacts/{path.name}",
                        redacted(path.read_bytes()),
                        0o644,
                    )
                )

    raw = subprocess.check_output(
        ["git", "archive", "--format=tar", "HEAD"], cwd=ROOT
    )
    members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe member: {member.name}")
            extracted = archive.extractfile(member)
            if extracted is None:
                fail(f"missing payload: {member.name}")
            members.append((member.name, extracted.read(), member.mode))

    manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "source_branch": branch,
            "accepted_slice_predecessor": checker.BASE,
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
        f"source_branch={branch}\n"
        f"archive_name=moex-trading-project-{short}.zip\n"
        f"accepted_slice_predecessor={checker.BASE}\n"
    ).encode()
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "7B-b-R2",
            "status": "independent_acceptance_pending",
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7a_predecessor": checker.STAGE7A_BASE,
            "accepted_slice_predecessor": checker.BASE,
            "gate_exit_code": 0,
            "proof_map_row_count": proof_map["row_count"],
            "implemented_count": proof_map["implemented_count"],
            "pending_count": proof_map["pending_count"],
            "stage7b_accepted": proof_map["stage7b_accepted"],
            "focused_stage7b_b_test_count": 18,
            "focused_stage7b_core_test_count": 9,
            "real_subprocess_test_count": 4,
            "negative_case_count": negative_count,
            "inherited_stage7b_a_r1_gate_passed": True,
            "workspace_tests_passed": True,
            "workspace_doc_tests_passed": True,
            "workspace_clippy_passed": True,
            "durable_path_validation": True,
            "single_writer_implemented": True,
            "root_directory_fd_anchored": True,
            "trusted_parent_directory_fd_anchored": True,
            "anchored_child_openat": True,
            "identity_scoped_parent_namespace_lock": True,
            "lock_namespace_lifetime_validation": True,
            "constructor_identity_rebind_guard": True,
            "full_identity_digest_revalidated": True,
            "recovery_seal_implemented": False,
            "redis_consumer_attached": False,
            "cross_process_fault_matrix_implemented": False,
            "cross_process_exactly_once_claimed": False,
            "finam_post_delete_enabled": False,
            "broker_network_dispatch_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "gate_sha256": hashlib.sha256(gate_output).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members += [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", manifest, 0o644),
        ("handoff-evidence/stage7b-b-full-gate.txt", gate_output, 0o644),
        ("handoff-evidence/stage7b-b-evidence.json", evidence, 0o644),
        ("handoff-evidence/stage7b-b-negative.txt", negative, 0o644),
        (
            "handoff-evidence/stage7b-b-toolchain.txt",
            f"{run(['rustc', '--version'])}\n{run(['cargo', '--version'])}\n".encode(),
            0o644,
        ),
    ] + artifact_members
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate archive member")

    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / f"moex-trading-project-{short}.zip"
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data, mode in sorted(members):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = (
                stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)
            ) << 16
            archive.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    sidecar = target.with_suffix(".zip.sha256")
    sidecar.write_text(f"{digest}  {target.name}\n")
    print(target)
    print(sidecar)
    print(digest)


if __name__ == "__main__":
    main()
