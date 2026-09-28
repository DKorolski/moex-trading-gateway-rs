#!/usr/bin/env python3
"""Build the immutable Stage 8B-P1-e I1A source review package."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_i1a_source_check as source_check
import stage8b_p1e_i1a_source_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
CRASH_FILES = (
    "stage8b-p1d4-crash-replay-run-1.json",
    "stage8b-p1d4-crash-replay-run-2.json",
    "stage8b-p1d4-crash-replay-semantic-digest.txt",
)


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def main() -> None:
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1a-source-handoff: FAIL dirty worktree")
    branch = git("branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1e-i1a-source-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != source_check.REVIEWED_SOURCE:
        raise SystemExit(f"stage8b-p1e-i1a-source-handoff: FAIL source_parent={source_parent}")

    with tempfile.TemporaryDirectory(prefix="stage8b-p1e-i1a-source-") as temporary:
        retained = Path(temporary) / "retained"
        gate = subprocess.run(
            ["bash", "scripts/stage8b_p1e_i1a_source_gate.sh", str(retained)],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if gate.returncode != 0 or b"PASS stage8b-p1e-i1a-source-gate" not in gate.stdout:
            raise SystemExit(gate.stdout.decode(errors="replace"))
        gate_bytes = (retained / "stage8b-p1e-i1a-source-gate.txt").read_bytes()
        crash = {name: (retained / "p1d4-crash-evidence" / name).read_bytes() for name in CRASH_FILES}

        short_ref = source_ref[:7]
        archive_name = f"moex-trading-project-{short_ref}-stage8b-p1e-i1a-source-review-package.zip"
        archive_path = OUTPUT / archive_name
        manifest, entries = common.source_manifest(source_ref)
        manifest_document = json.loads(manifest)
        manifest_document.update(
            {
                "schema_version": 3,
                "stage": "Stage 8B-P1-e I1A source implementation",
                "source_tree": source_tree,
                "source_branch": branch,
            }
        )
        manifest = (json.dumps(manifest_document, indent=2, sort_keys=True) + "\n").encode()
        changed_paths = sorted(
            filter(None, git("diff", "--name-only", source_check.ACCEPTED_DESIGN, source_ref, "--").decode().splitlines())
        )
        if changed_paths != sorted(source_check.EXPECTED_CHANGED):
            raise SystemExit("stage8b-p1e-i1a-source-handoff: FAIL changed path inventory")
        evidence = json.loads(
            (ROOT / "docs/stage-8/stage8b-p1e-i1a-schedule-source-implementation-evidence.json").read_text()
        )
        evidence.update(
            {
                "source_ref": source_ref,
                "source_parent": source_parent,
                "reviewed_source_ref": source_check.REVIEWED_SOURCE,
                "source_tree": source_tree,
                "branch": branch,
                "archive_name": archive_name,
                "worktree_clean": True,
                "pushed_to_origin": False,
                "source_negative_cases": 100,
                "p1d4_sigkill_cells": 105,
                "p1d4_sigkill_runs": 2,
                "changed_paths": changed_paths,
                "gate_sha256": sha256(gate_bytes),
                "manifest_sha256": sha256(manifest),
                "crash_evidence_sha256": {name: sha256(raw) for name, raw in sorted(crash.items())},
            }
        )
        evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
        marker = (
            "stage=Stage 8B-P1-e I1A source implementation\n"
            f"source_short_ref={short_ref}\n"
            f"source_ref={source_ref}\n"
            f"source_parent={source_parent}\n"
            f"source_tree={source_tree}\n"
            f"branch={branch}\n"
            f"accepted_design_ref={source_check.ACCEPTED_DESIGN}\n"
            f"reviewed_source_ref={source_check.REVIEWED_SOURCE}\n"
            f"archive_name={archive_name}\n"
        ).encode()
        additions = {
            safety.MARKER: marker,
            safety.MANIFEST: manifest,
            safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
            safety.EVIDENCE: evidence_bytes,
            safety.GATE: gate_bytes,
            **{f"handoff-evidence/{name}": raw for name, raw in crash.items()},
        }

        OUTPUT.mkdir(parents=True, exist_ok=True)
        archive_path.unlink(missing_ok=True)
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for entry in entries:
                archive.writestr(
                    common.zip_info(entry["path"], entry["mode"]),
                    git("show", f"{source_ref}:{entry['path']}"),
                )
            for name, raw in sorted(additions.items()):
                archive.writestr(common.zip_info(name), raw)

        result = safety.check(str(archive_path))
        digest = sha256(archive_path.read_bytes())
        sha_path = archive_path.with_suffix(".zip.sha256")
        safety_path = archive_path.with_suffix(".zip.safety.json")
        sha_path.write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
        safety_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(
            f"archive={archive_path}\nsha256={digest}\nsafety={safety_path}\n"
            f"source_ref={source_ref}\nsource_tree={source_tree}\n"
            "stage8b-p1e-i1a-source-handoff: PASS"
        )


if __name__ == "__main__":
    main()
