#!/usr/bin/env python3
"""Create a commit-bound Generation-2 composition rebuild review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_generation2_composition_r0_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
LOCAL_ARTIFACT_ROOT = ROOT / "reports/stage8b-p-r2b-generation2-composition-r0/linux-amd64"


def run(*arguments: str) -> bytes:
    return subprocess.check_output(arguments, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def execute(command: list[str], cwd: Path) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    output = completed.stdout.decode(errors="replace")
    if completed.returncode != 0:
        raise SystemExit(output)
    return output


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-generation2-composition-r0-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-generation2-composition-r0-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    upstream_ref = run("git", "rev-parse", "@{upstream}").decode().strip()
    if upstream_ref != source_ref:
        raise SystemExit("stage8b-generation2-composition-r0-handoff: FAIL exact commit not pushed upstream")
    if run("git", "merge-base", source_ref, safety.ACCEPTED_PREDECESSOR).decode().strip() != safety.ACCEPTED_PREDECESSOR:
        raise SystemExit("stage8b-generation2-composition-r0-handoff: FAIL accepted lineage drift")
    if run("git", "rev-parse", safety.SOURCE_FOUNDATION).decode().strip() != safety.SOURCE_FOUNDATION:
        raise SystemExit("stage8b-generation2-composition-r0-handoff: FAIL build source missing")
    if run("git", "rev-parse", f"{safety.SOURCE_FOUNDATION}^{{tree}}").decode().strip() != safety.SOURCE_FOUNDATION_TREE:
        raise SystemExit("stage8b-generation2-composition-r0-handoff: FAIL build source tree drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_generation2_composition_r0_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-generation2-composition-r0-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    build_bytes = (ROOT / safety.BUILD).read_bytes()
    build = json.loads(build_bytes)
    authority_bytes = (ROOT / safety.AUTHORITY).read_bytes()
    rehearsal_bytes = (ROOT / safety.REHEARSAL).read_bytes()
    manifest, entries = common.source_manifest(source_ref)
    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    binary_additions: dict[str, bytes] = {}
    records = build["binaries"]
    for build_name in ("build-a", "build-b"):
        for name, record in records.items():
            local = LOCAL_ARTIFACT_ROOT / build_name / name
            data = local.read_bytes()
            expected = record["build_a_sha256" if build_name == "build-a" else "build_b_sha256"]
            if sha256(data) != expected or not data.startswith(b"\x7fELF"):
                raise SystemExit(f"stage8b-generation2-composition-r0-handoff: FAIL binary={build_name}/{name}")
            binary_additions[f"{safety.ARTIFACT_ROOT}/{build_name}/{name}"] = data

    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": safety.STAGE,
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor": safety.ACCEPTED_PREDECESSOR,
                "build_source_ref": safety.SOURCE_FOUNDATION,
                "build_source_tree": safety.SOURCE_FOUNDATION_TREE,
                "manifest_sha256": sha256(manifest),
                "gate_sha256": sha256(gate.stdout),
                "authority_sha256": sha256(authority_bytes),
                "build_evidence_sha256": sha256(build_bytes),
                "rehearsal_evidence_sha256": sha256(rehearsal_bytes),
                "binary_artifact_count": len(binary_additions),
                "clean_build_count": 2,
                "all_binary_hashes_identical": True,
                "generation": 2,
                "generation_2_active": False,
                "production_credentials_installed": False,
                "controlled_installation": False,
                "authorization": "NOT_ISSUED",
                "finam_endpoint_called": False,
                "container_residue_count": 0,
                "review_status": "INDEPENDENT_REVIEW_REQUIRED",
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    additions: dict[str, tuple[bytes, str]] = {
        "handoff-commit.txt": (
            (
                f"source_short_ref={short_ref}\nsource_ref={source_ref}\nsource_tree={source_tree}\n"
                f"branch={branch}\nbuild_source_ref={safety.SOURCE_FOUNDATION}\n"
                f"build_source_tree={safety.SOURCE_FOUNDATION_TREE}\narchive_name={archive_name}\n"
            ).encode(),
            "100644",
        ),
        safety.EVIDENCE: (evidence, "100644"),
        safety.GATE: (gate.stdout, "100644"),
        safety.MANIFEST: (manifest, "100644"),
    }
    additions.update({name: (data, "100755") for name, data in binary_additions.items()})

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(
                common.zip_info(entry["path"], entry["mode"]),
                run("git", "show", f"{source_ref}:{entry['path']}"),
            )
        for name, (data, mode) in sorted(additions.items()):
            archive.writestr(common.zip_info(name, mode), data)

    result = safety.check(str(archive_path))
    outputs: list[str] = []
    with tempfile.TemporaryDirectory(prefix="stage8b-g2-composition-post-package-") as temporary:
        extracted = Path(temporary)
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extracted)
        for command in (
            [sys.executable, "scripts/stage8b_p_r2b_generation2_composition_r0_check.py", "--root", str(extracted), "--artifact-root", safety.ARTIFACT_ROOT],
            [sys.executable, "scripts/stage8b_p_r2b_generation2_composition_r0_handoff_safety_check.py", str(archive_path)],
            [sys.executable, "scripts/stage8b_p_r2b_generation2_composition_r0_handoff_negative_harness.py", str(archive_path)],
        ):
            outputs.append(execute(command, extracted))

    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    archive_path.with_suffix(".zip.post-package-verification.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "archive_name": archive_name,
                "archive_sha256": digest,
                "source_ref": source_ref,
                "build_source_ref": safety.SOURCE_FOUNDATION,
                "fresh_extraction": True,
                "composition_checker_passed": True,
                "handoff_safety_passed": True,
                "handoff_negative_harness_passed": True,
                "binary_artifacts_verified": len(binary_additions),
                "private_ceremony_members": 0,
                "generation_2_active": False,
                "authorization": "NOT_ISSUED",
                "finam_endpoint_called": False,
                "output": "".join(outputs),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\n"
        "stage8b-generation2-composition-r0-handoff: PASS"
    )


if __name__ == "__main__":
    main()
