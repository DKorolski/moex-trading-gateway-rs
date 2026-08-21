#!/usr/bin/env python3
"""Create an immutable commit-bound Stage 8B Design R1 review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8b_design_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-design"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def generated_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    return info


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-design-handoff: FAIL branch={branch}")

    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    gate = subprocess.run(
        ["bash", "scripts/stage8b_design_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    marker = b"stage8b-design-gate: PASS rows=48 negatives=36 design_only=true implementation=false execution=false finam=false redis=false dispatch=false live=false"
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()
    entries: list[dict[str, object]] = []
    for name in tracked:
        data = run("git", "show", f"HEAD:{name}")
        entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
    manifest = (json.dumps({
        "schema_version": 1,
        "source_ref": full_ref,
        "entry_count": len(entries),
        "entries": entries,
    }, indent=2, sort_keys=True) + "\n").encode()
    evidence = (json.dumps({
        "schema_version": 1,
        "stage": "8B-design-R1",
        "source_ref": full_ref,
        "source_short_ref": short_ref,
        "archive_name": archive_name,
        "branch": branch,
        "design_base_ref": "0ce76a334f12bf7b13e682ca976c9a4cde6be137",
        "accepted_stage8a5_ref": "bf58b47fdef8af774a4107455dfcc6204e594283",
        "accepted_stage8a5_review_sha256": "72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc",
        "acceptance_rows": 48,
        "negative_cases": 36,
        "phase_count": 5,
        "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
        "design_only": True,
        "implementation_enabled": False,
        "stage8b_execution": False,
        "finam_post_delete": False,
        "redis_xadd_xack": False,
        "redis_live_consumer": False,
        "ack_readiness_publication": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
    }, indent=2, sort_keys=True) + "\n").encode()

    with tempfile.TemporaryDirectory(prefix="stage8b-design-handoff-") as raw:
        source_zip = Path(raw) / "source.zip"
        subprocess.run(
            ["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"],
            cwd=ROOT,
            check=True,
        )
        additions = {
            "handoff-commit.txt": (
                f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n"
            ).encode(),
            safety.GATE: gate.stdout,
            safety.EVIDENCE: evidence,
            safety.MANIFEST: manifest,
        }
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
        ) as target:
            for info in source.infolist():
                target.writestr(info, source.read(info.filename))
            for name, data in sorted(additions.items()):
                target.writestr(generated_info(name), data)

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
