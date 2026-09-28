#!/usr/bin/env python3
"""Build the immutable Stage 8B-P1-e P1-PSR01 correction handoff."""

from __future__ import annotations

import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

import make_stage8b_design_handoff as common
import stage8b_p1e_i1_pre_seal_recovery_correction_handoff_safety_check as safety


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "reports/handoff"


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args], cwd=ROOT)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def main() -> None:
    if git("status", "--porcelain", "--untracked-files=all").decode().strip():
        raise SystemExit("stage8b-p1e-i1-pre-seal-recovery-correction-handoff: FAIL dirty worktree")
    branch = git("branch", "--show-current").decode().strip()
    if branch != safety.BRANCH:
        raise SystemExit(f"stage8b-p1e-i1-pre-seal-recovery-correction-handoff: FAIL branch={branch}")
    source_ref = git("rev-parse", "HEAD").decode().strip()
    source_parent = git("rev-parse", "HEAD^").decode().strip()
    source_tree = git("rev-parse", "HEAD^{tree}").decode().strip()
    if source_parent != safety.HELD_PREDECESSOR:
        raise SystemExit(
            "stage8b-p1e-i1-pre-seal-recovery-correction-handoff: FAIL "
            f"parent={source_parent} expected={safety.HELD_PREDECESSOR}"
        )
    changed = set(
        filter(
            None,
            git("diff", "--name-only", safety.HELD_PREDECESSOR, source_ref, "--")
            .decode()
            .splitlines(),
        )
    )
    if changed != safety.EXPECTED_CHANGED:
        raise SystemExit(
            "stage8b-p1e-i1-pre-seal-recovery-correction-handoff: FAIL changed paths "
            f"missing={sorted(safety.EXPECTED_CHANGED - changed)} "
            f"unexpected={sorted(changed - safety.EXPECTED_CHANGED)}"
        )

    gate = subprocess.run(
        ["bash", "scripts/stage8b_p1e_i1_pre_seal_recovery_correction_gate.sh"],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    gate_marker = b"PASS stage8b-p1e-i1-pre-seal-recovery-correction-gate"
    if gate.returncode != 0 or gate_marker not in gate.stdout:
        raise SystemExit(gate.stdout.decode(errors="replace"))

    short_ref = source_ref[:7]
    archive_name = (
        f"moex-trading-project-{short_ref}-stage8b-p1e-i1-pre-seal-recovery-correction-review-package.zip"
    )
    archive_path = OUTPUT / archive_name
    manifest, entries = common.source_manifest(source_ref)
    manifest_document = json.loads(manifest)
    manifest_document.update(
        {
            "schema_version": 3,
            "stage": safety.STAGE,
            "source_tree": source_tree,
            "source_branch": branch,
        }
    )
    manifest = (json.dumps(manifest_document, indent=2, sort_keys=True) + "\n").encode()
    evidence = {
        "schema_version": 1,
        "stage": safety.STAGE,
        "status": safety.STATUS,
        "finding_closed": "P1-PSR01",
        "source_ref": source_ref,
        "source_parent": source_parent,
        "held_predecessor": safety.HELD_PREDECESSOR,
        "source_tree": source_tree,
        "branch": branch,
        "archive_name": archive_name,
        "worktree_clean": True,
        "pushed_to_origin": False,
        "fresh_admission_max_age_seconds": 300,
        "administrative_actions_without_f00": 3,
        "administrative_recovery_requires_f00": False,
        "continuation_actions": 4,
        "historical_source_byte_exact": True,
        "time_advance_boundaries_seconds": [300, 301],
        "long_downtime_days": 30,
        "negative_cases": 17,
        "changed_paths": sorted(changed),
        "deferred": [
            "deployable bootstrap-recover CLI and systemd composition",
            "durable owner loop",
            "release process signal panic SIGKILL restart matrix",
        ],
        "closed_surfaces": {
            "operational_redis_db0_db15": False,
            "vps_activation": False,
            "finam_post_delete": False,
            "broker_dispatch": False,
            "runtime_live": False,
            "real_orders": False,
        },
        "gate_sha256": sha256(gate.stdout),
        "manifest_sha256": sha256(manifest),
    }
    evidence_bytes = (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode()
    marker = (
        f"stage={safety.STAGE}\n"
        f"source_short_ref={short_ref}\n"
        f"source_ref={source_ref}\n"
        f"source_parent={source_parent}\n"
        f"source_tree={source_tree}\n"
        f"branch={branch}\n"
        f"held_predecessor={safety.HELD_PREDECESSOR}\n"
        f"archive_name={archive_name}\n"
    ).encode()
    additions = {
        safety.MARKER: marker,
        safety.MANIFEST: manifest,
        safety.COMMIT_RAW: git("cat-file", "commit", source_ref),
        safety.EVIDENCE: evidence_bytes,
        safety.GATE: gate.stdout,
    }

    OUTPUT.mkdir(parents=True, exist_ok=True)
    archive_path.unlink(missing_ok=True)
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
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
    safety_path.write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(
        f"archive={archive_path}\nsha256={digest}\nsafety={safety_path}\n"
        f"source_ref={source_ref}\nsource_tree={source_tree}\n"
        "stage8b-p1e-i1-pre-seal-recovery-correction-handoff: PASS"
    )


if __name__ == "__main__":
    main()
