#!/usr/bin/env python3
"""Create immutable Stage 8B-P R2A7 source/Linux-build handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2a7_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2-readonly-preflight"
PREDECESSOR = "02acf8fd03ad3a80bbb3f87c5bd49316ae3ef7a6"
ADAPTER = ROOT / "tmp/stage8b-r2a7-adapter-exact-a/release/stage8b-r2a7-source-adapter"
SEEDER = ROOT / "tmp/stage8b-r2a7-controlled/release/stage8b-r2a7-controlled-seeder"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2a7-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2a7-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2a7-handoff: FAIL predecessor drift")
    gate = subprocess.run(["bash", "scripts/stage8b_p_r2a7_gate.sh"], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if gate.returncode != 0 or b"stage8b-p-r2a7-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))
    build = json.loads((ROOT / "docs/stage-8/stage8b-p-r2a7-build-evidence.json").read_text())
    binaries = {"adapter": ADAPTER.read_bytes(), "seeder": SEEDER.read_bytes()}
    if sha(binaries["adapter"]) != build["build_a_sha256"] or sha(binaries["seeder"]) != build["controlled_seeder_sha256"]:
        raise SystemExit("stage8b-p-r2a7-handoff: FAIL Linux binary drift")

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (json.dumps({
        "schema_version": 1, "stage": "8B-P", "revision": "R2A7",
        "source_ref": source_ref, "source_tree": source_tree, "archive_name": archive_name,
        "branch": branch, "accepted_predecessor_ref": PREDECESSOR,
        "causal_source_ref": build["source_ref"], "gate_sha256": sha(gate.stdout),
        "manifest_sha256": sha(manifest), "negative_mutations": 18,
        "current_tree_negative_mutations": 33, "controlled_place": True,
        "controlled_cancel": True, "production_fixture_dependencies": False,
        "authorization_status": "NOT_ISSUED", "credential_used": False,
        "network_accessed": False, "operator_arm_issued": False,
        "dispatch_entered": False, "effect_transport_entered": False,
        "finam_order_post_delete_sent": False, "r2b_authorized": False,
        "runtime_live": False, "real_orders": False,
    }, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": (f"source_short_ref={short_ref}\nsource_ref={source_ref}\nsource_tree={source_tree}\narchive_name={archive_name}\n").encode(),
        safety.EVIDENCE: evidence, safety.GATE: gate.stdout, safety.MANIFEST: manifest,
        safety.BINARIES["adapter"]: binaries["adapter"], safety.BINARIES["seeder"]: binaries["seeder"],
    }
    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{source_ref}:{entry['path']}"))
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)
    result = safety.check(str(archive_path))
    digest = sha(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p-r2a7-handoff: PASS")


if __name__ == "__main__":
    main()
