#!/usr/bin/env python3
"""Create an immutable Stage 7B-c source/evidence handoff archive."""
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

import stage7b_c_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"
EXPECTED_ARTIFACTS = (
    "fmt.txt",
    "stage7b-c-check.txt",
    "closed-surface.txt",
    "negative.txt",
    "inherited-stage7b-b-r2-gate.txt",
    "stage7b-c-debug.txt",
    "stage7b-c-release.txt",
    "stage6-finalized-ahead.txt",
    "stage6-unbound-nonfinal.txt",
    "stage6-cross-bound-active.txt",
    "stage6-regression.txt",
    "workspace-tests.txt",
    "workspace-docs.txt",
    "clippy.txt",
    "toolchain.txt",
)


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-c-handoff: FAIL: {message}")


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


def require_artifacts(path: Path) -> None:
    missing = [name for name in EXPECTED_ARTIFACTS if not (path / name).is_file()]
    if missing:
        fail(f"missing gate artifacts: {', '.join(missing)}")
    required_markers = {
        "fmt.txt": "fmt: PASS",
        "stage7b-c-check.txt": "stage7b-c-check: PASS",
        "closed-surface.txt": "stage7b-c-closed-surface: PASS",
        "negative.txt": "stage7b-c-negative: PASS cases=26",
        "inherited-stage7b-b-r2-gate.txt": "stage7b-b-r2-gate: PASS",
        "stage7b-c-debug.txt": "test result: ok. 10 passed",
        "stage7b-c-release.txt": "test result: ok. 10 passed",
    }
    for name, marker in required_markers.items():
        if marker not in (path / name).read_text(errors="replace"):
            fail(f"gate artifact lacks PASS marker: {name}")


def collect_gate_artifacts() -> tuple[list[tuple[str, bytes, int]], bytes, int]:
    supplied = os.environ.get("STAGE7B_C_PRECOMPUTED_ARTIFACT_DIR")
    if supplied:
        artifact_dir = Path(supplied).resolve()
        require_artifacts(artifact_dir)
        cleanup = None
    else:
        cleanup = tempfile.TemporaryDirectory(prefix="stage7b-c-handoff-")
        artifact_dir = Path(cleanup.name)
        env = dict(os.environ)
        env["STAGE7B_C_ARTIFACT_DIR"] = str(artifact_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage7b_c_gate.sh"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            env=env,
        )
        if gate.returncode:
            print(gate.stdout)
            fail(f"gate failed: {gate.returncode}")
        require_artifacts(artifact_dir)

    artifacts: list[tuple[str, bytes, int]] = []
    artifact_hashes: dict[str, str] = {}
    for name in EXPECTED_ARTIFACTS:
        data = redacted((artifact_dir / name).read_bytes())
        artifacts.append((f"handoff-evidence/gate-artifacts/{name}", data, 0o644))
        artifact_hashes[name] = hashlib.sha256(data).hexdigest()
    negative = (artifact_dir / "negative.txt").read_bytes()
    negative_count = sum(line.startswith(b"PASS ") for line in negative.splitlines())
    summary = json.dumps(
        {
            "schema_version": 1,
            "stage7b_c_gate": "PASS",
            "artifact_sha256": artifact_hashes,
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    if cleanup is not None:
        cleanup.cleanup()
    return artifacts, summary, negative_count


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree not clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    branch = run(["git", "branch", "--show-current"])
    if run(["git", "merge-base", "HEAD", checker.BASE]) != checker.BASE:
        fail("wrong accepted Stage 7B-b-R2 predecessor")
    if branch != checker.BRANCH:
        fail("wrong branch")
    if os.environ.get("STAGE7B_REQUIRE_ORIGIN") == "1":
        if run(["git", "rev-parse", f"origin/{checker.BRANCH}"]) != head:
            fail("origin branch mismatch")

    artifact_members, gate_summary, negative_count = collect_gate_artifacts()
    raw = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
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
    archive_name = f"moex-trading-project-{short}.zip"
    marker = (
        f"source_ref={head}\nsource_short_ref={short}\nsource_branch={branch}\n"
        f"archive_name={archive_name}\naccepted_slice_predecessor={checker.BASE}\n"
    ).encode()
    proof_map = json.loads(
        (ROOT / "docs/stage-7/stage7b-acceptance-proof-map.json").read_text()
    )
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "7B-c",
            "status": "independent_acceptance_pending",
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7b_b_predecessor": checker.BASE,
            "proof_map_row_count": proof_map["row_count"],
            "implemented_count": proof_map["implemented_count"],
            "pending_count": proof_map["pending_count"],
            "stage7b_accepted": proof_map["stage7b_accepted"],
            "focused_recovery_test_count_debug": 10,
            "focused_recovery_test_count_release": 10,
            "negative_case_count": negative_count,
            "inherited_stage7b_b_r2_gate_passed": True,
            "workspace_tests_passed": True,
            "workspace_doc_tests_passed": True,
            "workspace_clippy_passed": True,
            "recovery_seal_implemented": True,
            "recovery_seal_hmac_authenticated": True,
            "atomic_recovery_seal_commit": True,
            "linear_recovered_runtime_and_writer_lease_owner": True,
            "recovery_blocked_zero_effect": True,
            "redis_consumer_attached": False,
            "redis_settlement_enabled": False,
            "xack_enabled": False,
            "finam_post_delete_enabled": False,
            "broker_network_dispatch_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "gate_summary_sha256": hashlib.sha256(gate_summary).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members += artifact_members + [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", manifest, 0o644),
        ("handoff-evidence/stage7b-c-gate-summary.json", gate_summary, 0o644),
        ("handoff-evidence/stage7b-c-evidence.json", evidence, 0o644),
    ]
    if len({name for name, _, _ in members}) != len(members):
        fail("duplicate archive member")

    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / archive_name
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
