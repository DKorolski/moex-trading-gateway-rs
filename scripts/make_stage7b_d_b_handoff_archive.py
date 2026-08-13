#!/usr/bin/env python3
"""Create an immutable Stage 7B-d-b source/evidence handoff ZIP."""
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

import stage7b_d_b_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"
ARTIFACTS = (
    "redis-toolchain.txt",
    "fmt.txt",
    "stage7b-d-b-check.txt",
    "negative.txt",
    "inherited-d-a-gate.txt",
    "stage7b-d-b-debug.txt",
    "stage7b-d-b-release.txt",
    "workspace-tests.txt",
    "workspace-docs.txt",
    "clippy.txt",
    "toolchain.txt",
)


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-d-b-handoff: FAIL: {message}")


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


def require_artifacts(directory: Path) -> None:
    missing = [name for name in ARTIFACTS if not (directory / name).is_file()]
    if missing:
        fail(f"missing gate artifacts: {', '.join(missing)}")
    markers = {
        "fmt.txt": "fmt: PASS",
        "stage7b-d-b-check.txt": "stage7b-d-b-check: PASS rows=7",
        "negative.txt": "stage7b-d-b-negative: PASS cases=26",
        "inherited-d-a-gate.txt": "stage7b-d-a-gate: PASS",
        "stage7b-d-b-debug.txt": "8 passed; 0 failed",
        "stage7b-d-b-release.txt": "8 passed; 0 failed",
        "workspace-tests.txt": "test result: ok",
        "workspace-docs.txt": "test result: ok",
        "clippy.txt": "Finished `dev` profile",
    }
    for name, marker in markers.items():
        if marker not in (directory / name).read_text(errors="replace"):
            fail(f"gate artifact lacks marker: {name}: {marker}")


def collect_artifacts() -> tuple[list[tuple[str, bytes, int]], dict[str, str]]:
    supplied = os.environ.get("STAGE7B_D_B_PRECOMPUTED_ARTIFACT_DIR")
    cleanup: tempfile.TemporaryDirectory[str] | None = None
    if supplied:
        directory = Path(supplied).resolve()
    else:
        cleanup = tempfile.TemporaryDirectory(prefix="stage7b-d-b-handoff-")
        directory = Path(cleanup.name)
        environment = dict(os.environ)
        environment["STAGE7B_D_B_ARTIFACT_DIR"] = str(directory)
        result = subprocess.run(
            ["bash", "scripts/stage7b_d_b_gate.sh"],
            cwd=ROOT,
            env=environment,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        if result.returncode:
            print(result.stdout)
            fail(f"gate failed: {result.returncode}")
    require_artifacts(directory)
    members: list[tuple[str, bytes, int]] = []
    hashes: dict[str, str] = {}
    for name in ARTIFACTS:
        data = redacted((directory / name).read_bytes())
        members.append((f"handoff-evidence/gate-artifacts/{name}", data, 0o644))
        hashes[name] = hashlib.sha256(data).hexdigest()
    if cleanup is not None:
        cleanup.cleanup()
    return members, hashes


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree not clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    branch = run(["git", "branch", "--show-current"])
    if branch != checker.BRANCH:
        fail("wrong branch")
    if run(["git", "merge-base", "HEAD", checker.ACCEPTED_D_A]) != checker.ACCEPTED_D_A:
        fail("accepted d-a-R1 is not an ancestor")
    if os.environ.get("STAGE7B_REQUIRE_ORIGIN") == "1":
        if run(["git", "rev-parse", f"origin/{branch}"]) != head:
            fail("origin branch mismatch")

    evidence_members, artifact_hashes = collect_artifacts()
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

    manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7b_d_a_ref": checker.ACCEPTED_D_A,
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
        f"archive_name={archive_name}\naccepted_stage7b_d_a_ref={checker.ACCEPTED_D_A}\n"
    ).encode()
    proof_map = json.loads(
        (ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text()
    )
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "7B-d-b",
            "status": "independent_acceptance_pending",
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7b_d_a_ref": checker.ACCEPTED_D_A,
            "implemented_rows": 61,
            "pending_rows": 19,
            "d_b_owned_rows": sorted(checker.OWNED),
            "b052_b053_status": "pending_d_c_real_redis_restart",
            "focused_debug_tests_passed": 8,
            "focused_release_tests_passed": 8,
            "negative_case_count": 26,
            "atomic_ack_xack": True,
            "atomic_redacted_dlq_xack": True,
            "response_loss_idempotency": True,
            "artifact_sha256": artifact_hashes,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "redis_consumer_attached": False,
            "stage7b_d_c_open": False,
            "finam_post_delete_enabled": False,
            "broker_network_dispatch_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
            "stage7b_accepted": proof_map["stage7b_accepted"],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members = source_members + evidence_members + [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", manifest, 0o644),
        ("handoff-evidence/stage7b-d-b-evidence.json", evidence, 0o644),
    ]
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate archive member")

    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / archive_name
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
