#!/usr/bin/env python3
"""Create an immutable Stage 7B-d design/entry handoff archive."""
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

import stage7b_d_design_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"
ARTIFACTS = (
    "fmt.txt",
    "design-check.txt",
    "negative.txt",
    "python-compile.txt",
    "production-diff.txt",
    "diff-check.txt",
    "toolchain.txt",
)


def run(*args: str) -> str:
    return subprocess.check_output(list(args), cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7b-d-design-handoff: FAIL {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def collect_artifacts() -> list[tuple[str, bytes, int]]:
    supplied = os.environ.get("STAGE7B_D_DESIGN_PRECOMPUTED_ARTIFACT_DIR")
    cleanup = None
    if supplied:
        artifact_dir = Path(supplied).resolve()
    else:
        cleanup = tempfile.TemporaryDirectory(prefix="stage7b-d-design-handoff-")
        artifact_dir = Path(cleanup.name)
        env = dict(os.environ)
        env["STAGE7B_D_DESIGN_ARTIFACT_DIR"] = str(artifact_dir)
        subprocess.run(["bash", "scripts/stage7b_d_design_gate.sh"], cwd=ROOT, env=env, check=True)
    missing = [name for name in ARTIFACTS if not (artifact_dir / name).is_file()]
    if missing:
        fail(f"missing artifacts: {missing}")
    markers = {
        "fmt.txt": "fmt: PASS",
        "design-check.txt": "stage7b-d-design-check: PASS",
        "negative.txt": "stage7b-d-design-negative: PASS cases=44",
        "python-compile.txt": "python-compile: PASS",
        "production-diff.txt": "accepted-stage7b-c-production-tree-unchanged: PASS",
        "diff-check.txt": "diff-check: PASS",
    }
    members = []
    for name in ARTIFACTS:
        data = (artifact_dir / name).read_bytes()
        marker = markers.get(name)
        if marker and marker.encode() not in data:
            fail(f"artifact lacks marker: {name}")
        data = data.replace(str(ROOT).encode(), b"<REPO>").replace(str(Path.home()).encode(), b"<HOME>")
        members.append((f"handoff-evidence/gate-artifacts/{name}", data, 0o644))
    if cleanup:
        cleanup.cleanup()
    return members


def main() -> None:
    if run("git", "status", "--porcelain"):
        fail("worktree not clean")
    head = run("git", "rev-parse", "HEAD")
    short = run("git", "rev-parse", "--short=7", "HEAD")
    branch = run("git", "branch", "--show-current")
    if run("git", "merge-base", "HEAD", checker.BASE) != checker.BASE:
        fail("wrong Stage 7B-c predecessor")
    if branch != checker.BRANCH:
        fail("wrong branch")
    if os.environ.get("STAGE7B_REQUIRE_ORIGIN") == "1" and run(
        "git", "rev-parse", f"origin/{branch}"
    ) != head:
        fail("origin branch mismatch")

    raw = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe source member: {member.name}")
            handle = archive.extractfile(member)
            if handle is None:
                fail(f"missing source payload: {member.name}")
            members.append((member.name, handle.read(), member.mode))

    artifact_members = collect_artifacts()
    archive_name = f"moex-trading-project-{short}.zip"
    manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7b_c_predecessor": checker.BASE,
            "members": [
                {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
                for name, data, _ in sorted(members)
            ],
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "7B-d-design-R1",
            "status": "independent_design_acceptance_pending",
            "source_ref": head,
            "source_branch": branch,
            "accepted_stage7b_c_predecessor": checker.BASE,
            "proof_map_rows": 80,
            "implemented_count": 42,
            "pending_count": 38,
            "design_negative_cases": 44,
            "production_diff_from_accepted_stage7b_c": False,
            "redis_consumer_attached": False,
            "redis_settlement_enabled": False,
            "xack_enabled": False,
            "cross_process_exactly_once_claimed": False,
            "finam_post_delete": False,
            "broker_network_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    marker = (
        f"source_ref={head}\nsource_short_ref={short}\nsource_branch={branch}\n"
        f"archive_name={archive_name}\naccepted_stage7b_c_predecessor={checker.BASE}\n"
    ).encode()
    members.extend(artifact_members)
    members.extend(
        [
            ("handoff-commit.txt", marker, 0o644),
            ("handoff-evidence/source-tree-manifest.json", manifest, 0o644),
            ("handoff-evidence/stage7b-d-design-evidence.json", evidence, 0o644),
        ]
    )
    names = [name for name, _, _ in members]
    if len(names) != len(set(names)):
        fail("duplicate archive member")
    OUT.mkdir(parents=True, exist_ok=True)
    output = OUT / archive_name
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data, mode in sorted(members):
            if not safe(name) and not name.startswith("handoff-"):
                fail(f"unsafe handoff member: {name}")
            info = zipfile.ZipInfo(name)
            info.date_time = (1980, 1, 1, 0, 0, 0)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (stat.S_IFREG | mode) << 16
            archive.writestr(info, data)
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    sidecar = output.with_suffix(output.suffix + ".sha256")
    sidecar.write_text(f"{digest}  {output.name}\n")
    print(output)
    print(sidecar)
    print(digest)


if __name__ == "__main__":
    main()
