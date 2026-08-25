#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R1A corrective authorization handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r1a_authorization_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-authorization-r1"
PREDECESSOR = "16a59bca74f94881c70d9fa39bbdf1c357e65f95"
R1_REF = "12a7aeec20824d3b90e18caa5961ba28a3eb7fd6"
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r1a-authorization-authority.json"
FRESHNESS = ROOT / "docs/stage-8/stage8b-p-r1a-freshness-budget-authority.json"
NETWORK = ROOT / "docs/stage-8/stage8b-p-r1a-network-policy-authority.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r1a-authorization-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r1a-authorization-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-p-r1a-authorization-handoff: FAIL commit not pushed")
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r1a-authorization-handoff: FAIL predecessor drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r1a_authorization_gate.sh"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    marker = b"stage8b-p-r1a-authorization-gate: PASS rows=64 new_negatives=50 inherited=48 total=98"
    if gate.returncode != 0 or marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8B-P",
                "revision": "R1A",
                "status": "design_only_corrective_authorization_candidate",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": PREDECESSOR,
                "r1_candidate_ref": R1_REF,
                "authority_sha256": sha(AUTHORITY.read_bytes()),
                "freshness_budget_authority_sha256": sha(FRESHNESS.read_bytes()),
                "network_policy_authority_sha256": sha(NETWORK.read_bytes()),
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "acceptance_rows": 64,
                "r1a_negative_mutations": 50,
                "inherited_r1_negative_mutations": 48,
                "total_negative_mutations": 98,
                "authorization_status": "NOT_ISSUED",
                "account_credential_used": False,
                "broker_readonly_get": False,
                "operator_arm_issued": False,
                "dispatch_attempt_recorded": False,
                "transport_entered": False,
                "finam_post_delete": False,
                "broker_effect": False,
                "stage8b_p": False,
                "stage8b_xe": False,
                "redis_execution": False,
                "broker_dispatch": False,
                "runtime_live": False,
                "real_orders": False,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
    }

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
    archive_digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{archive_digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\nsha256={archive_digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r1a-authorization-handoff: PASS"
    )


if __name__ == "__main__":
    main()
