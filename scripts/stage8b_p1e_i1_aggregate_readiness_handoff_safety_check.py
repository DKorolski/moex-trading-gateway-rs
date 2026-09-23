#!/usr/bin/env python3
"""Validate the immutable I1 aggregate-readiness handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1_aggregate_readiness_check as aggregate_check
import stage8b_p1e_i1a_handoff_safety_check as common


PARENT = aggregate_check.ACCEPTED_SOURCE
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-e I1 aggregate closure readiness"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1e-i1-aggregate-readiness-evidence.json"
GATE_LOG = "handoff-evidence/stage8b-p1e-i1-aggregate-readiness-gate.log"
LOGS = {"aggregate_readiness_gate": GATE_LOG}

REVIEWS = {
    "handoff-evidence/reviews/FINAM_P1e_I0_R2_REVIEW_afda87a_2026-09-11.md": "de61700f1fa13124ba4b73bf1e43d25d876528b0e7f5817dde15acbbb401745d",
    "handoff-evidence/reviews/FINAM_P1e_FOUNDATION_R2_I1A_R1_REVIEW_130e9a4_2026-09-12.md": "28b41968954924cf28f2dd27cdcc524a3b00285888be3baeaca89ebf1dbe3485",
    "handoff-evidence/reviews/FINAM_P1e_I1A_R2_REVIEW_aa24e84_2026-09-12.md": "38f85a38452a0650713ced0c4569793509d4e144ed2a3bd58b3f82efd3936e29",
    "handoff-evidence/reviews/FINAM_P1e_I1A_SOURCE_CORRECTION_REVIEW_8360c47_2026-09-13.md": "5ccdc3e7beafbc35090ec9e7f406cef9c330da4e15db8d928224784ae543a2e5",
    "handoff-evidence/reviews/FINAM_I1_FIRST_BOOT_CORRECTION_REVIEW_4d7ee64_2026-09-13.md": "d1baa8fe5ca0e6acc37948deceb120bde9906e32aeb14adf75cf7981e80737b1",
    "handoff-evidence/reviews/FINAM_I1_GOVERNANCE_CORRECTION_REVIEW_21eaf01_2026-09-13.md": "cbd70851c19b7c7d502acb685566f077211c528a213f8ba70471f9f5be3580b2",
    "handoff-evidence/reviews/FINAM_I1_TRANSACTION_V5_CORRECTION_REVIEW_5e2e157_2026-09-14.md": "625b4a5508494d8d68d9f7259bbcf9a2c40eb40433f29f540e141c8d17668fd7",
    "handoff-evidence/reviews/FINAM_I1_PRE_SEAL_RECOVERY_CORRECTION_REVIEW_a655da9_2026-09-14.md": "777ccbb2c3ad4377c2d5d8296b53387da622492c1a105e8a120472ac48ba8ddc",
    "handoff-evidence/reviews/FINAM_I1_GENERATED_MARKET_CORRECTION_REVIEW_ff6639e_2026-09-17.md": "668b2be4bae317adea3b8cf3741ade3384cf10414a45f45d32f44a929af21639",
    "handoff-evidence/reviews/FINAM_I1_CANCEL_COMPOSITION_REVIEW_cc1f02c_2026-09-17.md": "5fd0b3220c3c680ac41e597d54e639cef9709879aea51cab0d5002c49c583a86",
    "handoff-evidence/reviews/FINAM_I1_DAY_EXPIRY_CORRECTION_REVIEW_a667938_2026-09-18.md": "abab1053624b8f14a262045a3a7fc8b2b268cb62f6650254d6c82f93ef380e0b",
    "handoff-evidence/reviews/FINAM_I1_COMMITTED_CANCEL_DAY_EXPIRY_REVIEW_efe56a9_2026-09-18.md": "2ee294118e2e4e92d1ffc5a616a5c392fdc67c8ca8f7dd648b81b143e585d413",
    "handoff-evidence/reviews/FINAM_I1_OWNER_LOOP_CORRECTION_REVIEW_e2ce442_2026-09-18.md": "467ead8c60e605b362cca07ca4fe56c8e10ef92318f0782ea18549fbdbb0972f",
    "handoff-evidence/reviews/FINAM_I1_PROCESS_ACCEPTANCE_1086b8d_2026-09-23.md": "df7dc27aa0ed3981ed5c6f3271a0f30f6073b7ef3ac36cb694520b243b102f4a",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE_LOG, *REVIEWS}


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
        set(values) == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"},
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
            require(not item.filename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3")), f"secret/runtime member: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["source_parent"] == PARENT, "source parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        lines = commit_raw.decode().splitlines()
        require(lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in lines, "commit parent mismatch")

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
        require(evidence["status"] == "REVIEW_CANDIDATE_I1_NOT_CLOSED", "evidence status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == PARENT, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == aggregate_check.ALLOWED_CHANGES, "changed path evidence drift")
        require(evidence["production_source_changed"] is False, "production change claimed")
        require(evidence["i1_closed"] is False, "I1 self-closed")
        require(evidence["open_slices"] == ["telemetry composition", "fixed-path installation and systemd material", "aggregate I1 acceptance"], "open slices drift")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")
        require(evidence["review_sha256"] == REVIEWS, "review evidence inventory drift")
        for name, expected in REVIEWS.items():
            require(sha256(files[name]) == expected, f"review digest mismatch: {name}")

        gate = files[GATE_LOG]
        require(evidence["gate_log_sha256"] == sha256(gate), "gate log digest mismatch")
        for token in (
            b"stage8b-p1e-i1-aggregate-readiness-check: PASS milestones=14 rows=15 open_slices=3",
            b"stage8b-p1e-i1-aggregate-readiness-negative-harness: PASS 13/13",
            b"stage8b-p1e-i1-process-supervision-negative-harness 77/77",
            b"stage8b-p1e-i1-transaction-v5-negative-harness 27/27",
            b"PASS stage8b-p1e-i1-aggregate-readiness-gate",
            b"exit_code=0",
        ):
            require(token in gate, f"gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-i1-aggregate-") as directory:
            root = Path(directory)
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            aggregate_check.validate(root, verify_lineage=False)

        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "reviews_verified": len(REVIEWS),
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
        raise SystemExit("usage: stage8b_p1e_i1_aggregate_readiness_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1e-i1-aggregate-readiness-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1-aggregate-readiness-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
