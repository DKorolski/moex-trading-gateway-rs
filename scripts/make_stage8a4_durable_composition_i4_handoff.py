#!/usr/bin/env python3
"""Build a commit-bound Stage 8A-4 I4 review archive."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a4_durable_composition_i4_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports" / "handoff"


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
        raise SystemExit("stage8a4-i4-handoff: FAIL worktree must be clean")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != "stage8a4-durable-composition-i4":
        raise SystemExit(f"stage8a4-i4-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name
    with tempfile.TemporaryDirectory(prefix="stage8a4-i4-handoff-") as temp_name:
        temp = Path(temp_name)
        gate_dir = temp / "gate"
        env = os.environ.copy()
        env["STAGE8A4_I4_ARTIFACT_DIR"] = str(gate_dir)
        gate = subprocess.run(["bash", "scripts/stage8a4_durable_composition_i4_gate.sh"], cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
        if gate.returncode != 0 or b"stage8a4-durable-composition-i4-gate: PASS" not in gate.stdout:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        entries = []
        for name in run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines():
            data = run("git", "show", f"HEAD:{name}")
            entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
        manifest = (json.dumps({"schema_version": 1, "source_ref": full_ref, "entry_count": len(entries), "entries": entries}, indent=2, sort_keys=True) + "\n").encode()
        evidence = (json.dumps({
            "schema_version": 1,
            "stage": "8A-4-durable-composition-I4",
            "source_ref": full_ref,
            "source_short_ref": short_ref,
            "branch": branch,
            "archive_name": archive_name,
            "accepted_design_ref": "81727aae1f648f17961177fc9541e2483cbf07f2",
            "acceptance_rows": 60,
            "negative_cases": 28,
            "accepted_design_traceability_rows": 64,
            "inherited_design_negative_cases": 46,
            "source_tree_manifest_sha256": sha256(manifest),
            "full_gate_sha256": sha256(gate.stdout),
            "read_only_no_effect": True,
            "terminal_authority_public_opaque": True,
            "ack_timestamp_free": True,
            "current_readiness_independent": True,
            "seal_mutation": False,
            "ack_readiness_publication_enabled": False,
            "redis_mutation_enabled": False,
            "finam_post_delete_enabled": False,
            "runtime_live_enabled": False,
            "real_orders_enabled": False,
        }, indent=2, sort_keys=True) + "\n").encode()
        source_zip = temp / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
            "handoff-evidence/stage8a4-i4-full-gate.txt": gate.stdout,
            "handoff-evidence/stage8a4-i4-evidence.json": evidence,
            "handoff-evidence/source-tree-manifest.json": manifest,
        }
        additions.update({f"handoff-evidence/gate-artifacts/{path.name}": path.read_bytes() for path in sorted(gate_dir.iterdir()) if path.is_file()})
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for member in source.infolist():
                target.writestr(member, source.read(member.filename))
            for name, data in sorted(additions.items()):
                target.writestr(info(name), data)
    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"archive={archive_path}")
    print(f"sha256={digest}")
    print(f"source_ref={full_ref}")
    print("stage8a4-i4-handoff: PASS")


if __name__ == "__main__":
    main()
