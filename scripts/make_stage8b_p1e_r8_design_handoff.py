#!/usr/bin/env python3
"""Create immutable Stage 8B-P1-e R8 corrected-design handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_r8_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
BASE = "7ebdcef45c1c55f4783bd6b2b1502ea78d4d97d7"
R7_REVIEW = Path("/Users/denisq/Downloads/P1e_R7_design_engineering_review_7ebdcef.md")
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r8-design-evidence.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-r8-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-r8-design-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    parent = run("git", "rev-parse", "HEAD^").decode().strip()
    if parent != BASE:
        raise SystemExit(f"stage8b-p1e-r8-design-handoff: FAIL direct_parent={parent}")

    review_bytes = R7_REVIEW.read_bytes()
    if sha256(review_bytes) != safety.R7_REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-r8-design-handoff: FAIL R7 review digest")
    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_r8_design_gate.sh"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1e-r8-design-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1e-r8-design-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = json.loads(EVIDENCE_TEMPLATE.read_text(encoding="utf-8"))
    evidence.update({
        "source_ref": source_ref, "source_short_ref": short_ref,
        "source_tree": source_tree, "branch": branch,
        "archive_name": archive_name, "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
        "bundled_r7_review_sha256": sha256(review_bytes),
    })
    evidence["verification"] = {
        "aggregate_gate": "PASS", "scope_checker": "PASS",
        "active_contract_279": "PASS", "semantic_authority_31": "PASS",
        "r8_redigested_negative_harness_45": "PASS",
        "r6_partition_15_plus_5_retained": "PASS",
        "route_variant_matrix_30": "PASS", "reclaim_variants_19": "PASS",
        "terminal_variants_11": "PASS", "terminal_resolution_protocol": "PASS",
        "p1d4_revalidation_post_permit": "PASS", "shutdown_cross_product_9": "PASS",
        "e18_e25_cause_preserved": "PASS", "i0_p1d4_105x2_gate_bound": "PASS",
        "i0_three_file_allowlist": "PASS", "production_scope_unchanged": "PASS",
        "r7_review_binding": "PASS",
    }
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence_bytes,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
        safety.R7_REVIEW: review_bytes,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-r8-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
