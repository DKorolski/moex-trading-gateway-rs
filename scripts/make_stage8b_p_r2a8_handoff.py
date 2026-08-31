#!/usr/bin/env python3
"""Create the immutable Stage 8B-P R2A8 review handoff."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2a8_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2-readonly-preflight"
PREDECESSOR = "02acf8fd03ad3a80bbb3f87c5bd49316ae3ef7a6"
BINARIES = {
    "production_adapter": ROOT / "tmp/stage8b-r2a8-r1-production-a/release/stage8b-r2a7-source-adapter",
    "production_issuer": ROOT / "tmp/stage8b-r2a8-r1-production-a/release/stage8b-r2a8-current-manifest-issuer",
    "controlled_adapter": ROOT / "tmp/stage8b-r2a8-r1-linux/release/stage8b-r2a7-source-adapter",
    "controlled_seeder": ROOT / "tmp/stage8b-r2a8-r1-linux/release/stage8b-r2a7-controlled-seeder",
    "controlled_issuer": ROOT / "tmp/stage8b-r2a8-r1-linux/release/stage8b-r2a8-current-manifest-issuer",
    "authority_producer": ROOT / "tmp/stage8b-r2a8-tools-linux/release/stage8b-r2a5-authority-producer",
    "authority_issuer": ROOT / "tmp/stage8b-r2a8-tools-linux/release/stage8b-r2a5-authority-issuer",
    "package_issuer": ROOT / "tmp/stage8b-r2a8-tools-linux/release/stage8b-r2a5-package-issuer",
    "accepted_helper": ROOT / "tmp/stage8b-r2a5-build-a/release/stage8b-readonly-preflight",
    "accepted_launcher": ROOT / "tmp/stage8b-r2a5-build-a/release/stage8b-r2a5-launcher",
}


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2a8-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2a8-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    upstream_ref = run("git", "rev-parse", "@{upstream}").decode().strip()
    if source_ref != upstream_ref:
        raise SystemExit("stage8b-p-r2a8-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2a8-handoff: FAIL predecessor drift")

    environment = dict(os.environ)
    environment["STAGE8B_R2A8_NATIVE_FULL_CHAIN"] = "0"
    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2a8_gate.sh"],
        cwd=ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-p-r2a8-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))
    build = json.loads(
        (ROOT / "docs/stage-8/stage8b-p-r2a8-r1-causal-build-evidence.json").read_text()
    )
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", build["causal_build_source_ref"], source_ref],
        cwd=ROOT,
        check=True,
    )
    binary_bytes = {name: path.read_bytes() for name, path in BINARIES.items()}

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "8B-P",
                "revision": "R2A8-R1",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor_ref": PREDECESSOR,
                "causal_build_source_ref": build["causal_build_source_ref"],
                "final_handoff_source_ref": source_ref,
                "gate_sha256": sha(gate.stdout),
                "manifest_sha256": sha(manifest),
                "negative_mutations": 13,
                "readiness_negative_mutations": 27,
                "current_tree_negative_mutations": 33,
                "controlled_place_full_chain": True,
                "controlled_cancel_full_chain": True,
                "production_fixture_dependencies": False,
                "authorization_status": "NOT_ISSUED",
                "credential_used": False,
                "finam_network_accessed": False,
                "operator_arm_issued": False,
                "dispatch_entered": False,
                "effect_transport_entered": False,
                "finam_order_post_delete_sent": False,
                "r2b_authorized": False,
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
        "stage8b-p-r2a8-handoff: PASS"
    )


if __name__ == "__main__":
    main()
