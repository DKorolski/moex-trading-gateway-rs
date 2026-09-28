#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-f R0 design handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_design_check as design
import stage8b_p1f_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
REVIEW_SOURCE = Path("/Users/denisq/Downloads/FINAM_I1_AGGREGATE_ACCEPTANCE_a9bcd94_2026-09-24.md")
REVIEW_SHA256 = "c5be3e66cadd6c45f6678669afa3f9a194c31f6288aae1c3d29fa59aac47ca54"


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_gate(invocation: list[str]) -> tuple[int, bytes]:
    process = subprocess.Popen(invocation, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if process.stdout is None:
        raise SystemExit("stage8b-p1f-design-handoff: FAIL gate stdout unavailable")
    chunks: list[bytes] = []
    for line in iter(process.stdout.readline, b""):
        chunks.append(line)
        print(line.decode(errors="replace"), end="", flush=True)
    return process.wait(), b"".join(chunks)


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-design-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-design-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.PARENT:
        raise SystemExit("stage8b-p1f-design-handoff: FAIL parent drift")
    changed_paths = set(git("diff", "--name-only", safety.PARENT, source_ref, "--").decode().splitlines())
    if changed_paths != design.ALLOWED_CHANGES:
        raise SystemExit("stage8b-p1f-design-handoff: FAIL changed paths")

    review_raw = REVIEW_SOURCE.read_bytes()
    if sha256(review_raw) != REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-design-handoff: FAIL aggregate review digest")

    invocation = ["bash", "scripts/stage8b_p1f_design_gate.sh"]
    returncode, stdout = run_gate(invocation)
    gate_log = (
        f"source_ref={source_ref}\nsource_tree={source_tree}\ninvocation={' '.join(invocation)}\n--- stdout-stderr ---\n".encode()
        + stdout
        + f"\n--- result ---\nexit_code={returncode}\n".encode()
    )
    if returncode != 0:
        raise SystemExit(gate_log.decode(errors="replace"))
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-design-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-r0-design.zip"
    archive_path = OUTPUT / archive_name
    manifest_raw, entries = common.source_manifest(source_ref)
    closed_surfaces = {name: False for name in sorted(design.CLOSED_SURFACES)}
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": "DESIGN_REVIEW_CANDIDATE_NO_ACTIVATION",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "manifest_sha256": sha256(manifest_raw),
        "changed_paths": sorted(changed_paths),
        "accepted_predecessor": design.BASE,
        "aggregate_review_sha256": REVIEW_SHA256,
        "target_baseline_sha256": design.TARGET_SHA256,
        "matrix_sha256": design.MATRIX_SHA256,
        "production_source_changed": False,
        "remote_mutation_performed": False,
        "p1f_source_implementation_authorized": False,
        "operational_activation_authorized": False,
        "closed_surfaces": closed_surfaces,
        "gate_log_sha256": sha256(gate_log),
        "next_after_acceptance": "P1F-I source implementation only",
    }
    evidence_raw = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\nsource_short_ref={short}\nsource_ref={source_ref}\n"
        f"source_parent={source_parent}\nsource_tree={source_tree}\nbranch={branch}\narchive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest_raw,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_raw,
        safety.GATE_LOG: gate_log,
        safety.REVIEW: review_raw,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
        for name, raw in sorted(additions.items()):
            archive.writestr(common.zip_info(name), raw)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p1f-design-handoff: PASS")


if __name__ == "__main__":
    main()
