#!/usr/bin/env python3
"""Create a self-verifying controlled-installation R0 design handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p_r2b_controlled_installation_r0_handoff_safety_check as safety

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-p-r2b-controlled-installation-r0"
PREDECESSOR = "6672819e357a3c2a2c1e73e5408c393da01913a1"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p-r2b-controlled-installation-r0-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p-r2b-controlled-installation-r0-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if run("git", "merge-base", source_ref, PREDECESSOR).decode().strip() != PREDECESSOR:
        raise SystemExit("stage8b-p-r2b-controlled-installation-r0-handoff: FAIL predecessor lineage drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p_r2b_controlled_installation_r0_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"stage8b-p-r2b-controlled-installation-r0-gate: PASS" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = (
        json.dumps(
            {
                "schema_version": 1,
                "stage": "Stage 8B-P R2B Controlled Installation / Full Transaction Proof R0",
                "source_ref": source_ref,
                "source_tree": source_tree,
                "source_short_ref": short_ref,
                "archive_name": archive_name,
                "branch": branch,
                "accepted_predecessor": PREDECESSOR,
                "gate_sha256": sha256(gate.stdout),
                "manifest_sha256": sha256(manifest),
                "phase_count": 6,
                "service_invocations": 31,
                "negative_mutations": 20,
                "installed": False,
                "enabled": False,
                "started": False,
                "operator_selected": False,
                "real_credentials_materialized": False,
                "authorization": "NOT_ISSUED",
                "finam_open": False,
                "runtime_live": False,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
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

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{source_ref}:{entry['path']}"))
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name, "100644"), data)

    result = safety.check(str(archive_path))
    with tempfile.TemporaryDirectory(prefix="stage8b-r2b-controlled-installation-r0-") as temporary:
        extracted = Path(temporary)
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extracted)
        verification_output: list[str] = []
        for command in (
            [sys.executable, "scripts/stage8b_p_r2b_controlled_installation_r0_check.py", "--root", str(extracted)],
            [sys.executable, "scripts/stage8b_p_r2b_controlled_installation_r0_negative_harness.py"],
        ):
            completed = subprocess.run(command, cwd=extracted, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
            verification_output.append(completed.stdout.decode(errors="replace"))
            if completed.returncode != 0:
                raise SystemExit("".join(verification_output))
        completed = subprocess.run(
            [sys.executable, "scripts/stage8b_p_r2b_controlled_installation_r0_handoff_safety_check.py", str(archive_path)],
            cwd=extracted,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        verification_output.append(completed.stdout.decode(errors="replace"))
        if completed.returncode != 0:
            raise SystemExit("".join(verification_output))

    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    archive_path.with_suffix(".zip.post-package-verification.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "archive_name": archive_name,
                "archive_sha256": digest,
                "fresh_extraction": True,
                "checker_passed": True,
                "negative_harness_passed": True,
                "handoff_safety_passed": True,
                "manual_artifact_copy_performed": False,
                "output": "".join(verification_output),
            },
            indent=2,
            sort_keys=True,
        ) + "\n",
        encoding="utf-8",
    )
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p-r2b-controlled-installation-r0-handoff: PASS")


if __name__ == "__main__":
    main()
