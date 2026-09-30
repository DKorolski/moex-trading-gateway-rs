#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1F O2 execution-artifact handoff."""

from __future__ import annotations

import hashlib
import base64
import json
import os
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1f_o2_artifact_handoff_safety_check as safety
import test_stage8b_p1f_o2_artifact_archive as archive_probes


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
DOWNLOADS = Path("/Users/denisq/Downloads")
BUILD_DIR = ROOT / "tmp/o2-linux-9d9bd11-offline"
BUILD_ROOT = BUILD_DIR / "target/release"


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def run_capture(command: list[str]) -> bytes:
    process = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if process.returncode != 0:
        raise SystemExit(process.stdout.decode(errors="replace"))
    return process.stdout


def main() -> None:
    status = git("status", "--porcelain", "--untracked-files=all").decode().strip()
    if status:
        raise SystemExit(f"stage8b-p1f-o2-artifact-handoff: FAIL dirty worktree\n{status}")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1f-o2-artifact-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if subprocess.run(("git", "merge-base", "--is-ancestor", safety.ARTIFACT_REF, source_ref), cwd=ROOT).returncode != 0:
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL artifact ref is not an ancestor")

    materializer = (BUILD_ROOT / "stage8b-p1f-o2-materializer").read_bytes()
    operator = (BUILD_ROOT / "stage8b-p1f-o2-operator").read_bytes()
    supervisor = (BUILD_ROOT / "stage8b-p1-paper-supervisor").read_bytes()
    artifact = json.loads((ROOT / "docs/stage-8/stage8b-p1f-o2-execution-artifact.json").read_text())
    expected = {item["name"]: item for item in artifact["build"]["binaries"]}
    if sha256(materializer) != expected["stage8b-p1f-o2-materializer"]["sha256"] or len(materializer) != expected["stage8b-p1f-o2-materializer"]["size"]:
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL materializer payload drift")
    if sha256(operator) != expected["stage8b-p1f-o2-operator"]["sha256"] or len(operator) != expected["stage8b-p1f-o2-operator"]["size"]:
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL operator payload drift")
    if sha256(supervisor) != expected["stage8b-p1-paper-supervisor"]["sha256"] or len(supervisor) != expected["stage8b-p1-paper-supervisor"]["size"]:
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL bootstrap runtime payload drift")

    review = (DOWNLOADS / safety.REVIEW_NAME).read_bytes()
    if sha256(review) != safety.REVIEW_SHA256:
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL accepted review digest")
    source_review = (DOWNLOADS / safety.SOURCE_REVIEW_NAME).read_bytes()
    if sha256(source_review) != safety.SOURCE_REVIEW_SHA256:
        raise SystemExit("source review digest mismatch")
    gate = run_capture(["bash", "scripts/stage8b_p1f_o2_artifact_gate.sh"])
    if git("rev-parse", "HEAD").decode().strip() != source_ref or git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1f-o2-artifact-handoff: FAIL source changed during gate")

    short = source_ref[:7]
    archive_name = f"moex-trading-project-{short}-stage8b-p1f-o2-execution-artifact.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    build = (BUILD_DIR / "build.json").read_bytes()
    build_log = (BUILD_DIR / "build.log").read_bytes()
    build_commit = (BUILD_DIR / "source-commit.raw").read_bytes()
    build_manifest, build_entries = common.source_manifest(safety.IMPLEMENTATION_REF)
    current_names = {entry["path"] for entry in entries}
    originals = {}
    for entry in build_entries:
        name = entry["path"]
        raw = git("show", f"{safety.IMPLEMENTATION_REF}:{name}")
        if name not in current_names or raw != git("show", f"{source_ref}:{name}"):
            originals[name] = base64.b64encode(raw).decode()
    elf_smoke = run_capture([
        "docker", "run", "--rm", "--network", "none", "--platform", "linux/amd64",
        "--mount", f"type=bind,src={BUILD_ROOT},dst=/payload,readonly",
        "--mount", f"type=bind,src={ROOT / 'deploy/stage8b-p1e'},dst=/units,readonly",
        "--mount", f"type=bind,src={ROOT / 'scripts/stage8b_p1f_o2_elf_smoke.sh'},dst=/probe.sh,readonly",
        "--mount", f"type=bind,src={ROOT / 'docs/stage-8/stage8b-p1f-o2-supervisor-template.json'},dst=/template.json,readonly",
        artifact["build"]["rust_image"], "bash", "/probe.sh",
    ])
    closed_surfaces = dict(artifact["closed_surfaces"])
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "status": safety.STATUS,
                "source_ref": source_ref,
                "source_parent": source_parent,
                "source_tree": source_tree,
                "branch": branch,
                "archive_name": archive_name,
                "contract_ref": safety.CONTRACT_REF,
                "implementation_ref": safety.IMPLEMENTATION_REF,
                "artifact_ref": safety.ARTIFACT_REF,
                "manifest_sha256": sha256(manifest),
                "gate_sha256": sha256(gate),
                "build_sha256": sha256(build),
                "review_sha256": sha256(review),
                "elf_smoke_sha256": sha256(elf_smoke),
                "systemd_runtime_tested": False,
                "materializer_sha256": sha256(materializer),
                "operator_sha256": sha256(operator),
                "bootstrap_supervisor_sha256": sha256(supervisor),
                "private_authority_key_in_handoff": False,
                "finam_credentials_in_handoff": False,
                "execution_authorized": False,
                "target_mutation_performed": False,
                "closed_surfaces": closed_surfaces,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    marker = (
        f"stage={safety.STAGE}\n"
        f"status={safety.STATUS}\n"
        f"source_short_ref={short}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"contract_ref={safety.CONTRACT_REF}\n"
        f"implementation_ref={safety.IMPLEMENTATION_REF}\n"
        f"artifact_ref={safety.ARTIFACT_REF}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions: dict[str, tuple[bytes, str]] = {
        safety.MARKER: (marker, "100644"),
        safety.MANIFEST: (manifest, "100644"),
        safety.COMMIT_RAW: (git("cat-file", "commit", source_ref), "100644"),
        safety.EVIDENCE: (evidence, "100644"),
        safety.GATE: (gate, "100644"),
        safety.BUILD: (build, "100644"),
        safety.BUILD_LOG: (build_log, "100644"),
        safety.BUILD_COMMIT: (build_commit, "100644"),
        safety.BUILD_MANIFEST: (build_manifest, "100644"),
        safety.BUILD_ORIGINALS: ((json.dumps(originals, sort_keys=True) + "\n").encode(), "100644"),
        safety.ELF_SMOKE: (elf_smoke, "100644"),
        safety.SOURCE_REVIEW: (source_review, "100644"),
        safety.REVIEW: (review, "100644"),
        safety.MATERIALIZER: (materializer, "100755"),
        safety.OPERATOR: (operator, "100755"),
        safety.SUPERVISOR: (supervisor, "100755"),
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    if archive_path.exists():
        raise SystemExit(f"immutable archive already exists: {archive_path}")
    # Publish only verified bytes; never replace an existing immutable package.
    with tempfile.TemporaryDirectory(prefix=".o2-preseal-", dir=OUTPUT) as staging:
        staged_archive = Path(staging) / archive_name
        with zipfile.ZipFile(staged_archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for entry in entries:
                archive.writestr(common.zip_info(entry["path"], entry["mode"]), git("show", f"{source_ref}:{entry['path']}"))
            for name, (raw, mode) in sorted(additions.items()):
                archive.writestr(common.zip_info(name, mode), raw)
        result = safety.check(str(staged_archive))
        probes = archive_probes.run(staged_archive)
        os.link(staged_archive, archive_path)
    digest = sha256(archive_path.read_bytes())
    sha_path = archive_path.with_suffix(".zip.sha256")
    safety_path = archive_path.with_suffix(".zip.safety.json")
    sha_path.write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    safety_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    archive_path.with_suffix(".zip.negative.json").write_text(json.dumps(probes, indent=2) + "\n", encoding="utf-8")
    print(
        f"archive={archive_path}\n"
        f"sha256_file={sha_path}\n"
        f"safety_file={safety_path}\n"
        f"sha256={digest}\n"
        f"source_ref={source_ref}\n"
        "PASS stage8b-p1f-o2-artifact-handoff"
    )


if __name__ == "__main__":
    main()
