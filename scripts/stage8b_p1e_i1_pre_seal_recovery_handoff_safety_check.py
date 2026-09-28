#!/usr/bin/env python3
"""Validate an immutable Stage 8B-P1-e I1 pre-seal recovery handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common


BRANCH = "stage8b-paper-shadow-resumption"
ACCEPTED_PREDECESSOR = "5e2e157e032406fdbb9047c33c641f5973514504"
STAGE = "Stage 8B-P1-e I1 pre-seal administrative recovery"
STATUS = "SOURCE_REVIEW_CANDIDATE"
EXPECTED_CHANGED = {
    "crates/runtime-durable-service/src/lib.rs",
    "crates/runtime-durable-service/src/recovery.rs",
    "crates/runtime-durable-service/src/stage8b_p1_bootstrap.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_source.rs",
    "crates/runtime-durable-service/src/stage8b_p1e_first_boot_transaction.rs",
    "docs/current-status.md",
    "docs/stage-8/stage8b-p1e-i1-pre-seal-recovery-implementation.md",
    "scripts/make_stage8b_p1e_i1_pre_seal_recovery_handoff.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_check.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_gate.sh",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_handoff_safety_check.py",
    "scripts/stage8b_p1e_i1_pre_seal_recovery_negative_harness.py",
}
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1e-i1-pre-seal-recovery-evidence.json"
GATE = PREFIX + "stage8b-p1e-i1-pre-seal-recovery-gate.txt"
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
            "accepted_predecessor",
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
        require(
            not (REQUIRED - set(names)),
            f"missing required members: {sorted(REQUIRED - set(names))}",
        )
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
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_parent"] == ACCEPTED_PREDECESSOR, "source parent mismatch")
        require(marker["accepted_predecessor"] == ACCEPTED_PREDECESSOR, "predecessor mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(
            common.git_object_id("commit", commit_raw) == marker["source_ref"],
            "commit object mismatch",
        )
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(
            f"parent {ACCEPTED_PREDECESSOR}" in commit_lines,
            "commit parent mismatch",
        )

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 3, "manifest version mismatch")
        require(manifest["stage"] == STAGE, "manifest stage mismatch")
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
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(
            common.build_tree_oid(entries, payloads) == marker["source_tree"],
            "reconstructed tree mismatch",
        )

        evidence = json.loads(files[EVIDENCE])
        require(evidence["status"] == STATUS, "evidence status mismatch")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == ACCEPTED_PREDECESSOR, "evidence parent mismatch")
        require(
            evidence["accepted_predecessor"] == ACCEPTED_PREDECESSOR,
            "evidence predecessor mismatch",
        )
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["changed_paths"] == sorted(EXPECTED_CHANGED), "changed paths mismatch")
        require(
            (
                evidence["recovery_actions"],
                evidence["continuable_frontiers"],
                evidence["quarantine_frontiers"],
                evidence["response_loss_hooks"],
                evidence["negative_cases"],
            )
            == (7, 4, 2, 4, 21),
            "evidence inventory mismatch",
        )
        require(evidence["generation_guard"] is True, "generation guard missing")
        require(
            all(value is False for value in evidence["closed_surfaces"].values()),
            "closed surface opened",
        )
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        for expected in (
            b"PASS stage8b-p1e-i1-pre-seal-recovery-check",
            b"PASS stage8b-p1e-i1-pre-seal-recovery-negative-harness 21/21",
            b"PASS stage8b-p1e-i1-transaction-v5-gate",
            b"PASS stage8b-p1e-i1-pre-seal-recovery-gate",
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
            "accepted_predecessor": ACCEPTED_PREDECESSOR,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(
            "usage: stage8b_p1e_i1_pre_seal_recovery_handoff_safety_check.py ARCHIVE"
        )
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
        print(f"stage8b-p1e-i1-pre-seal-recovery-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print(
        "stage8b-p1e-i1-pre-seal-recovery-handoff-safety: PASS "
        + json.dumps(result, sort_keys=True)
    )


if __name__ == "__main__":
    main()
