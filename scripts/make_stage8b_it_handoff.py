#!/usr/bin/env python3
"""Create a deterministic commit-bound Stage 8B-IT review archive."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_it_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
GATE_LOG = ROOT / "reports/stage8b-it-gate.log"
BRANCH = "stage8b-it"
BASE = "0af222f252cdc2b4c763c9e04935a5cb5f0c6d65"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-it-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-it-handoff: FAIL branch={branch}")
    full_ref = run("git", "rev-parse", "HEAD").decode().strip()
    if full_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-it-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", full_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-it-handoff: FAIL predecessor drift")
    short_ref = run("git", "rev-parse", "--short=7", "HEAD").decode().strip()
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    OUTPUT.mkdir(parents=True, exist_ok=True)

    if not GATE_LOG.is_file():
        raise SystemExit("stage8b-it-handoff: FAIL exact-commit gate log missing")
    gate = GATE_LOG.read_bytes()
    marker = b"stage8b-it-gate: PASS revision=R3 rows=78 negatives=68 external_compile_fail=12 internal_compile_fail=6 canonical_full_regression=true adapter=1 post=1 delete=1 send=1 controlled_only=true broker_effect=false stage8b_p=false stage8b_xe=false stage12=false"
    if marker not in gate:
        raise SystemExit("stage8b-it-handoff: FAIL stale or incomplete exact-commit gate log")
    if f"current-tree-ci-gate: PASS source_ref={full_ref} ".encode() not in gate:
        raise SystemExit("stage8b-it-handoff: FAIL full regression is not exact-commit bound")
    if b"stage8b-i-full-regression: PASS canonical_ci=true" not in gate:
        raise SystemExit("stage8b-it-handoff: FAIL canonical full regression missing")

    manifest, entries = common.source_manifest(full_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8B-IT",
                "revision": "R3",
                "source_ref": full_ref,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": BASE,
                "accepted_predecessor_replay": True,
                "rejected_stage8b_it_ref": "e44053917a928aeb4bc8e3330a58a693edc31fd3",
                "acceptance_rows": 78,
                "negative_cases": 68,
                "external_compile_fail_negative_cases": 12,
                "internal_compile_fail_negative_cases": 6,
                "gate_sha256": sha256(gate),
                "manifest_sha256": sha256(manifest),
                "adapter_qualified": True,
                "request_parts_module_private": True,
                "adapter_parent_only": True,
                "single_consuming_transition": True,
                "k4_proof_bound_stage8a2_extraction": True,
                "adapter_input_unforgeable": True,
                "reqwest_automatic_retry_disabled": True,
                "mandatory_classifier_inside_adapter": True,
                "classified_only_result": True,
                "canonical_full_regression": True,
                "controlled_tls_qualification": "blocking_stage8b_p_precondition",
                "controlled_loopback_only": True,
                "single_transport_attempt": True,
                "accepted_builder_bridge": True,
                "accepted_classifier_bridge": True,
                "production_endpoint_authority": False,
                "production_operator_arm": False,
                "broker_effect": False,
                "finam_network_send": False,
                "redis_execution": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "real_orders": False,
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
    with tempfile.TemporaryDirectory(prefix="stage8b-it-handoff-"):
        with zipfile.ZipFile(
            archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9,
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
        "stage8b-it-handoff: PASS"
    )


if __name__ == "__main__":
    main()
