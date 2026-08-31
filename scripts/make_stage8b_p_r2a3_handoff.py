#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2A3 source/Linux-build handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2a3_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2-readonly-preflight"
PREDECESSOR = "497cf7244c1e717c56303385193dff004bbd4b31"
LINUX_BIN_DIR = ROOT / "tmp/stage8b-r2a3-linux-final"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2a3-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2a3-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2a3-handoff: FAIL predecessor drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2a3_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-p-r2a3-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))
    build = json.loads((ROOT / "docs/stage-8/stage8b-p-r2a3-build-evidence.json").read_text())
    binaries: dict[str, bytes] = {}
    for name, expected in build["linux_release_sha256"].items():
        data = (LINUX_BIN_DIR / name).read_bytes()
        if sha(data) != expected:
            raise SystemExit(f"stage8b-p-r2a3-handoff: FAIL Linux digest drift: {name}")
        binaries[name] = data

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    try:
        upstream_ref = run("git", "rev-parse", "@{upstream}").decode().strip()
    except subprocess.CalledProcessError:
        upstream_ref = ""
    evidence = (
        json.dumps({
            "schema_version": 1,
            "stage": "8B-P",
            "revision": "R2A3",
            "status": "runnable_contract_provenance_qualification_candidate",
            "source_ref": source_ref,
            "source_tree": source_tree,
            "archive_name": archive_name,
            "branch": branch,
            "source_pushed": source_ref == upstream_ref,
            "accepted_predecessor_ref": PREDECESSOR,
            "gate_sha256": sha(gate.stdout),
            "manifest_sha256": sha(manifest),
            "controlled_tests": 32,
            "r2a3_negative_mutations": 47,
            "inherited_negative_mutations": 134,
            "official_contract_documents": 6,
            "strict_golden_fixtures": 6,
            "linux_release_binaries": 3,
            "fd_bound_launcher": True,
            "authorization_status": "NOT_ISSUED",
            "credential_used": False,
            "real_auth_request_sent": False,
            "real_broker_get_sent": False,
            "operator_arm_issued": False,
            "dispatch_attempt_appended": False,
            "effect_transport_entered": False,
            "finam_order_post_delete_sent": False,
            "broker_effect": False,
            "r2b_authorized": False,
            "redis_execution": False,
            "runtime_live": False,
            "real_orders": False,
        }, indent=2, sort_keys=True) + "\n"
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
    for name, data in binaries.items():
        additions[safety.BINARIES[name]] = data

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
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{archive_digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={archive_digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2a3-handoff: PASS"
    )


if __name__ == "__main__":
    main()
