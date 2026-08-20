#!/usr/bin/env python3
"""Create a commit-bound I4 Design R2 review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import stage8a4_durable_composition_i4_design_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8a4-durable-composition-i4"


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
        raise SystemExit("stage8a4-i4-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8a4-i4-design-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path = OUTPUT / archive_name

    gate = subprocess.run(
        ["bash", "scripts/stage8a4_durable_composition_i4_design_gate.sh"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    marker = b"stage8a4-durable-composition-i4-design-gate: PASS revision=R2 rows=56 negatives=38 implementation=false ack_publish=false xack=false redis=false finam=false dispatch=false live=false"
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    tracked = run("git", "ls-tree", "-r", "--name-only", "HEAD").decode().splitlines()
    entries = []
    for name in tracked:
        data = run("git", "show", f"HEAD:{name}")
        entries.append({"path": name, "sha256": sha256(data), "size": len(data)})
    manifest = (json.dumps({
        "schema_version": 1, "source_ref": full_ref,
        "entry_count": len(entries), "entries": entries,
    }, indent=2, sort_keys=True) + "\n").encode()
    evidence = (json.dumps({
        "schema_version": 1,
        "stage": "8A-4-durable-composition-I4-design-R2",
        "source_ref": full_ref, "source_short_ref": short_ref,
        "archive_name": archive_name, "branch": branch,
        "accepted_i3_r6_ref": "593ff255ef7826a22e66c9aff6f7ea47acf47644",
        "accepted_i3_r6_review_sha256": "1da167c3e7f1266473133d2d8a1412906a26d7f83b5dc026ce84dc7969090257",
        "rejected_i4_design_r1_ref": "06bb09fa13431d0ae34039f37497d4f37914f022",
        "acceptance_rows": 56, "negative_cases": 38,
        "gate_sha256": sha256(gate.stdout), "manifest_sha256": sha256(manifest),
        "timestamp_model": "timestamp_free_model_a",
        "stable_ack_identity": "reuse_exact_stage7b_terminal_request_ack_identity_sha256",
        "implementation_enabled": False, "ack_publication": False,
        "redis_xack": False, "redis_live": False, "finam_post_delete": False,
        "broker_dispatch": False, "runtime_live": False, "real_orders": False,
    }, indent=2, sort_keys=True) + "\n").encode()

    with tempfile.TemporaryDirectory(prefix="stage8a4-i4-design-") as raw:
        source_zip = Path(raw) / "source.zip"
        subprocess.run(["git", "archive", "--format=zip", f"--output={source_zip}", "HEAD"], cwd=ROOT, check=True)
        additions = {
            "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
            safety.GATE: gate.stdout, safety.EVIDENCE: evidence, safety.MANIFEST: manifest,
        }
        with zipfile.ZipFile(source_zip) as source, zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as target:
            for info in source.infolist():
                target.writestr(info, source.read(info.filename))
            for name, data in sorted(additions.items()):
                target.writestr(generated_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={full_ref}\nstage8a4-i4-design-handoff: PASS")


if __name__ == "__main__":
    main()
