#!/usr/bin/env python3
"""Create immutable Stage 8B-P1-e R3 corrected-design handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_r3_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
BASE = "3aaed81da4f1a558b4d31f4d3a169ddceca61e6f"
R2_REVIEW = Path("/Users/denisq/Downloads/P1e_R2_design_engineering_review_3aaed81.md")
R2_REVIEW_SHA256 = "ea5828a52e2098da1fc2f4eeb52c37e40f9328e634f42840e64dc703e2b15b26"
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r3-design-evidence.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-r3-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-r3-design-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_ref == BASE:
        raise SystemExit("stage8b-p1e-r3-design-handoff: FAIL no R3 commit")
    if run("git", "merge-base", source_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-p1e-r3-design-handoff: FAIL R2 lineage drift")

    review_bytes = R2_REVIEW.read_bytes()
    if sha256(review_bytes) != R2_REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-r3-design-handoff: FAIL R2 review digest")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_r3_design_gate.sh"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1e-r3-design-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1e-r3-design-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = json.loads(EVIDENCE_TEMPLATE.read_text(encoding="utf-8"))
    evidence.update({
        "source_ref": source_ref,
        "source_short_ref": short_ref,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
        "bundled_r2_review_sha256": sha256(review_bytes),
    })
    evidence["verification"] = {
        "aggregate_gate": "PASS",
        "scope_checker": "PASS",
        "active_contract_merge_160": "PASS",
        "supersession_map_19": "PASS",
        "r3_redigested_negative_harness_36": "PASS",
        "restart_outer_matrix_22": "PASS",
        "operational_pretransition_matrix_52": "PASS",
        "first_boot_transaction_v2": "PASS",
        "receipt_v1": "PASS",
        "restart_package_v2": "PASS",
        "linear_acquisition_seam_v1": "PASS",
        "p0_unit_immutability": "PASS",
        "r2_review_binding": "PASS",
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
        safety.R2_REVIEW: review_bytes,
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
    archive_path.with_suffix(".zip.sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-r3-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
