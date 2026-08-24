#!/usr/bin/env python3
"""Create an immutable Stage 8B-P preconditions design-only handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_preconditions_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
GATE_LOG = ROOT / "reports/stage8b-p-preconditions-gate.log"
BUILD_REPORT = ROOT / "reports/stage8b-p-build-repro.json"
BINARY = ROOT / "reports/stage8b-p-broker-cli-aarch64-apple-darwin"
BRANCH = "stage8b-p-preconditions-r2-gov"
BASE = "6cb179509fad97e8be56e31bb930b2a86caefc6a"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-preconditions-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-preconditions-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    if source_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-p-preconditions-handoff: FAIL commit not pushed")
    if run("git", "merge-base", source_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-p-preconditions-handoff: FAIL accepted TLS base drift")

    gate = subprocess.run(["bash", "scripts/stage8b_p_preconditions_gate.sh"], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False).stdout
    GATE_LOG.parent.mkdir(parents=True, exist_ok=True)
    GATE_LOG.write_bytes(gate)
    if b"stage8b-p-preconditions-gate: PASS revision=R2 rows=48 negatives=53" not in gate:
        raise SystemExit("stage8b-p-preconditions-handoff: FAIL gate")
    build_report = BUILD_REPORT.read_bytes()
    binary = BINARY.read_bytes()
    build = json.loads(build_report)
    if sha(binary) != build["executable_sha256"] or len(binary) != build["executable_size"]:
        raise SystemExit("stage8b-p-preconditions-handoff: FAIL build artifact")

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (json.dumps({
        "schema_version": 1,
        "stage": "8B-P-PRECONDITIONS",
        "revision": "R2",
        "source_ref": source_ref,
        "accepted_tls_ref": BASE,
        "archive_name": archive_name,
        "branch": branch,
        "contract_accepted": True,
        "build_accepted": True,
        "governance_ready": True,
        "all_prerequisites_accepted": False,
        "acceptance_rows": 48,
        "negative_mutations": 53,
        "gate_sha256": sha(gate),
        "build_report_sha256": sha(build_report),
        "executable_sha256": sha(binary),
        "manifest_sha256": sha(manifest),
        "stage8b_p": False,
        "stage8b_xe": False,
        "finam_post_delete": False,
        "broker_effect": False,
        "redis_execution": False,
        "broker_dispatch": False,
        "runtime_live": False,
        "real_orders": False,
        "stage12": False,
    }, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": f"source_short_ref={short_ref}\nsource_ref={source_ref}\narchive_name={archive_name}\n".encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate,
        safety.BUILD: build_report,
        safety.BINARY: binary,
        safety.MANIFEST: manifest,
    }
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="stage8b-p-preconditions-handoff-"):
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for entry in entries:
                archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{source_ref}:{entry['path']}"))
            for name, data in sorted(additions.items()):
                archive.writestr(common.zip_info(name, "100755" if name == safety.BINARY else "100644"), data)
    result = safety.check(str(archive_path))
    archive_digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{archive_digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={archive_digest}\nsource_ref={source_ref}\nstage8b-p-preconditions-handoff: PASS")


if __name__ == "__main__":
    main()
