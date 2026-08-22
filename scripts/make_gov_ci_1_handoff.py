#!/usr/bin/env python3
"""Create a commit-bound GOV-CI-1B handoff with complete mandatory logs."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path
from typing import Any, Optional

import gov_ci_1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "gov-ci-1b"
ACCEPTED_PREDECESSOR = "1dea519cbf2affc3d99866fdae66bbddbafefa24"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def generated_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    return info


def tracked_info(name: str, mode: str) -> zipfile.ZipInfo:
    """Create a deterministic Unix ZIP member from the Git tree mode."""
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = int(mode, 8) << 16
    return info


def execute(
    command_id: str,
    command: list[str],
    log_dir: Path,
    env: Optional[dict[str, str]] = None,
) -> dict[str, Any]:
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    log_path = log_dir / f"{command_id}.txt"
    log_path.write_bytes(result.stdout)
    if result.returncode != 0:
        raise SystemExit(
            f"gov-ci-1b-handoff: FAIL command={command_id} exit={result.returncode}\n"
            + result.stdout.decode(errors="replace")
        )
    archive_path = f"handoff-evidence/logs/{log_path.name}"
    return {
        "id": command_id,
        "command": command,
        "exit_code": result.returncode,
        "log_path": archive_path,
        "log_sha256": sha256(result.stdout),
        "log_size": len(result.stdout),
    }


def tracked_manifest(source_ref: str) -> bytes:
    raw = run("git", "ls-tree", "-rz", "--full-tree", source_ref)
    entries: list[dict[str, Any]] = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, name_raw = record.split(b"\t", 1)
        mode_raw, object_type, _object_id = metadata.split(b" ", 2)
        if object_type != b"blob":
            raise SystemExit(f"gov-ci-1b-handoff: FAIL non-blob tracked object {name_raw!r}")
        name = name_raw.decode("utf-8")
        data = run("git", "show", f"{source_ref}:{name}")
        entries.append(
            {
                "mode": mode_raw.decode(),
                "path": name,
                "sha256": sha256(data),
                "size": len(data),
            }
        )
    return (
        json.dumps(
            {
                "schema_version": 2,
                "source_ref": source_ref,
                "entry_count": len(entries),
                "entries": entries,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("gov-ci-1b-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"gov-ci-1b-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    upstream_ref = run("git", "rev-parse", "@{upstream}").decode().strip()
    if upstream_ref != full_ref:
        raise SystemExit("gov-ci-1b-handoff: FAIL exact commit is not pushed to upstream")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    with tempfile.TemporaryDirectory(prefix="gov-ci-1b-evidence-") as raw_temp:
        temp = Path(raw_temp)
        log_dir = temp / "logs"
        gate_artifact_dir = temp / "current-tree-gate-artifacts"
        log_dir.mkdir()
        environment = os.environ.copy()
        environment["CURRENT_TREE_CI_ARTIFACT_DIR"] = str(gate_artifact_dir)
        commands = [
            execute("current-tree-authority-gate", ["bash", "scripts/current_tree_ci_gate.sh"], log_dir, environment),
            execute("cargo-fmt", ["cargo", "fmt", "--all", "--check"], log_dir),
            execute("cargo-debug", ["cargo", "test", "--workspace", "--all-targets", "--", "--test-threads=1"], log_dir),
            execute("cargo-release", ["cargo", "test", "--workspace", "--release", "--all-targets", "--", "--test-threads=1"], log_dir),
            execute("cargo-doc", ["cargo", "test", "--workspace", "--doc"], log_dir),
            execute("cargo-clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"], log_dir),
            execute("no-redis-smoke", ["bash", "scripts/test_m4_3x_evidence_no_redis.sh"], log_dir),
            execute("redis-shadow-smoke", ["bash", "scripts/redis_shadow_smoke.sh"], log_dir),
            execute("runtime-bridge-dry-smoke", ["bash", "scripts/runtime_bridge_dry_smoke.sh"], log_dir),
            execute("git-diff-check", ["git", "diff", "--check", "HEAD"], log_dir),
        ]

        artifact_additions: dict[str, bytes] = {}
        artifact_rows: list[dict[str, Any]] = []
        for path in sorted(gate_artifact_dir.rglob("*")):
            if not path.is_file():
                continue
            relative = path.relative_to(gate_artifact_dir).as_posix()
            archive_artifact = f"handoff-evidence/current-tree-gate-artifacts/{relative}"
            data = path.read_bytes()
            artifact_additions[archive_artifact] = data
            artifact_rows.append(
                {"path": archive_artifact, "sha256": sha256(data), "size": len(data)}
            )

        command_document = (
            json.dumps({"schema_version": 1, "source_ref": full_ref, "commands": commands}, indent=2, sort_keys=True)
            + "\n"
        ).encode()
        manifest = tracked_manifest(full_ref)
        authority = json.loads((ROOT / "docs/stage-8/gov-ci-1-authority.json").read_text(encoding="utf-8"))
        evidence = (
            json.dumps(
                {
                    "schema_version": 2,
                    "stage": "GOV-CI-1B",
                    "source_ref": full_ref,
                    "source_short_ref": short_ref,
                    "archive_name": archive_name,
                    "branch": branch,
                    "accepted_predecessor": ACCEPTED_PREDECESSOR,
                    "accepted_stage8a5_ref": authority["accepted_stage8a5_replay"]["source_ref"],
                    "acceptance_rows": 30,
                    "negative_cases": 27,
                    "inherited_stage8_negative_cases": 544,
                    "stage8a5_negative_cases": 20,
                    "stage8a5_forbidden_negative_cases": 10,
                    "current_i4_negative_cases": 28,
                    "production_code_manifest_sha256": authority["production_code_manifest"]["aggregate_sha256"],
                    "governance_control_plane_manifest_sha256": authority["governance_control_plane_manifest"]["aggregate_sha256"],
                    "source_tree_manifest_sha256": sha256(manifest),
                    "commands_sha256": sha256(command_document),
                    "gate_artifacts": artifact_rows,
                    "governance_only": True,
                    "stage8b_s_authorized": False,
                    "finam_post_delete": False,
                    "broker_execution": False,
                    "redis_live_consumer": False,
                    "redis_xadd_xack": False,
                    "runtime_live": False,
                    "real_orders": False,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        ).encode()

        source_zip = temp / "source.zip"
        subprocess.run(
            ["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"],
            cwd=ROOT,
            check=True,
        )
        additions = {
            "handoff-commit.txt": (
                f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n"
            ).encode(),
            safety.EVIDENCE: evidence,
            safety.MANIFEST: manifest,
            safety.COMMANDS: command_document,
        }
        additions.update(
            {
                row["log_path"]: (log_dir / f"{row['id']}.txt").read_bytes()
                for row in commands
            }
        )
        additions.update(artifact_additions)
        manifest_entries = json.loads(manifest)["entries"]
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as target:
            for entry in manifest_entries:
                name = entry["path"]
                target.writestr(tracked_info(name, entry["mode"]), source.read(name))
            for name, data in sorted(additions.items()):
                target.writestr(generated_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={full_ref}\ngov-ci-1b-handoff: PASS")


if __name__ == "__main__":
    main()
