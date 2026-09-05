#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-d4 design review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1d4_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
BASE = "7dc7c802feca6e79d3a1a9902c181ad7b6afc506"
R0 = "b06c78b46d2a5b7a8209d58f1c327d7cf30ae98f"
R1 = "3a3f14f595b9672b23d421e7a857117fb2c578d2"
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence.json"


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1d4-design-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1d4-design-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_ref == BASE:
        raise SystemExit("stage8b-p1d4-design-handoff: FAIL no design commit")
    if run("git", "merge-base", source_ref, BASE).decode().strip() != BASE:
        raise SystemExit("stage8b-p1d4-design-handoff: FAIL accepted P1-d3 drift")
    if run("git", "merge-base", source_ref, R0).decode().strip() != R0:
        raise SystemExit("stage8b-p1d4-design-handoff: FAIL reviewed R0 drift")
    if run("git", "merge-base", source_ref, R1).decode().strip() != R1:
        raise SystemExit("stage8b-p1d4-design-handoff: FAIL reviewed R1 drift")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1d4_design_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1d4-r2-design-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1d4-r2-design-review-package.zip"
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
            "gate_sha256": sha256(gate.stdout),
            "manifest_sha256": sha256(manifest),
        }
    )
    evidence["verification"] = {
        "aggregate_gate": "PASS",
        "format": "PASS",
        "matrix": "PASS",
        "negative_harness": "PASS",
        "scope_checker": "PASS",
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
        "stage8b-p1d4-r2-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
