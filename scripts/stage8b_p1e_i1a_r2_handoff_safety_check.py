#!/usr/bin/env python3
"""Validate the immutable I1A R2 semantic-identity design handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common


DESIGN_REF = "62b3f1cd200f0f907cfe5c7d9e6877aade105dcb"
I1A_R1_HANDOFF_REF = "130e9a4593570c4f3e9e6e40806baef85f487eec"
I1A_R1_DESIGN_REF = "7686c93eb9f38a0124d2ae558a80b78f51c99f8f"
FOUNDATION_REF = "37088964e50c0ceb4d82887a30c32103110749b0"
REVIEW_SHA256 = "28b41968954924cf28f2dd27cdcc524a3b00285888be3baeaca89ebf1dbe3485"
PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
INDEX = PREFIX + "stage8b-p1e-i1a-r2-verification-index.json"
REVIEW = PREFIX + "FINAM_P1e_FOUNDATION_R2_I1A_R1_REVIEW_130e9a4_2026-09-12.md"
LOGS = {
    "i1a_r2_design_gate": PREFIX + "stage8b-p1e-i1a-r2-design-gate.txt",
    "fmt": PREFIX + "cargo-fmt-all-check.txt",
    "strategy_runtime_core_lib": PREFIX + "strategy-runtime-core-lib-all-features.txt",
    "runtime_durable_service_lib": PREFIX + "runtime-durable-service-lib-all-features.txt",
    "strategy_runtime_core_doc": PREFIX + "strategy-runtime-core-doc-all-features.txt",
    "runtime_durable_service_doc": PREFIX + "runtime-durable-service-doc-all-features.txt",
    "strict_clippy": PREFIX + "strict-clippy-two-crates.txt",
    "p1d4_negative": PREFIX + "p1d4-source-negative-harness.txt",
}
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, INDEX, REVIEW, *LOGS.values()}
REQUIRED = GENERATED | {
    "docs/stage-8/stage8b-p1e-i1-supervisor-foundation-r2-review-boundary.md",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.md",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v2.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v2.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v2.schema.json",
    "docs/stage-8/stage8b-p1e-i1a-source-progression-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-r1-model-fixtures-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-implementation-scope-v2.json",
    "docs/stage-8/stage8b-p1e-i1a-semantic-identity-v1.schema.json",
    "docs/stage-8/stage8b-p1e-i1a-semantic-identity-contract-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-envelope-v3.schema.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-policy-v3.json",
    "docs/stage-8/stage8b-p1e-i1a-source-progression-v2.json",
    "docs/stage-8/stage8b-p1e-i1a-r2-acceptance-matrix-v1.csv",
    "docs/stage-8/stage8b-p1e-i1a-r2-semantic-fixtures-v1.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v3.json",
    "docs/stage-8/stage8b-p1e-i1a-schedule-source-design-v3.md",
    "scripts/stage8b_p1e_i1a_r2_design_check.py",
    "scripts/stage8b_p1e_i1a_r2_semantic_model.py",
    "scripts/stage8b_p1e_i1a_r2_negative_harness.py",
    "scripts/stage8b_p1e_i1a_r2_design_gate.sh",
}


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
            "source_tree",
            "source_branch",
            "parent_ref",
            "design_ref",
            "i1a_r1_handoff_ref",
            "i1a_r1_design_ref",
            "foundation_ref",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, object]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        require(len(names) == len(set(names)), "duplicate archive members")
        require(not (REQUIRED - set(names)), f"missing required members: {sorted(REQUIRED - set(names))}")
        for item in infos:
            common.validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["design_ref"] == DESIGN_REF, "design ref drift")
        require(marker["i1a_r1_handoff_ref"] == I1A_R1_HANDOFF_REF, "R1 handoff ref drift")
        require(marker["i1a_r1_design_ref"] == I1A_R1_DESIGN_REF, "R1 design ref drift")
        require(marker["foundation_ref"] == FOUNDATION_REF, "foundation ref drift")
        require(marker["parent_ref"] == DESIGN_REF, "tooling is not a direct design child")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
        commit_lines = commit_raw.decode("utf-8").splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {DESIGN_REF}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST])
        require(manifest["schema_version"] == 3, "manifest version drift")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source mismatch")
        require(manifest["source_tree"] == marker["source_tree"], "manifest tree mismatch")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count mismatch")
        by_name = {item.filename: item for item in infos}
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            body = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(body) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(body) == entry["sha256"], f"manifest digest mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = body
        require(set(names) - tracked == GENERATED, "generated member inventory mismatch")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "reconstructed tree mismatch")

        require(sha256(files[REVIEW]) == REVIEW_SHA256, "review digest mismatch")
        index = json.loads(files[INDEX])
        require(index["source_ref"] == marker["source_ref"], "index source mismatch")
        require(index["source_tree"] == marker["source_tree"], "index tree mismatch")
        require(index["design_ref"] == DESIGN_REF, "index design mismatch")
        require(index["i1a_r1_handoff_ref"] == I1A_R1_HANDOFF_REF, "index R1 handoff mismatch")
        require(index["i1a_r1_design_ref"] == I1A_R1_DESIGN_REF, "index R1 design mismatch")
        require(index["foundation_ref"] == FOUNDATION_REF, "index foundation mismatch")
        require(index["review_sha256"] == REVIEW_SHA256, "index review mismatch")
        require(index["source_manifest_sha256"] == sha256(files[MANIFEST]), "index manifest mismatch")
        require(index["all_passed"] is True, "verification not all-pass")
        require(set(index["checks"]) == set(LOGS), "retained check inventory drift")
        for name, log_path in LOGS.items():
            record = index["checks"][name]
            require(record["exit_code"] == 0, f"failed retained check: {name}")
            require(record["log_path"] == log_path, f"log path mismatch: {name}")
            require(record["log_sha256"] == sha256(files[log_path]), f"log digest mismatch: {name}")
            require(record["log_bytes"] == len(files[log_path]), f"log size mismatch: {name}")
            require(f"source_ref={marker['source_ref']}".encode() in files[log_path], f"log source missing: {name}")
            require(b"exit_code=0" in files[log_path], f"log exit missing: {name}")

        markers = {
            "i1a_r2_design_gate": b"stage8b-p1e-i1a-r2-design-gate: ok",
            "fmt": b"exit_code=0",
            "strategy_runtime_core_lib": b"test result: ok.",
            "runtime_durable_service_lib": b"test result: ok.",
            "strategy_runtime_core_doc": b"test result: ok.",
            "runtime_durable_service_doc": b"test result: ok.",
            "strict_clippy": b"Finished",
            "p1d4_negative": b"PASS stage8b-p1d4-source-negative-harness 60/60",
        }
        for name, expected in markers.items():
            require(expected in files[LOGS[name]], f"retained marker missing: {name}")

        correction = index["i1a_r2_design"]
        require(correction["semantic_identity_fields"] == 9, "identity field count drift")
        require(correction["linked_model_cases"] == 8, "linked case count drift")
        require(correction["negative_cases"] == 28, "negative case count drift")
        require(correction["effective_design_cases"] == 127, "effective case count drift")
        require(correction["inherited_table_lookup_cases"] == 14, "coverage disclosure drift")
        require(correction["production_implementation_authorized"] is False, "implementation opened")
        require(all(value is False for value in index["closed_surfaces"].values()), "operational surface opened")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "retained_checks": len(LOGS),
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1e_i1a_r2_handoff_safety_check.py ARCHIVE")
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
        print(f"stage8b-p1e-i1a-r2-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("stage8b-p1e-i1a-r2-handoff-safety: PASS " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
