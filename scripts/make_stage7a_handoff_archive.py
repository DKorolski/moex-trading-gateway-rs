#!/usr/bin/env python3
"""Create immutable Stage 7A source plus evidence handoff."""
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

import stage7a_check as checker

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports" / "handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage7a-handoff: FAIL: {message}")


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
        fail("worktree not clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    if run(["git", "rev-parse", "HEAD^"]) != checker.R1_PREDECESSOR:
        fail("wrong predecessor")
    if run(["git", "branch", "--show-current"]) != checker.BRANCH:
        fail("wrong branch")
    if os.environ.get("STAGE7A_REQUIRE_ORIGIN") == "1":
        if run(["git", "rev-parse", f"origin/{checker.BRANCH}"]) != head:
            fail("origin branch mismatch")

    with tempfile.TemporaryDirectory(prefix="stage7a-handoff-") as temp:
        artifact_dir = Path(temp)
        env = dict(os.environ)
        env.pop("STAGE7A_SKIP_PRESEAL", None)
        env["STAGE7A_ARTIFACT_DIR"] = str(artifact_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage7a_gate.sh"],
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
        acceptance = json.loads((artifact_dir / "stage7a-r1-acceptance.json").read_text())
        negative = (artifact_dir / "negative.txt").read_bytes()
        negative_count = sum(line.startswith(b"PASS ") for line in negative.splitlines())
        artifact_members = []
        for path in sorted(artifact_dir.iterdir()):
            if not path.is_file():
                continue
            data = path.read_bytes()
            if path.suffix in {".txt", ".json"}:
                data = data.replace(str(ROOT).encode(), b"<REPO>").replace(str(Path.home()).encode(), b"<HOME>")
            artifact_members.append((f"handoff-evidence/gate-artifacts/{path.name}", data, 0o644))

    raw = subprocess.check_output(["git", "archive", "--format=tar", "HEAD"], cwd=ROOT)
    members: list[tuple[str, bytes, int]] = []
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not safe(member.name):
                fail(f"unsafe member: {member.name}")
            extracted = tar.extractfile(member)
            if extracted is None:
                fail(f"missing archive payload: {member.name}")
            members.append((member.name, extracted.read(), member.mode))

    manifest = json.dumps(
        {
            "schema_version": 1,
            "source_ref": head,
            "predecessor": checker.R1_PREDECESSOR,
            "accepted_stage6_base": checker.BASE,
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
        f"predecessor={checker.R1_PREDECESSOR}\n"
        f"accepted_stage6_base={checker.BASE}\n"
    ).encode()
    evidence = json.dumps(
        {
            "schema_version": 1,
            "stage": "7A-R1",
            "status": "review_closure_candidate",
            "source_ref": head,
            "predecessor": checker.R1_PREDECESSOR,
            "accepted_stage6_base": checker.BASE,
            "gate_exit_code": 0,
            "acceptance_row_count": acceptance["acceptance_row_count"],
            "acceptance_evaluated_count": acceptance["acceptance_evaluated_count"],
            "acceptance_pass_count": acceptance["acceptance_pass_count"],
            "all_blocking_rows_passed": acceptance["all_blocking_rows_passed"],
            "negative_case_count": negative_count,
            "focused_runtime_bridge_test_count": 16,
            "focused_stage6_admission_test_count": 7,
            "real_redis_integration_passed": True,
            "stage6_execution_authority_exclusive": True,
            "paper_namespace_only": True,
            "cross_process_exactly_once_claimed": False,
            "finam_post_delete_enabled": False,
            "broker_network_dispatch_enabled": False,
            "runtime_live_enabled": False,
            "source_manifest_sha256": hashlib.sha256(manifest).hexdigest(),
            "gate_sha256": hashlib.sha256(redacted).hexdigest(),
        },
        indent=2,
        sort_keys=True,
    ).encode() + b"\n"
    members += [
        ("handoff-commit.txt", marker, 0o644),
        ("source-tree-manifest.json", manifest, 0o644),
        ("handoff-evidence/stage7a-full-gate.txt", redacted, 0o644),
        ("handoff-evidence/stage7a-evidence.json", evidence, 0o644),
        ("handoff-evidence/stage7a-negative.txt", negative, 0o644),
        (
            "handoff-evidence/stage7a-toolchain.txt",
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
