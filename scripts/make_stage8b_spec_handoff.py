#!/usr/bin/env python3
"""Create a deterministic commit-bound Stage 8B-S review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_spec_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-s-r2"
BASE = "a675a772e02fa6da1a33973127542696019eb2f7"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-spec-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-spec-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    if full_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-spec-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", full_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-spec-handoff: FAIL predecessor drift")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    OUTPUT.mkdir(parents=True, exist_ok=True)

    gate = subprocess.run(["bash", "scripts/stage8b_spec_gate.sh"], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    marker = b"stage8b-spec-gate: PASS rows=100 negatives=90 corrective_specification=true implementation=false execution=false finam=false redis=false dispatch=false live=false stage8b_i=false stage8b_p=false stage8b_xt=false stage8b_xe=false stage12=false"
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    manifest, entries = common.source_manifest(full_ref)
    evidence = (json.dumps({
        "schema_version": 2, "stage": "8B-S-R2", "source_ref": full_ref,
        "source_short_ref": short_ref, "archive_name": archive_name,
        "branch": branch, "retained_stage8b_s_r1_ref": BASE,
        "accepted_stage8b_d_merge_ref": "50ed5382fdbe2d62ed253d65a312f951e2a267ff",
        "accepted_stage8b_d_candidate_ref": "f296d0be782b8aa550a20e27600ba16826214349",
        "acceptance_rows": 100, "negative_cases": 90,
        "gate_sha256": sha256(gate.stdout), "manifest_sha256": sha256(manifest),
        "specification_only": True, "production_implementation": False,
        "stage8b_i": False, "stage8b_p": False, "stage8b_xt": False,
        "stage8b_xe": False, "operator_arm_issuance": False, "finam_post_delete": False,
        "network_send": False, "redis_live": False,
        "ack_readiness_publication": False, "broker_dispatch": False,
        "runtime_live": False, "real_orders": False, "stage12": False,
    }, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={full_ref}\narchive_name={archive_name}\n".encode(),
        safety.GATE: gate.stdout, safety.EVIDENCE: evidence, safety.MANIFEST: manifest,
    }
    with tempfile.TemporaryDirectory(prefix="stage8b-spec-handoff-"):
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for entry in entries:
                archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{full_ref}:{entry['path']}"))
            for name, data in sorted(additions.items()):
                archive.writestr(common.zip_info(name), data)
    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={full_ref}\nstage8b-spec-handoff: PASS")


if __name__ == "__main__":
    main()
