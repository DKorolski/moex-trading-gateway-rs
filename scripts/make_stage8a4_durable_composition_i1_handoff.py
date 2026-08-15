#!/usr/bin/env python3
"""Build a commit-bound Stage 8A-4 I1 review archive."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a4_durable_composition_i1_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports" / "handoff"


def run(*args: str, cwd: Path = ROOT) -> bytes:
    return subprocess.check_output(args, cwd=cwd)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    return info


def main() -> None:
    status = run("git", "status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit("stage8a4-durable-composition-i1-handoff: FAIL worktree must be clean")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != "stage8a4-durable-composition-i1":
        raise SystemExit(f"stage8a4-durable-composition-i1-handoff: FAIL branch={branch}")

    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    with tempfile.TemporaryDirectory(prefix="stage8a4-i1-handoff-") as raw:
        temporary = Path(raw)
        gate_dir = temporary / "gate"
        environment = os.environ.copy()
        environment["STAGE8A4_I1_ARTIFACT_DIR"] = str(gate_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a4_durable_composition_i1_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        marker = b"stage8a4-durable-composition-i1-gate: PASS rows=40 negatives=20 goldens=20 focused=12 writer=false apply=false execution=false"
        if marker not in gate.stdout:
            raise SystemExit("stage8a4-durable-composition-i1-handoff: FAIL gate marker missing")

        tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()
        manifest_entries = []
        for name in tracked:
            data = run("git", "show", f"HEAD:{name}")
            manifest_entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
        manifest = (json.dumps({
            "schema_version": 1,
            "source_ref": full_ref,
            "entry_count": len(manifest_entries),
            "entries": manifest_entries,
        }, indent=2, sort_keys=True) + "\n").encode()

        gate_files: dict[str, bytes] = {}
        for path in sorted(gate_dir.iterdir()):
            if path.is_file():
                gate_files[f"handoff-evidence/gate-artifacts/{path.name}"] = path.read_bytes()

        evidence = (json.dumps({
            "schema_version": 1,
            "stage": "8A-4-durable-composition-I1",
            "source_ref": full_ref,
            "source_short_ref": short_ref,
            "archive_name": archive_name,
            "branch": branch,
            "accepted_spec_r2_ref": "dd01253596527d6cff1db11cc32ae3c3348c96a0",
            "accepted_spec_r2_review_sha256": "acb8364ee2100bf64e50522823b1da21093f96c73f93b20b4cdf9e7ac09b58ec",
            "source_tree_manifest_sha256": sha256(manifest),
            "full_gate_sha256": sha256(gate.stdout),
            "gate_artifact_sha256": {name: sha256(data) for name, data in gate_files.items()},
            "v2_writer_enabled": False,
            "durable_apply_enabled": False,
            "redis_live_enabled": False,
            "finam_post_delete_enabled": False,
            "broker_dispatch_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
        }, indent=2, sort_keys=True) + "\n").encode()

        source_zip = temporary / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": (
                f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n"
            ).encode(),
            "handoff-evidence/stage8a4-durable-composition-i1-full-gate.txt": gate.stdout,
            "handoff-evidence/stage8a4-durable-composition-i1-evidence.json": evidence,
            "handoff-evidence/source-tree-manifest.json": manifest,
            **gate_files,
        }
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for info in source.infolist():
                target.writestr(info, source.read(info.filename))
            for name, data in sorted(additions.items()):
                target.writestr(zip_info(name), data)

    result = safety.check(str(archive_path))
    archive_hash = sha256(archive_path.read_bytes())
    (archive_path.with_suffix(archive_path.suffix + ".sha256")).write_text(
        f"{archive_hash}  {archive_name}\n", encoding="utf-8"
    )
    (archive_path.with_suffix(archive_path.suffix + ".safety.json")).write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"archive={archive_path}")
    print(f"sha256={archive_hash}")
    print(f"source_ref={full_ref}")
    print("stage8a4-durable-composition-i1-handoff: PASS")


if __name__ == "__main__":
    main()
