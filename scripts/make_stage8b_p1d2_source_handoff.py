#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-d2 source review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1d2_source_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_ANNEX = "0cf1cd810a6ff479b69afb914db3b2aa2259593a"
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1d2-source-evidence.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1d2-source-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1d2-source-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_ref == ACCEPTED_ANNEX:
        raise SystemExit("stage8b-p1d2-source-handoff: FAIL no source commit")
    if run("git", "merge-base", source_ref, ACCEPTED_ANNEX).decode().strip() != ACCEPTED_ANNEX:
        raise SystemExit("stage8b-p1d2-source-handoff: FAIL accepted annex drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1d2_source_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1d2-source-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1d2-source-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = json.loads(EVIDENCE_TEMPLATE.read_text(encoding="utf-8"))
    evidence.update(
        {
            "source_ref": source_ref,
            "source_tree": source_tree,
            "source_short_ref": short_ref,
            "archive_name": archive_name,
            "branch": branch,
            "negative_cases": 26,
            "gate_sha256": sha256(gate.stdout),
            "manifest_sha256": sha256(manifest),
        }
    )
    evidence["verification"] = {
        "format": "PASS",
        "source_scope_checker": "PASS",
        "negative_harness": "PASS",
        "strategy_runtime_core_lib": "PASS",
        "runtime_durable_service_lib": "PASS",
        "doctests": "PASS",
        "strict_targeted_clippy": "PASS",
        "aggregate_gate": "PASS",
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
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
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
        "stage8b-p1d2-source-handoff: PASS"
    )


if __name__ == "__main__":
    main()
