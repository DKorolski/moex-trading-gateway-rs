#!/usr/bin/env python3
"""Create the immutable Stage 8B-P1-d4 source review handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1d4_source_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"
BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_DESIGN = "1a1ea05775f1d15b86fcc3495ad6863b851e9212"
REVIEWED_SOURCE = "250f71a5a36c796281e946eeeb557f04818daab0"
EVIDENCE_TEMPLATE = ROOT / "docs/stage-8/stage8b-p1d4-source-evidence.json"
CRASH_EVIDENCE_ROOT = ROOT / "reports/stage8b-p1d4-r1-crash-evidence"
CRASH_EVIDENCE_FILES = (
    "stage8b-p1d4-crash-replay-run-1.json",
    "stage8b-p1d4-crash-replay-run-2.json",
    "stage8b-p1d4-crash-replay-semantic-digest.txt",
)


def run(*args: str) -> bytes:
    return subprocess.check_output(args, cwd=ROOT)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    if run("git", "status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1d4-source-handoff: FAIL dirty worktree")
    branch = run("git", "branch", "--show-current").decode().strip()
    if branch != BRANCH:
        raise SystemExit(f"stage8b-p1d4-source-handoff: FAIL branch={branch}")
    source_ref = run("git", "rev-parse", "HEAD").decode().strip()
    source_parent = run("git", "rev-parse", "HEAD^").decode().strip()
    source_tree = run("git", "rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != REVIEWED_SOURCE:
        raise SystemExit(
            "stage8b-p1d4-source-handoff: FAIL "
            f"source_parent={source_parent} expected={REVIEWED_SOURCE}"
        )

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1d4_source_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if gate.returncode != 0 or b"PASS stage8b-p1d4-source-gate" not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    crash_evidence = {
        name: (CRASH_EVIDENCE_ROOT / name).read_bytes() for name in CRASH_EVIDENCE_FILES
    }
    semantic_digest = crash_evidence[CRASH_EVIDENCE_FILES[-1]].decode().strip()
    archive_name = f"moex-trading-project-{short_ref}-stage8b-p1d4-source-r1-review-package.zip"
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    changed_paths = run("git", "diff", "--name-only", REVIEWED_SOURCE, source_ref).decode().splitlines()
    protected_prefixes = (
        ".github/",
        "deploy/",
        "deployment/",
        "docker-compose",
        "configs/",
        "config/",
        "crates/broker-finam/",
        "crates/finam-",
    )
    unexpected_protected_changes = [path for path in changed_paths if path.startswith(protected_prefixes)]
    if unexpected_protected_changes:
        raise SystemExit(f"stage8b-p1d4-source-handoff: FAIL protected path changed: {unexpected_protected_changes}")
    evidence = json.loads(EVIDENCE_TEMPLATE.read_text(encoding="utf-8"))
    evidence.update(
        {
            "source_ref": source_ref,
            "reviewed_source_ref": REVIEWED_SOURCE,
            "review_target": source_ref,
            "source_parent": source_parent,
            "source_tree": source_tree,
            "source_short_ref": short_ref,
            "archive_name": archive_name,
            "branch": branch,
            "worktree_clean": True,
            "pushed_to_origin": False,
            "negative_cases": 51,
            "crash_evidence_semantic_digest": semantic_digest,
            "crash_evidence_sha256": {
                name: sha256(data) for name, data in sorted(crash_evidence.items())
            },
            "gate_sha256": sha256(gate.stdout),
            "manifest_sha256": sha256(manifest),
            "changed_paths": changed_paths,
            "changed_paths_sha256": sha256(("\n".join(changed_paths) + "\n").encode()),
            "unexpected_protected_path_changes": unexpected_protected_changes,
        }
    )
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    additions = {
        "handoff-commit.txt": (
            f"source_short_ref={short_ref}\nsource_ref={source_ref}\n"
            f"source_parent={source_parent}\nsource_tree={source_tree}\n"
            f"reviewed_source_ref={REVIEWED_SOURCE}\naccepted_design_ref={ACCEPTED_DESIGN}\n"
            f"branch={branch}\narchive_name={archive_name}\n"
        ).encode(),
        safety.EVIDENCE: evidence_bytes,
        safety.GATE: gate.stdout,
        safety.MANIFEST: manifest,
    }
    additions.update(
        {
            f"handoff-evidence/{name}": data
            for name, data in crash_evidence.items()
        }
    )

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for entry in entries:
            archive.writestr(common.zip_info(entry["path"], entry["mode"]), run("git", "show", f"{source_ref}:{entry['path']}"))
        for name, data in sorted(additions.items()):
            archive.writestr(common.zip_info(name), data)

    result = safety.check(str(archive_path))
    digest = sha256(archive_path.read_bytes())
    archive_path.with_suffix(".zip.sha256").write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    archive_path.with_suffix(".zip.safety.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"archive={archive_path}\nsha256={digest}\nsource_ref={source_ref}\nstage8b-p1d4-source-handoff: PASS")


if __name__ == "__main__":
    main()
