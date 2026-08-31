#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2A1 source/build handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2a1_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2-readonly-preflight"
PREDECESSOR = "f1070a428c884f846ed3a2007e38f2401b62e5ce"
R1B_REF = "b9a423c4ffd96bf4a5f69027aa4fef4dcc503830"
IMPLEMENTATION_REF = "42fcba448abbfc870be4ffaa108e1334a30d21cb"
HELPER = ROOT / "tools/stage8b-readonly-preflight/target/release/stage8b-readonly-preflight"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2a1-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2a1-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_ref != run("git", "rev-parse", "@{upstream}").decode().strip():
        raise SystemExit("stage8b-p-r2a1-handoff: FAIL commit not pushed")
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2a1-handoff: FAIL predecessor drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2a1_readonly_preflight_gate.sh"],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    if gate.returncode != 0 or b"stage8b-p-r2a1-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "--manifest-path", "tools/stage8b-readonly-preflight/Cargo.toml"],
        cwd=ROOT, check=True,
    )
    helper = HELPER.read_bytes()
    helper_sha = sha(helper)
    expected = json.loads((ROOT / "docs/stage-8/stage8b-p-r2a1-build-evidence.json").read_text())["helper"]["executable_sha256"]
    if helper_sha != expected:
        raise SystemExit("stage8b-p-r2a1-handoff: FAIL helper build digest drift")

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps({
            "schema_version": 1,
            "stage": "8B-P",
            "revision": "R2A1",
            "status": "corrective_readonly_preflight_candidate",
            "source_ref": source_ref,
            "source_tree": source_tree,
            "archive_name": archive_name,
            "branch": branch,
            "accepted_predecessor_ref": PREDECESSOR,
            "accepted_r1b_ref": R1B_REF,
            "implementation_source_ref": IMPLEMENTATION_REF,
            "helper_executable_sha256": helper_sha,
            "gate_sha256": sha(gate.stdout),
            "manifest_sha256": sha(manifest),
            "controlled_tests": 9,
            "new_negative_mutations": 40,
            "inherited_negative_mutations": 134,
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
        safety.BINARY: helper,
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
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{archive_digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={archive_digest}\nsource_ref={source_ref}\n"
        "stage8b-p-r2a1-handoff: PASS"
    )


if __name__ == "__main__":
    main()
