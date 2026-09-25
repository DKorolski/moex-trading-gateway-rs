#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-f R1 design-correction handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common
import stage8b_p1f_r1_design_check as design


PARENT = design.BASE
BRANCH = "stage8b-paper-shadow-resumption"
STAGE = "Stage 8B-P1-f R1 isolated operational acceptance design correction"
MARKER = "handoff-commit.txt"
MANIFEST = "handoff-evidence/source-tree-manifest.json"
COMMIT_RAW = "handoff-evidence/source-commit.raw"
EVIDENCE = "handoff-evidence/stage8b-p1f-r1-design-evidence.json"
GATE_LOG = "handoff-evidence/stage8b-p1f-r1-design-gate.log"
REVIEW = "handoff-evidence/reviews/FINAM_P1F_R0_DESIGN_REVIEW_58bb4ca_2026-09-25.md"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE_LOG, REVIEW}
REVIEW_SHA256 = "ea5b184ad7c412e63229c8b98bacda895da151c09fc666057dc512968cc24ba5"
EVIDENCE_KEYS = {
    "schema_version", "stage", "status", "source_ref", "source_parent", "source_tree",
    "branch", "archive_name", "manifest_sha256", "changed_paths", "correction_parent",
    "r0_review_sha256", "accepted_i1_predecessor", "target_baseline_sha256",
    "matrix_sha256", "models_sha256", "findings_closed", "production_source_changed",
    "remote_mutation_performed", "p1f_source_implementation_authorized",
    "operational_activation_authorized", "closed_surfaces", "gate_log_sha256",
    "next_after_acceptance",
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
    require(set(values) == {"stage", "source_short_ref", "source_ref", "source_parent", "source_tree", "branch", "archive_name"}, "marker inventory drift")
    return values


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive member")
        require(not (GENERATED - set(names)), "generated member missing")
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
        require(marker["source_parent"] == PARENT, "parent mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["archive_name"] == Path(path).name, "archive name mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit hash mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {PARENT}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=design.strict_object)
        require(manifest["schema_version"] == 2 and manifest["source_ref"] == marker["source_ref"], "manifest identity mismatch")
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
        require(set(names) - tracked == GENERATED, "generated inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "tree reconstruction mismatch")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=design.strict_object)
        require(set(evidence) == EVIDENCE_KEYS, "evidence key inventory drift")
        require(evidence["schema_version"] == 1 and evidence["stage"] == STAGE, "evidence identity drift")
        require(evidence["status"] == "DESIGN_CORRECTION_REVIEW_CANDIDATE_NO_ACTIVATION", "evidence status drift")
        require(evidence["source_ref"] == marker["source_ref"] and evidence["source_parent"] == PARENT, "evidence source mismatch")
        require(evidence["source_tree"] == marker["source_tree"] and evidence["branch"] == BRANCH, "source binding drift")
        require(evidence["archive_name"] == marker["archive_name"], "archive identity drift")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(set(evidence["changed_paths"]) == design.ALLOWED_CHANGES, "changed paths drift")
        require(evidence["correction_parent"] == PARENT, "correction parent drift")
        require(evidence["r0_review_sha256"] == REVIEW_SHA256 and sha256(files[REVIEW]) == REVIEW_SHA256, "R0 review bytes drift")
        require(evidence["accepted_i1_predecessor"] == "3f171d997de5616cb9a07311d7776e446456c0c1", "I1 predecessor drift")
        require(evidence["target_baseline_sha256"] == design.r0.TARGET_SHA256, "target baseline drift")
        require(evidence["matrix_sha256"] == design.MATRIX_SHA256 and evidence["models_sha256"] == design.MODELS_SHA256, "design input digest drift")
        require(evidence["findings_closed"] == ["P1-F01", "P1-F02", "P1-F03", "P2-F04"], "finding closure inventory drift")
        require(evidence["production_source_changed"] is False and evidence["remote_mutation_performed"] is False, "design performed effects")
        require(evidence["p1f_source_implementation_authorized"] is False and evidence["operational_activation_authorized"] is False, "design self-authorized")
        require(set(evidence["closed_surfaces"]) == design.CLOSED_SURFACES and all(value is False for value in evidence["closed_surfaces"].values()), "closed surface drift")
        require(evidence["next_after_acceptance"] == "P1F-I source implementation only", "next boundary drift")

        gate = files[GATE_LOG]
        require(evidence["gate_log_sha256"] == sha256(gate), "gate digest mismatch")
        for token in (
            b"stage8b-p1f-r1-design-check: PASS rows=64 models=20 artifacts=8 roles=8 phases=7 activation=false",
            b"stage8b-p1f-r1-design-negative-harness: PASS 44/44 no_op=0",
            b"stage8b-p1f-design-handoff-safety: PASS",
            b"PASS stage8b-p1f-r1-design-gate rows=64 models=20 artifacts=8 roles=8 negatives=44 remote_mutation=false activation=false",
            b"exit_code=0",
        ):
            require(token in gate, f"gate marker missing: {token!r}")

        with tempfile.TemporaryDirectory(prefix="stage8b-p1f-r1-design-") as directory:
            root = Path(directory) / "source"
            for name, raw in payloads.items():
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(raw)
            design.validate(root, verify_lineage=False)

        return {
            "archive_members": len(names), "tracked_members_verified": len(tracked),
            "review_verified": True, "duplicates": 0, "symlinks": 0, "unsafe_paths": 0,
            "source_ref": marker["source_ref"], "source_parent": PARENT,
            "source_tree": marker["source_tree"], "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_r1_design_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile,
            json.JSONDecodeError, design.CheckFailure, design.r0.CheckFailure) as error:
        print(f"stage8b-p1f-r1-design-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1f-r1-design-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
