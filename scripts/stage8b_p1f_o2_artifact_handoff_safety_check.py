#!/usr/bin/env python3
"""Validate the commit-bound Stage 8B-P1F O2 execution-artifact handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath
from typing import Any

import stage8b_p1e_i1a_handoff_safety_check as common


STAGE = "Stage 8B-P1F O2 immutable execution artifact"
STATUS = "REVIEW_CANDIDATE_EXECUTION_NOT_AUTHORIZED"
BRANCH = "stage8b-paper-shadow-resumption"
CONTRACT_REF = "a9f8fe30a45752c943f9e399775322d83fcd8a36"
IMPLEMENTATION_REF = "9e7f44d119cac63e33d5d5437ff972e49922c797"
ARTIFACT_REF = "5ab038c2da48b649a641b45e3bd00eb44e658f8f"
REVIEW_NAME = "FINAM_P1F_O2_R1_CONTRACT_ACCEPT_a9f8fe3_2026-09-27.md"
REVIEW_SHA256 = "3342409133760dbb8ea909460310881eebdd91c797ed97f3879be4cfbb12ce1b"

PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-o2-artifact-handoff-evidence.json"
GATE = PREFIX + "stage8b-p1f-o2-artifact-gate.txt"
BUILD = PREFIX + "stage8b-p1f-o2-linux-build.json"
REVIEW = PREFIX + "reviews/" + REVIEW_NAME
MATERIALIZER = "payload/stage8b-p1f-o2-materializer"
OPERATOR = "payload/stage8b-p1f-o2-operator"
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, BUILD, REVIEW, MATERIALIZER, OPERATOR}

REQUIRED_TRACKED = {
    "docs/stage-8/stage8b-p1f-o2-execution-artifact.json",
    "docs/stage-8/stage8b-p1f-o2-execution-artifact.md",
    "docs/stage-8/stage8b-p1f-o2-artifact-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1f-o2-authority-public-key.hex",
    "docs/stage-8/stage8b-p1f-o2-materialization-policy.json",
    "docs/stage-8/stage8b-p1f-o2-source-template.json",
    "docs/stage-8/stage8b-p1f-o2-supervisor-template.json",
    "deploy/stage8b-p1e/moex-finam-p1f-o2-materializer.service",
    "deploy/stage8b-p1e/moex-finam-p1-paper-o2-bootstrap-runner.service",
    "scripts/stage8b_p1f_o2_artifact_check.py",
    "scripts/stage8b_p1f_o2_artifact_negative_harness.py",
    "scripts/stage8b_p1f_o2_artifact_witness.sh",
    "scripts/stage8b_p1f_o2_artifact_gate.sh",
    "scripts/make_stage8b_p1f_o2_artifact_handoff.py",
    "scripts/stage8b_p1f_o2_artifact_handoff_safety_check.py",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON key: {key}")
        value[key] = item
    return value


def validate_name(name: str) -> None:
    path = PurePosixPath(name)
    require(
        bool(name)
        and not name.startswith("/")
        and "\\" not in name
        and all(part not in {"", ".", ".."} for part in path.parts),
        f"unsafe archive member: {name!r}",
    )
    require(
        not any(part in {".git", "target", "tmp", "__pycache__", "__MACOSX"} for part in path.parts),
        f"forbidden archive path: {name}",
    )
    basename = path.name
    require(
        basename != ".env" and not (basename.startswith(".env.") and basename != ".env.example"),
        f"secret env member: {name}",
    )
    require(
        not basename.endswith((".pem", ".key", ".ed25519", ".sqlite", ".sqlite3", ".rdb")),
        f"secret/runtime artifact: {name}",
    )


def parse_marker(raw: bytes) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in raw.decode().splitlines():
        require(bool(line) and "=" in line, "invalid marker line")
        key, value = line.split("=", 1)
        require(bool(key) and bool(value) and key not in result, "invalid marker item")
        result[key] = value
    require(
        set(result)
        == {
            "stage",
            "status",
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "contract_ref",
            "implementation_ref",
            "artifact_ref",
            "archive_name",
        },
        "marker inventory drift",
    )
    return result


def check(path: str) -> dict[str, Any]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        names = [item.filename for item in infos]
        by_name = {item.filename: item for item in infos}
        require(len(names) == len(set(names)), "duplicate archive members")
        for item in infos:
            validate_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")
        files = {name: archive.read(name) for name in names}
        require(not (GENERATED - set(names)), f"generated members missing: {sorted(GENERATED - set(names))}")
        require(not (REQUIRED_TRACKED - set(names)), f"tracked members missing: {sorted(REQUIRED_TRACKED - set(names))}")

        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE and marker["status"] == STATUS, "marker status drift")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive name drift")
        require(marker["branch"] == BRANCH, "branch drift")
        require(marker["contract_ref"] == CONTRACT_REF, "contract ref drift")
        require(marker["implementation_ref"] == IMPLEMENTATION_REF, "implementation ref drift")
        require(marker["artifact_ref"] == ARTIFACT_REF, "artifact ref drift")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref drift")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit object mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {marker['source_parent']}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=strict_object)
        require(manifest["schema_version"] == 2, "manifest schema drift")
        require(manifest["source_ref"] == marker["source_ref"], "manifest source drift")
        entries = manifest["entries"]
        require(manifest["entry_count"] == len(entries), "manifest count drift")
        tracked: set[str] = set()
        payloads: dict[str, bytes] = {}
        for entry in entries:
            name = entry["path"]
            require(name not in tracked and name in files, f"manifest member mismatch: {name}")
            raw = files[name]
            mode = f"{((by_name[name].external_attr >> 16) & 0o177777):06o}"
            require(len(raw) == entry["size"], f"manifest size mismatch: {name}")
            require(sha256(raw) == entry["sha256"], f"manifest hash mismatch: {name}")
            require(mode == entry["mode"], f"manifest mode mismatch: {name}")
            tracked.add(name)
            payloads[name] = raw
        require(set(names) - tracked == GENERATED, "generated member inventory drift")
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "source tree reconstruction failed")

        artifact = json.loads(
            files["docs/stage-8/stage8b-p1f-o2-execution-artifact.json"],
            object_pairs_hook=strict_object,
        )
        require(artifact["status"] == STATUS, "artifact status drift")
        require(artifact["accepted_contract_ref"] == CONTRACT_REF, "artifact contract drift")
        require(artifact["implementation_ref"] == IMPLEMENTATION_REF, "artifact implementation drift")
        require(artifact["execution_authorized"] is False, "execution opened")
        require(artifact["target_mutation_performed"] is False, "target mutation claimed")
        binary_by_name = {item["name"]: item for item in artifact["build"]["binaries"]}
        require(sha256(files[MATERIALIZER]) == binary_by_name["stage8b-p1f-o2-materializer"]["sha256"], "materializer payload drift")
        require(len(files[MATERIALIZER]) == binary_by_name["stage8b-p1f-o2-materializer"]["size"], "materializer size drift")
        require(sha256(files[OPERATOR]) == binary_by_name["stage8b-p1f-o2-operator"]["sha256"], "operator payload drift")
        require(len(files[OPERATOR]) == binary_by_name["stage8b-p1f-o2-operator"]["size"], "operator size drift")
        require(files[MATERIALIZER][:4] == b"\x7fELF" and files[OPERATOR][:4] == b"\x7fELF", "payload is not ELF")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=strict_object)
        require(evidence["stage"] == STAGE and evidence["status"] == STATUS, "evidence status drift")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source drift")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree drift")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest evidence drift")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate evidence drift")
        require(evidence["build_sha256"] == sha256(files[BUILD]), "build evidence drift")
        require(evidence["review_sha256"] == REVIEW_SHA256 == sha256(files[REVIEW]), "review evidence drift")
        require(evidence["private_authority_key_in_handoff"] is False, "private authority key included")
        require(evidence["finam_credentials_in_handoff"] is False, "FINAM credential included")
        require(evidence["execution_authorized"] is False, "evidence opened execution")
        require(evidence["target_mutation_performed"] is False, "evidence claimed target mutation")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")

        build = json.loads(files[BUILD], object_pairs_hook=strict_object)
        require(build["platform"] == "linux/amd64", "build platform drift")
        require(build["implementation_ref"] == IMPLEMENTATION_REF, "build implementation drift")
        require(build["materializer_sha256"] == sha256(files[MATERIALIZER]), "build materializer drift")
        require(build["operator_sha256"] == sha256(files[OPERATOR]), "build operator drift")
        for marker_text in (
            b"PASS stage8b-p1f-o2-artifact-check rows=30 execution=false",
            b"PASS stage8b-p1f-o2-artifact-negative-harness 25/25",
            b"PASS stage8b-p1f-o2-artifact-witness execution=false",
            b"PASS stage8b-p1f-o2-artifact-gate execution=false",
        ):
            require(marker_text in files[GATE], f"gate marker missing: {marker_text!r}")

        return {
            "result": "PASS",
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "materializer_sha256": sha256(files[MATERIALIZER]),
            "operator_sha256": sha256(files[OPERATOR]),
            "execution_authorized": False,
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_o2_artifact_handoff_safety_check.py ARCHIVE")
    try:
        result = check(sys.argv[1])
    except (OSError, UnicodeDecodeError, ValueError, KeyError, TypeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        print(f"stage8b-p1f-o2-artifact-handoff-safety: FAIL {error}")
        raise SystemExit(1) from error
    print("PASS stage8b-p1f-o2-artifact-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
