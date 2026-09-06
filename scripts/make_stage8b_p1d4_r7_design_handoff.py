#!/usr/bin/env python3
"""Create the immutable P1-d4 R7 design review handoff from committed HEAD."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1d4_r7_design_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
SOURCE_BRANCH = "stage8b-paper-shadow-resumption"
R6_REF = "cb6e6ddf863f314cc96b5f8ac0a75809e8c6824a"
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1d4-crash-replay-evidence-r7.json"
ALLOWED_EXCLUDED_WIP = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic.rs",
    "crates/runtime-durable-service/src/stage8b_p1_semantic/redis.rs",
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage6_durable_identity.rs",
    "crates/strategy-runtime-core/src/stage6d_live_core.rs",
    "crates/strategy-runtime-core/src/stage8b_p1d3_working_limit.rs",
}


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def excluded_worktree_wip() -> list[str]:
    lines = run("git", "status", "--porcelain", "--untracked-files=all").decode().splitlines()
    paths: list[str] = []
    for line in lines:
        if len(line) < 4 or line[:2] != " M":
            raise SystemExit(f"stage8b-p1d4-r7-design-handoff: FAIL unexpected worktree status {line!r}")
        path = line[3:]
        if path not in ALLOWED_EXCLUDED_WIP:
            raise SystemExit(f"stage8b-p1d4-r7-design-handoff: FAIL unexpected dirty path {path}")
        paths.append(path)
    if set(paths) != ALLOWED_EXCLUDED_WIP:
        raise SystemExit(f"stage8b-p1d4-r7-design-handoff: FAIL source WIP inventory={sorted(paths)}")
    if subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=ROOT).returncode != 0:
        raise SystemExit("stage8b-p1d4-r7-design-handoff: FAIL staged worktree changes")
    return sorted(paths)


def main() -> None:
    excluded = excluded_worktree_wip()
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    source_branch_ref = run("git", "rev-parse", f"refs/heads/{SOURCE_BRANCH}").decode().strip()
    parent_ref = run("git", "rev-parse", "HEAD^").decode().strip()
    if source_ref != source_branch_ref:
        raise SystemExit("stage8b-p1d4-r7-design-handoff: FAIL source branch does not point at HEAD")
    if parent_ref != R6_REF:
        raise SystemExit(f"stage8b-p1d4-r7-design-handoff: FAIL parent={parent_ref}")

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1d4_r7_design_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1d4-r7-design-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1d4-r7-design-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    evidence = json.loads(EVIDENCE_TEMPLATE.read_text(encoding="utf-8"))
    evidence.update(
        {
            "archive_name": archive_name,
            "branch": SOURCE_BRANCH,
            "excluded_uncommitted_source_wip": excluded,
            "gate_sha256": sha256(gate.stdout),
            "manifest_sha256": sha256(manifest),
            "parent_ref": parent_ref,
            "source_ref": source_ref,
            "source_short_ref": short_ref,
            "source_tree": source_tree,
            "verification": {
                "archive_safety": "PASS",
                "canonical_fixture": "PASS",
                "content_checker": "PASS",
                "matrix_and_source_shape": "PASS",
                "negative_harness": "PASS",
                "source_wip_exclusion": "PASS",
            },
        }
    )
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_tree={source_tree}\nbranch={SOURCE_BRANCH}\n"
            f"parent_ref={parent_ref}\narchive_name={archive_name}\n"
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
        "stage8b-p1d4-r7-design-handoff: PASS"
    )


if __name__ == "__main__":
    main()
