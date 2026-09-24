#!/usr/bin/env python3
"""Validate the immutable I1 fixed-path installation review handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_fixed_install_check as fixed_check
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = "37b9d065ce0e4763c84b1b8aa847e50f8814ca96"
ACCEPTED_BINARY_REF = "b6f6d5b6ea924db8c97512bc2bcecb8a5ed760ac"
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 fixed-path installation source/material correction"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-fixed-install-correction-evidence.json"
REVIEW = "handoff-evidence/reviews/FINAM_I1_FIXED_INSTALL_REVIEW_37b9d06_2026-09-24.md"
REVIEW_SHA256 = "8064f1be9f8f9a61cb4b7005daa906c60c7c951ac4b4d04507f3b7e10de944ba"
REPORT_PREFIX = "handoff-evidence/fixed-install-linux/"
REPORT_FILES = {
    "accepted-binary-build-result.json",
    "accepted-binary-build.log",
    "accepted-binary-source.txt",
    "accepted-binary.sha256",
    "behavioral-filesystem-matrix.json",
    "behavioral-filesystem-matrix.log",
    "install-first.json",
    "install-idempotent.json",
    "linux-runner-invocation.txt",
    "linux-runner.log",
    "rollback-durable-state.stderr",
    "rollback-durable-state.stdout",
    "rollback-operator-material.stderr",
    "rollback-operator-material.stdout",
    "rollback.json",
    "source-and-evidence-gate.log",
    "source-archive-check.txt",
    "static-and-systemd-check.txt",
    "status-installed.json",
    "status-rolled-back.json",
    "target-linux-evidence.json",
}
LOGS = {
    "fixed_install": "handoff-evidence/gates/stage8b-p1e-i1-fixed-install-check.log",
    "fixed_install_negative": "handoff-evidence/gates/stage8b-p1e-i1-fixed-install-negative.log",
    "telemetry": "handoff-evidence/gates/stage8b-p1e-i1-telemetry-check.log",
    "telemetry_negative": "handoff-evidence/gates/stage8b-p1e-i1-telemetry-negative.log",
    "process": "handoff-evidence/gates/stage8b-p1e-i1-process-check.log",
    "process_negative": "handoff-evidence/gates/stage8b-p1e-i1-process-negative.log",
}
ALLOWED_CHANGES = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1e-i1-fixed-path-installation-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1e-i1-fixed-path-installation.md",
    "scripts/make_stage8b_p1e_i1_fixed_install_handoff.py",
    "scripts/stage8b_p1e_i1_fixed_install.py",
    "scripts/stage8b_p1e_i1_fixed_install_behavioral_harness.py",
    "scripts/stage8b_p1e_i1_fixed_install_check.py",
    "scripts/stage8b_p1e_i1_fixed_install_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_fixed_install_linux_rehearsal.sh",
    "scripts/stage8b_p1e_i1_fixed_install_linux_runner.sh",
    "scripts/stage8b_p1e_i1_fixed_install_negative_harness.py",
}
GENERATED = {
    MARKER,
    MANIFEST,
    COMMIT_RAW,
    EVIDENCE,
    REVIEW,
    *LOGS.values(),
    *(REPORT_PREFIX + name for name in REPORT_FILES),
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require("=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in values, "invalid marker field")
        values[key] = value
    require(
        set(values)
        == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"},
        "marker inventory drift",
    )
    return values


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive member")
        require(not (GENERATED - set(names)), "generated evidence member missing")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000}, f"special member: {item.filename}")
            parts = PurePosixPath(item.filename).parts
            require(not any(part == ".env" for part in parts), f"secret member: {item.filename}")
            require(
                not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")),
                f"secret/runtime member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["source_parent"] == PARENT, "source parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 2, "manifest schema mismatch")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"tracked member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(entry["size"] == len(raw), f"size mismatch: {name}")
            require(entry["sha256"] == sha256(raw), f"digest mismatch: {name}")
            require(entry["mode"] == mode, f"mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE])
        require(evidence["status"] == "SOURCE_MATERIAL_CORRECTION_REVIEW_CANDIDATE_I1_NOT_CLOSED", "status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == ALLOWED_CHANGES, "changed-path evidence drift")
        require(evidence["i1_closed"] is False, "I1 self-closed")
        require(evidence["operational_activation_authorized"] is False, "activation opened")
        require(evidence["accepted_release_binary_source_ref"] == ACCEPTED_BINARY_REF, "accepted binary ref drift")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(sha256(files[REVIEW]) == REVIEW_SHA256, "predecessor review digest mismatch")
        require(set(evidence["commands"]) == set(LOGS), "command inventory drift")
        for name, log_path in LOGS.items():
            record = evidence["commands"][name]
            require(record["exit_code"] == 0, f"failed command: {name}")
            require(record["log_path"] == log_path, f"log path drift: {name}")
            require(record["log_sha256"] == sha256(files[log_path]), f"log digest drift: {name}")
            require(b"exit_code=0" in files[log_path], f"successful exit marker missing: {name}")

        report_hashes = evidence["target_evidence_sha256"]
        require(set(report_hashes) == REPORT_FILES, "target evidence inventory drift")
        for name, digest in report_hashes.items():
            require(digest == sha256(files[REPORT_PREFIX + name]), f"target evidence digest drift: {name}")

        with tempfile.TemporaryDirectory(prefix="stage8b-i1-fixed-install-") as directory:
            root = Path(directory) / "source"
            report_dir = Path(directory) / "evidence"
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            report_dir.mkdir()
            for name in REPORT_FILES:
                (report_dir / name).write_bytes(files[REPORT_PREFIX + name])
            fixed_check.check(root)
            fixed_check.check_evidence(report_dir)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "target_evidence_members_verified": len(REPORT_FILES),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_parent": PARENT,
            "source_tree": marker["source_tree"],
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_fixed_install_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1-fixed-install-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-fixed-install-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
