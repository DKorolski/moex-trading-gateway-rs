#!/usr/bin/env python3
"""Create a deterministic commit-bound Stage 8B-I review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_i_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
GATE_LOG = ROOT / "reports/stage8b-i-r2-gate.log"
BRANCH = "stage8b-i-r2"
BASE = "a52fbcae5340d632ce8b983eda6ecb4b8dedabce"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-i-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-i-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    if full_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-i-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", full_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-i-handoff: FAIL predecessor drift")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    OUTPUT.mkdir(parents=True, exist_ok=True)

    if not GATE_LOG.is_file():
        raise SystemExit("stage8b-i-handoff: FAIL exact-commit gate log missing")
    gate = GATE_LOG.read_bytes()
    marker = b"stage8b-i-gate: PASS revision=R2 rows=92 negatives=70 compile_fail=18 canonical_regression=true no_send=true adapter=false finam=false redis=false dispatch=false live=false real_orders=false stage8b_it=false stage8b_p=false stage8b_xe=false stage12=false"
    exact_ref_marker = f"current-tree-ci-gate: PASS source_ref={full_ref} ".encode()
    regression_marker = b"stage8b-i-full-regression: PASS canonical_ci=true"
    if marker not in gate or exact_ref_marker not in gate or regression_marker not in gate:
        raise SystemExit("stage8b-i-handoff: FAIL stale or incomplete exact-commit gate log")

    manifest, entries = common.source_manifest(full_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8B-I-R2",
                "source_ref": full_ref,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_stage8b_s_candidate_ref": "afecc2584593570b62cbe7f00ee81f64d4b9b26b",
                "accepted_stage8b_s_merge_ref": "d1581962666aa82b993854d0642e67bd66624032",
                "rejected_stage8b_i_ref": BASE,
                "acceptance_rows": 92,
                "negative_cases": 70,
                "compile_fail_negative_cases": 18,
                "canonical_full_regression": True,
                "gate_sha256": sha256(gate),
                "manifest_sha256": sha256(manifest),
                "no_send_implementation": True,
                "authority_constructed_by_public_facade": False,
                "real_adapter": False,
                "operator_arm_issuance": False,
                "finam_post_delete": False,
                "network_send": False,
                "redis_execution": False,
                "ack_readiness_publication": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "real_orders": False,
                "stage8b_it": False,
                "stage8b_p": False,
                "stage8b_xe": False,
                "stage12": False,
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
        safety.GATE: gate,
        safety.EVIDENCE: evidence,
        safety.MANIFEST: manifest,
    }
    with tempfile.TemporaryDirectory(prefix="stage8b-i-handoff-"):
        with zipfile.ZipFile(
            archive_path,
            "w",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=9,
        ) as archive:
            for entry in entries:
                archive.writestr(
                    common.zip_info(entry["path"], entry["mode"]),
                    run("git", "show", f"{full_ref}:{entry['path']}"),
                )
            for name, data in sorted(additions.items()):
                archive.writestr(common.zip_info(name), data)
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
        "stage8b-i-handoff: PASS"
    )


if __name__ == "__main__":
    main()
