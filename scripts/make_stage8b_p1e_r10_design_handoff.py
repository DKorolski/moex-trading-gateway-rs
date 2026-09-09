#!/usr/bin/env python3
"""Create immutable Stage 8B-P1-e R10 corrected-design handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_r10_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
BASE = "4051a7b4d2c810100aaf983bddede62dbd03d96f"
R9_REVIEW = Path("/Users/denisq/Downloads/FINAM_P1e_R9_REVIEW_4051a7b_2026-09-09.md")
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1e-deployable-supervisor-r10-design-evidence.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-r10-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-r10-design-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    parent = run("git", "rev-parse", "HEAD^").decode().strip()
    if parent != BASE:
        raise SystemExit(f"stage8b-p1e-r10-design-handoff: FAIL direct_parent={parent}")

    review_bytes = R9_REVIEW.read_bytes()
    if sha256(review_bytes) != safety.R9_REVIEW_SHA256:
        raise SystemExit("stage8b-p1e-r10-design-handoff: FAIL R9 review digest")
    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_r10_design_gate.sh"], cwd=ROOT,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1e-r10-design-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1e-r10-design-review-package.zip"
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
        "bundled_r9_review_sha256": sha256(review_bytes),
    })
    evidence["verification"] = {
        "aggregate_gate": "PASS",
        "scope_checker": "PASS",
        "active_contract_317": "PASS",
        "semantic_authority_36": "PASS",
        "integrity_negative_harness_8": "PASS",
        "semantic_redigested_negative_harness_45": "PASS",
        "normalized_semantic_oracle": "PASS",
        "fixture_composition_46": "PASS",
        "corrected_fixture_bindings_10": "PASS",
        "source_effect_paths_7": "PASS",
        "p1d4_revalidation_exactly_once": "PASS",
        "lr12_lr15_replacement_seal_counts": "PASS",
        "clean_immutable_i0_contract": "PASS",
        "retained_pass_fail_evidence_contract": "PASS",
        "retained_evidence_smoke": "PASS",
        "full_exact_test_names_6": "PASS",
        "zero_test_rejection": "PASS",
        "production_scope_unchanged": "PASS",
        "r9_review_binding": "PASS",
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
        safety.R9_REVIEW: review_bytes,
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
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-p1e-r10-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
