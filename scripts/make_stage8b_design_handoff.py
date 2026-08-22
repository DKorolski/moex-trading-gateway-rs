#!/usr/bin/env python3
"""Create a deterministic commit-bound Stage 8B-D R2 review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path
from typing import Any

import stage8b_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-d-r2"
BASE = "7bc9fdab190e011111b15ebdf2f35ff2263a8e34"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def zip_info(name: str, mode: str = "100644") -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = int(mode, 8) << 16
    return info


def source_manifest(source_ref: str) -> tuple[bytes, list[dict[str, Any]]]:
    raw = run("git", "ls-tree", "-rz", "--full-tree", source_ref)
    entries: list[dict[str, Any]] = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, name_raw = record.split(b"\t", 1)
        mode_raw, object_type, _object_id = metadata.split(b" ", 2)
        if object_type != b"blob":
            raise SystemExit(f"stage8b-design-handoff: FAIL non-blob {name_raw!r}")
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
    document = (
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
    return document, entries


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-design-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    upstream_ref = run("git", "rev-parse", "@{upstream}").decode().strip()
    if full_ref != upstream_ref:
        raise SystemExit("stage8b-design-handoff: FAIL exact commit not pushed upstream")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    OUTPUT.mkdir(parents=True, exist_ok=True)

    gate = subprocess.run(
        ["bash", "scripts/stage8b_design_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    gate_marker = (
        b"stage8b-design-gate: PASS rows=70 negatives=50 design_only=true "
        b"implementation=false execution=false finam=false redis=false dispatch=false "
        b"live=false stage8b_s=false stage12=false"
    )
    if gate.returncode != 0 or gate_marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    manifest, entries = source_manifest(full_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 2,
                "stage": "8B-D-R2",
                "source_ref": full_ref,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "design_base_ref": BASE,
                "accepted_gov_ci_1b_ref": "13f659f368cbb36a2d38c2b0b88efa376f0b690c",
                "accepted_stage8a5_ref": "bf58b47fdef8af774a4107455dfcc6204e594283",
                "retained_r1_ref": "b3358ba2268da3db4eb8352c097495ebb85575d7",
                "acceptance_rows": 70,
                "negative_cases": 50,
                "phase_count": 5,
                "gate_sha256": sha256(gate.stdout),
                "manifest_sha256": sha256(manifest),
                "design_only": True,
                "implementation_enabled": False,
                "stage8b_s_enabled": False,
                "stage8b_execution": False,
                "finam_post_delete": False,
                "redis_xadd_xack": False,
                "redis_live_consumer": False,
                "ack_readiness_publication": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "real_orders": False,
                "stage12_strategy_live": False,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()

    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n"
        ).encode(),
        safety.GATE: gate.stdout,
        safety.EVIDENCE: evidence,
        safety.MANIFEST: manifest,
    }
    with tempfile.TemporaryDirectory(prefix="stage8b-design-handoff-"):
        with zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as archive:
            for entry in entries:
                name = entry["path"]
                archive.writestr(zip_info(name, entry["mode"]), run("git", "show", f"{full_ref}:{name}"))
            for name, data in sorted(additions.items()):
                archive.writestr(zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={full_ref}\n"
        "stage8b-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
