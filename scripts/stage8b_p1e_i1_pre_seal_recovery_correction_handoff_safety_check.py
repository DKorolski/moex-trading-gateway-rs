#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-e P1-PSR01 correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common


BRANCH = "stage8b-paper-shadow-resumption"
HELD_PREDECESSOR = "be6707391dc53327fa3a29a40836d24f71eca850"
STAGE = "Stage 8B-P1-e I1 pre-seal recovery freshness correction"
STATUS = "SOURCE_REVIEW_CORRECTION_CANDIDATE"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
    "docs/current-status.md",
    "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-correction.md",
    "scripts/make_stage8b_p1e_i1_pre_seal_recovery_correction_handoff.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_correction_check.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_correction_gate.sh",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_correction_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_correction_negative_harness.py",
}
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1e-i1-pre-seal-recovery-correction-evidence.json"
GATE = PREFIX + "stage8b-p1e-i1-pre-seal-recovery-correction-gate.txt"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE}
REQUIRED = GENERATED | EXPECTED_CHANGED


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode("utf-8").splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker key")
        result[key] = value
    require(
        set(result)
        == {
            "stage",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "held_predecessor",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        require(not (REQUIRED - set(names)), "required member missing")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(
                mode & 0o170000 in {0, 0o100000, 0o040000},
                f"special member: {item.filename}",
            )

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive mismatch")
        require(marker["source_parent"] == HELD_PREDECESSOR, "parent mismatch")
        require(marker["held_predecessor"] == HELD_PREDECESSOR, "baseline mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(
            common.git_object_id("commit", commit_raw) == marker["source_ref"],
            "commit object mismatch",
        )
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {HELD_PREDECESSOR}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        require(manifest["source_tree"] == marker["source_tree"], "manifest tree mismatch")
        require(manifest["source_branch"] == BRANCH, "manifest branch mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(raw) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(raw) == entry["sha256"], f"manifest digest mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated inventory mismatch")
        require(
            common.build_tree_oid(entries, payloads) == marker["source_tree"],
            "reconstructed tree mismatch",
        )

        evidence = json.loads(files[EVIDENCE])
        require(evidence["status"] == STATUS, "evidence status mismatch")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == HELD_PREDECESSOR, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["changed_paths"] == sorted(EXPECTED_CHANGED), "changed paths mismatch")
        require(evidence["negative_cases"] == 17, "negative inventory mismatch")
        require(evidence["fresh_admission_max_age_seconds"] == 300, "freshness drift")
        require(evidence["historical_source_byte_exact"] is True, "historical binding missing")
        require(evidence["administrative_recovery_requires_f00"] is False, "F00 dependency")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "surface opened")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        for expected in (
            b"PASS stage8b-p1e-i1-pre-seal-recovery-correction-check",
            b"PASS stage8b-p1e-i1-pre-seal-recovery-correction-negative-harness 17/17",
            b"PASS stage8b-p1e-i1-transaction-v5-gate",
            b"PASS stage8b-p1e-i1-pre-seal-recovery-correction-gate",
        ):
            require(expected in files[GATE], f"gate marker missing: {expected!r}")

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "held_predecessor": HELD_PREDECESSOR,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1_pre_seal_recovery_correction_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (
        OSError,
        UnicodeDecodeError,
        ValueError,
        KeyError,
        TypeError,
        zipfile.BadZipFile,
        json.JSONDecodeError,
    ) as error:
        print(f"stage8b-p1e-i1-pre-seal-recovery-correction-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print(
        "stage8b-p1e-i1-pre-seal-recovery-correction-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
