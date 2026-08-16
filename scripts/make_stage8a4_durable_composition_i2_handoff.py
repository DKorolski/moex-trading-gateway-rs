#!/usr/bin/env python3
"""Build a commit-bound Stage 8A-4 I2 review archive."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a4_durable_composition_i2_handoff_safety_check as safety

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
        raise SystemExit("stage8a4-durable-composition-i2-handoff: FAIL worktree must be clean")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != "stage8a4-durable-composition-i2":
        raise SystemExit(f"stage8a4-durable-composition-i2-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    with tempfile.TemporaryDirectory(prefix="stage8a4-i2-handoff-") as raw:
        temporary = Path(raw)
        gate_dir = temporary / "gate"
        environment = os.environ.copy()
        environment["STAGE8A4_I2_ARTIFACT_DIR"] = str(gate_dir)
        gate = subprocess.run(
            ["bash", "scripts/stage8a4_durable_composition_i2_gate.sh"],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        marker = b"stage8a4-durable-composition-i2-gate: PASS rows=48 negatives=28 focused=13 append=false execution=false"
        if marker not in gate.stdout:
            raise SystemExit("stage8a4-durable-composition-i2-handoff: FAIL gate marker missing")

        tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()
        entries = []
        for name in tracked:
            data = run("git", "show", f"HEAD:{name}")
            entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
        manifest = (json.dumps({
            "schema_version": 1,
            "source_ref": full_ref,
            "entry_count": len(entries),
            "entries": entries,
        }, indent=2, sort_keys=True) + "\n").encode()
        gate_files = {
            f"handoff-evidence/gate-artifacts/{path.name}": path.read_bytes()
            for path in sorted(gate_dir.iterdir()) if path.is_file()
        }
        evidence = (json.dumps({
            "schema_version": 1,
            "stage": "8A-4-durable-composition-I2",
            "source_ref": full_ref,
            "source_short_ref": short_ref,
            "archive_name": archive_name,
            "branch": branch,
            "accepted_i1_r2_ref": "113d2827ef255e8d2c2597a3acb38fe52dd7e52d",
            "accepted_i1_r2_review_sha256": "5ef7d0fcc645874a8d9bce7e2d2bb3004f06b038c81b0bf5496582464cb1b9e7",
            "focused_tests": 13,
            "negative_cases": 28,
            "source_tree_manifest_sha256": sha256(manifest),
            "full_gate_sha256": sha256(gate.stdout),
            "durable_append_enabled": False,
            "cas_enabled": False,
            "covering_seal_writer_enabled": False,
            "ack_readiness_enabled": False,
            "redis_live_enabled": False,
            "finam_post_delete_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
        }, indent=2, sort_keys=True) + "\n").encode()
        source_zip = temporary / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
            "handoff-evidence/stage8a4-durable-composition-i2-full-gate.txt": gate.stdout,
            "handoff-evidence/stage8a4-durable-composition-i2-evidence.json": evidence,
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
    archive_path.with_suffix(archive_path.suffix + ".sha256").write_text(
        f"{archive_hash}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(archive_path.suffix + ".safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"archive={archive_path}")
    print(f"sha256={archive_hash}")
    print(f"source_ref={full_ref}")
    print("stage8a4-durable-composition-i2-handoff: PASS")


if __name__ == "__main__":
    main()
