#!/usr/bin/env python3
"""Create the immutable Stage 8A-4 design R1 handoff."""

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

import stage8a4_design_check as checker
import stage8a4_design_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage8a4-design-handoff: FAIL {message}")


def safe(name: str) -> bool:
    path = PurePosixPath(name)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and not any(part in {".git", "target", "tmp", "reports", "__MACOSX"} for part in path.parts)
        and path.name != ".env"
        and path.suffix != ".log"
    )


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def redacted(data: bytes) -> bytes:
    return data.replace(str(ROOT).encode(), b"<REPO>").replace(str(Path.home()).encode(), b"<HOME>")


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    branch = run(["git", "branch", "--show-current"])
    if branch != checker.BRANCH:
        fail("wrong branch")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    if run(["git", "rev-parse", f"origin/{branch}"]) != head:
        fail("origin branch must equal HEAD")
    checker.check(ROOT, git_scope=True)

    with tempfile.TemporaryDirectory(prefix="stage8a4-design-handoff-") as raw_dir:
        artifact_dir = Path(raw_dir) / "gate"
        environment = os.environ.copy()
        environment["STAGE8A4_DESIGN_ARTIFACT_DIR"] = str(artifact_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a4_design_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        gate_output = redacted(gate.stdout)
        marker = (
            b"stage8a4-design-r1-gate: PASS rows=72 negatives=48 design-only=true "
            b"next=8A-4-implementation-r1-pending"
        )
        if gate.returncode or marker not in gate_output:
            print(gate_output.decode(errors="replace"), end="")
            fail(f"full gate failed: {gate.returncode}")

        artifact_payloads: dict[str, bytes] = {}
        for path in sorted(artifact_dir.iterdir()):
            if path.is_file() and path.read_bytes():
                artifact_payloads[path.name] = redacted(path.read_bytes())
        required = {"stage8a4-design-gate-summary.json", "proof-map.stdout.txt"}
        if not required <= artifact_payloads.keys():
            fail("required gate artifacts missing")

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
                "accepted_stage8a3_ref": checker.BASE,
                "members": [
                    {"path": name, "sha256": digest(data)}
                    for name, data, _ in sorted(source_members)
                ],
            },
            indent=2,
            sort_keys=True,
        ).encode() + b"\n"
        archive_name = f"moex-trading-project-{short}.zip"
        commit_marker = (
            f"source_ref={head}\nsource_short_ref={short}\nsource_branch={branch}\n"
            f"archive_name={archive_name}\naccepted_stage8a3_ref={checker.BASE}\n"
            "candidate_stage=Stage 8A-4 design R1\n"
            "candidate_status=independent_acceptance_pending\n"
            "next_after_acceptance=Stage 8A-4 implementation R1 only\n"
            "reconciliation_implemented=false\nproven_no_match_available=false\n"
            "network_send_authorized=false\nredis_live_authorized=false\n"
            "broker_dispatch_authorized=false\nruntime_live_authorized=false\n"
            "real_orders_authorized=false\n"
        ).encode()
        artifact_hashes = {name: digest(payload) for name, payload in sorted(artifact_payloads.items())}
        evidence = json.dumps(
            {
                "schema_version": 1,
                "stage": "8A-4-design-R1",
                "candidate_status": "independent_acceptance_pending",
                "source_ref": head,
                "source_branch": branch,
                "accepted_stage8a3_ref": checker.BASE,
                "accepted_stage8a3_review_sha256":
                    "2e969db40bd847230f4df426ce3ee235f2f2273b87a778297b4588bf1f127232",
                "acceptance_rows": 72,
                "negative_cases": 48,
                "reconciliation_implemented": False,
                "proven_no_match_available": False,
                "network_send_authorized": False,
                "redis_live_authorized": False,
                "broker_dispatch_authorized": False,
                "runtime_live_authorized": False,
                "real_orders_authorized": False,
                "all_required_gates_passed": True,
                "source_manifest_sha256": digest(source_manifest),
                "full_gate_sha256": digest(gate_output),
                "gate_artifact_sha256": artifact_hashes,
                "next_after_independent_acceptance": "Stage 8A-4 implementation R1 only",
            },
            indent=2,
            sort_keys=True,
        ).encode() + b"\n"

        members = list(source_members)
        members.extend(
            [
                ("handoff-commit.txt", commit_marker, 0o644),
                ("source-tree-manifest.json", source_manifest, 0o644),
                ("handoff-evidence/stage8a4-design-full-gate.txt", gate_output, 0o644),
                ("handoff-evidence/stage8a4-design-evidence.json", evidence, 0o644),
            ]
        )
        members.extend(
            (f"handoff-evidence/gate-artifacts/{name}", payload, 0o644)
            for name, payload in sorted(artifact_payloads.items())
        )
        if len({name for name, _, _ in members}) != len(members):
            fail("duplicate archive member")

        OUT.mkdir(parents=True, exist_ok=True)
        destination = OUT / archive_name
        with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for name, data, mode in sorted(members):
                info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                permissions = 0o755 if mode & 0o111 else 0o644
                info.external_attr = (stat.S_IFREG | permissions) << 16
                target.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)

        archive_sha256 = digest(destination.read_bytes())
        sidecar = destination.with_suffix(".zip.sha256")
        sidecar.write_text(f"{archive_sha256}  {destination.name}\n")
        safety_result = safety.verify(str(destination))
        safety_path = destination.with_suffix(".zip.safety.json")
        safety_path.write_text(json.dumps(safety_result, indent=2, sort_keys=True) + "\n")
        print(destination)
        print(sidecar)
        print(safety_path)
        print(archive_sha256)


if __name__ == "__main__":
    main()
