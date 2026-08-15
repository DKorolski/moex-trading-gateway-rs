#!/usr/bin/env python3
"""Create an immutable, commit-bound Stage 8A-1 review handoff."""

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

import stage8a1_check as checker
import stage8a1_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "reports/handoff"
GATE_ARTIFACTS = tuple(sorted(safety.ARTIFACTS))


def run(args: list[str]) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def fail(message: str) -> None:
    raise SystemExit(f"stage8a1-handoff: FAIL {message}")


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
    return data.replace(str(ROOT).encode(), b"<REPO>").replace(
        str(Path.home()).encode(), b"<HOME>"
    )


def main() -> None:
    if run(["git", "status", "--porcelain"]):
        fail("worktree must be clean")
    head = run(["git", "rev-parse", "HEAD"])
    short = run(["git", "rev-parse", "--short=7", "HEAD"])
    branch = run(["git", "branch", "--show-current"])
    if branch != checker.BRANCH:
        fail("wrong branch")
    subprocess.run(["git", "merge-base", "--is-ancestor", checker.BASE, "HEAD"], cwd=ROOT, check=True)
    if run(["git", "rev-parse", f"origin/{branch}"]) != head:
        fail("origin branch must equal HEAD")

    with tempfile.TemporaryDirectory(prefix="stage8a1-handoff-") as raw_dir:
        artifact_dir = Path(raw_dir) / "gate"
        environment = os.environ.copy()
        environment["STAGE8A1_ARTIFACT_DIR"] = str(artifact_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a1_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        gate_output = redacted(gate.stdout)
        if gate.returncode:
            print(gate_output.decode(errors="replace"), end="")
            fail(f"full gate failed: {gate.returncode}")
        marker = b"stage8a1-r2-gate: PASS rows=68 negatives=62 production-issuers=true dispatch-ready=true no-send=true next=8A-2-pending"
        if marker not in gate_output:
            fail("full gate completion marker missing")
        artifact_payloads: dict[str, bytes] = {}
        for name in GATE_ARTIFACTS:
            path = artifact_dir / name
            if not path.is_file() or not path.read_bytes():
                fail(f"missing/empty gate artifact: {name}")
            artifact_payloads[name] = redacted(path.read_bytes())
        artifact_hashes = {
            name: hashlib.sha256(payload).hexdigest()
            for name, payload in sorted(artifact_payloads.items())
        }

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
                "stage8a1_r1_base_ref": checker.BASE,
                "accepted_stage8a0_ref": checker.STAGE8A0,
                "members": [
                    {"path": name, "sha256": hashlib.sha256(data).hexdigest()}
                    for name, data, _ in sorted(source_members)
                ],
            },
            indent=2,
            sort_keys=True,
        ).encode() + b"\n"
        archive_name = f"moex-trading-project-{short}.zip"
        commit_marker = (
            f"source_ref={head}\nsource_short_ref={short}\nsource_branch={branch}\n"
            f"archive_name={archive_name}\nstage8a1_r1_base_ref={checker.BASE}\n"
            f"accepted_stage8a0_ref={checker.STAGE8A0}\n"
            "candidate_stage=Stage 8A-1 R2\ncandidate_status=independent_acceptance_pending\n"
            "next_after_acceptance=Stage 8A-2 only\nfinam_post_delete_authorized=false\n"
            "broker_dispatch_authorized=false\nruntime_live_authorized=false\n"
            "real_orders_authorized=false\n"
        ).encode()
        preseal = json.dumps(
            {
                "schema_version": 1,
                "result": "PASS",
                "source_members_checked": len(source_members),
                "duplicates": 0,
                "unsafe_paths": 0,
                "symlinks": 0,
                "special_files": 0,
                "secrets_included": False,
            },
            indent=2,
            sort_keys=True,
        ).encode() + b"\n"
        evidence = json.dumps(
            {
                "schema_version": 1,
                "stage": "8A-1-R2",
                "candidate_status": "independent_acceptance_pending",
                "source_ref": head,
                "source_branch": branch,
                "stage8a1_r1_base_ref": checker.BASE,
                "stage8a1_r1_review_sha256": checker.R1_REVIEW_SHA,
                "accepted_stage8a0_ref": checker.STAGE8A0,
                "accepted_stage8a0_review_sha256": checker.STAGE8A0_REVIEW_SHA,
                "acceptance_rows": 68,
                "negative_cases": 62,
                "opaque_linear_capability": True,
                "operational_authority_continuity": True,
                "production_authority_issuers": True,
                "dispatch_ready_durable_authority": True,
                "post_mint_revalidation": True,
                "request_extraction_available": False,
                "serializer_composition_available": False,
                "transport_available": False,
                "finam_post_delete_authorized": False,
                "broker_dispatch_authorized": False,
                "runtime_live_authorized": False,
                "real_orders_authorized": False,
                "all_required_gates_passed": True,
                "source_manifest_sha256": hashlib.sha256(source_manifest).hexdigest(),
                "full_gate_sha256": hashlib.sha256(gate_output).hexdigest(),
                "gate_artifact_sha256": artifact_hashes,
                "next_after_independent_acceptance": "Stage 8A-2 only",
            },
            indent=2,
            sort_keys=True,
        ).encode() + b"\n"

        members = list(source_members)
        members.extend(
            [
                ("handoff-commit.txt", commit_marker, 0o644),
                ("source-tree-manifest.json", source_manifest, 0o644),
                ("handoff-evidence/stage8a1-full-gate.txt", gate_output, 0o644),
                ("handoff-evidence/stage8a1-evidence.json", evidence, 0o644),
                ("handoff-evidence/stage8a1-preseal-safety.json", preseal, 0o644),
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
                info.external_attr = (stat.S_IFREG | (0o755 if mode & 0o111 else 0o644)) << 16
                target.writestr(info, data, zipfile.ZIP_DEFLATED, compresslevel=9)

        digest = hashlib.sha256(destination.read_bytes()).hexdigest()
        sidecar = destination.with_suffix(".zip.sha256")
        sidecar.write_text(f"{digest}  {destination.name}\n")
        safety_result = safety.verify(destination)
        safety_path = destination.with_suffix(".zip.safety.json")
        safety_path.write_text(json.dumps(safety_result, indent=2, sort_keys=True) + "\n")
        os.chmod(destination, 0o644)
        os.chmod(sidecar, 0o644)
        os.chmod(safety_path, 0o644)
        print(destination)
        print(sidecar)
        print(safety_path)
        print(digest)


if __name__ == "__main__":
    main()
