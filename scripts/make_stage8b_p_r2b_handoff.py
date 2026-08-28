#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2B Proposal R3 review handoff."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-proposal"
PREDECESSOR = "4ae9ecd858ecf75d9cb6c819c369ee827bf5976f"
BINARIES = {
    "production_stager": ROOT / "tmp/stage8b-r2b-r3-production-a/release/stage8b-r2a8-production-intake-stager",
    "production_writer": ROOT / "tmp/stage8b-r2b-r3-production-a/release/stage8b-r2a8-production-current-source-writer",
    "production_manifest_issuer": ROOT / "tmp/stage8b-r2b-r3-production-a/release/stage8b-r2a8-current-manifest-issuer",
    "production_adapter": ROOT / "tmp/stage8b-r2b-r3-production-a/release/stage8b-r2a7-source-adapter",
    "authority_producer": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-r2a5-authority-producer",
    "authority_issuer": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-r2a5-authority-issuer",
    "package_issuer": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-r2a5-package-issuer",
    "production_launcher": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-r2b-launcher",
    "accepted_helper": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-readonly-preflight",
    "controlled_adapter": ROOT / "tmp/stage8b-r2b-r3-controlled-a/release/stage8b-r2a7-source-adapter",
    "controlled_manifest_issuer": ROOT / "tmp/stage8b-r2b-r3-controlled-a/release/stage8b-r2a8-current-manifest-issuer",
    "controlled_seeder": ROOT / "tmp/stage8b-r2b-r3-controlled-a/release/stage8b-r2a7-controlled-seeder",
    "controlled_tls_server": ROOT / "tmp/stage8b-r2b-r3-tool-a/release/stage8b-r2a5-controlled-server",
    "controlled_launcher": ROOT / "tmp/stage8b-r2b-r3-controlled-launcher-a/release/stage8b-r2b-launcher",
}


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2b-handoff: FAIL predecessor drift")

    environment = dict(os.environ)
    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_proposal_gate.sh"],
        cwd=ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-p-r2b-proposal-gate: PASS revision=R3" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))
    binary_bytes = {name: path.read_bytes() for name, path in BINARIES.items()}

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    closure = {
        "authorization_status": "NOT_ISSUED",
        "finam_network_accessed": False,
        "order_post_delete_sent": False,
        "redis_live_accessed": False,
        "broker_dispatch_entered": False,
        "runtime_live_entered": False,
        "real_orders_sent": False,
    }
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Proposal R3",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": PREDECESSOR,
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "root_authenticated_admission": True,
                "immutable_root_terminal_evidence": True,
                "authoritative_intake_creator": True,
                "full_admission_to_terminal_supervisor": True,
                "negative_mutations": 146,
                **closure,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\nsource_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
    }
    additions.update({safety.BINARIES[name]: data for name, data in binary_bytes.items()})
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)
    result = safety.check(str(archive_path))
    digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2b-handoff: PASS"
    )


if __name__ == "__main__":
    main()
