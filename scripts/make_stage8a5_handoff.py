#!/usr/bin/env python3
"""Build a commit-bound Stage 8A-5 aggregate review archive."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a5_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports" / "handoff"
BRANCH = "stage8a5-aggregate-acceptance"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def info(name: str) -> zipfile.ZipInfo:
    value = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    value.compress_type = zipfile.ZIP_DEFLATED
    value.external_attr = 0o100644 << 16
    return value


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8a5-handoff: FAIL worktree must be clean")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8a5-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name
    with tempfile.TemporaryDirectory(prefix="stage8a5-handoff-") as temp_name:
        temp = Path(temp_name)
        # A stable ignored path preserves hermetic per-checkout Cargo caches
        # across the preseal and commit-bound rerun without putting build
        # products into the handoff archive.
        gate_dir = ROOT / "tmp" / "stage8a5-full-gate"
        env = os.environ.copy()
        env["STAGE8A5_ARTIFACT_DIR"] = str(gate_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a5_gate.sh"],
            cwd=ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0 or b"stage8a5-gate: PASS" not in gate.stdout:
            raise SystemExit(gate.stdout.decode(errors="replace"))

        entries = []
        for name in run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines():
            data = run("git", "show", f"HEAD:{name}")
            entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
        source_manifest = (
            json.dumps(
                {"schema_version": 1, "source_ref": full_ref, "entry_count": len(entries), "entries": entries},
                indent=2,
                sort_keys=True,
            ) + "\n"
        ).encode()

        artifact_entries = []
        for path in sorted(item for item in gate_dir.rglob("*") if item.is_file()):
            data = path.read_bytes()
            artifact_entries.append({
                "path": path.relative_to(gate_dir).as_posix(),
                "sha256": sha256(data),
                "size": len(data),
            })
        artifact_manifest = (
            json.dumps(
                {"schema_version": 1, "source_ref": full_ref, "entry_count": len(artifact_entries), "entries": artifact_entries},
                indent=2,
                sort_keys=True,
            ) + "\n"
        ).encode()

        aggregate_result = json.loads((gate_dir / "stage8a5-aggregate-acceptance-result.json").read_text())
        if aggregate_result.get("source_ref") != full_ref or aggregate_result.get("result") != "PASS":
            raise SystemExit("stage8a5-handoff: FAIL gate result is not commit-bound")
        evidence = (
            json.dumps(
                {
                    "schema_version": 1,
                    "stage": "8A-5-aggregate-acceptance",
                    "status": "independent_acceptance_pending",
                    "source_ref": full_ref,
                    "source_short_ref": short_ref,
                    "branch": branch,
                    "archive_name": archive_name,
                    "accepted_predecessor": "4a11688c941ee240e377b384042c4bca837b040f",
                    "acceptance_rows": 30,
                    "negative_cases": 20,
                    "inherited_stage8_negative_cases": 544,
                    "current_i4_negative_cases": 28,
                    "aggregate_only": True,
                    "inherited_stage7b_gate_passed": True,
                    "workspace_debug_release_passed": True,
                    "source_tree_manifest_sha256": sha256(source_manifest),
                    "gate_artifact_manifest_sha256": sha256(artifact_manifest),
                    "full_gate_sha256": sha256(gate.stdout),
                    "production_rust_changed": False,
                    "cargo_or_lock_changed": False,
                    "workflow_changed": False,
                    "stage8b_authorized": False,
                    "redis_live_consumer_enabled": False,
                    "finam_post_delete_enabled": False,
                    "broker_dispatch_enabled": False,
                    "runtime_live_enabled": False,
                    "real_orders_enabled": False,
                },
                indent=2,
                sort_keys=True,
            ) + "\n"
        ).encode()
        source_zip = temp / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
            "handoff-evidence/stage8a5-full-gate.txt": gate.stdout,
            "handoff-evidence/stage8a5-evidence.json": evidence,
            "handoff-evidence/source-tree-manifest.json": source_manifest,
            "handoff-evidence/gate-artifact-manifest.json": artifact_manifest,
        }
        for name in (
            "stage8a5-aggregate-acceptance-result.json",
            "stage8a5-inherited-stage8-result.json",
            "aggregate-semantic.txt",
            "aggregate-negative.txt",
            "forbidden-surface.txt",
            "forbidden-surface-negative.txt",
            "inherited-stage8.txt",
            "inherited-stage7b-gate.txt",
            "workspace-debug.txt",
            "workspace-release.txt",
            "workspace-doc.txt",
            "workspace-clippy.txt",
            "i3-external-compile.txt",
            "i4-external-compile.txt",
        ):
            path = gate_dir / name
            if path.is_file():
                additions[f"handoff-evidence/gate-artifacts/{name}"] = path.read_bytes()
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as target:
            for member in source.infolist():
                target.writestr(member, source.read(member.filename))
            for name, data in sorted(additions.items()):
                target.writestr(info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"archive={archive_path}")
    print(f"sha256={digest}")
    print(f"source_ref={full_ref}")
    print("stage8a5-handoff: PASS")


if __name__ == "__main__":
    main()
