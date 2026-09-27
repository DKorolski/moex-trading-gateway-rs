#!/usr/bin/env python3
"""Validate the immutable Stage 8B-P1-f O2 R0 contract handoff."""

from __future__ import annotations

import hashlib
import json
import sys
import zipfile
from pathlib import PurePosixPath

import stage8b_p1e_i1a_handoff_safety_check as common


STAGE = "Stage 8B-P1-f O2 fresh materialization and isolated bootstrap package"
STATUS = "O2_R0_EXECUTION_CONTRACT_REVIEW_CANDIDATE_DO_NOT_EXECUTE"
BRANCH = "stage8b-paper-shadow-resumption"
CONTRACT_REF = "5e30c8ecee39c35f08a79b4df2fc524b64362fe1"
O1_CLOSURE_REF = "e11744e31f11d567633716f21211c484bb9045eb"
O1_REVIEW_NAME = "FINAM_P1F_O1_OPERATIONAL_ACCEPT_997e8a1_2026-09-27.md"
O1_REVIEW_SHA256 = "59fce7a8048b1c39eeb24f89a759d5cfd8c756af77443bd6d9bb8b1bede34742"

PREFIX = "handoff-evidence/"
MARKER = "handoff-commit.txt"
MANIFEST = PREFIX + "source-tree-manifest.json"
COMMIT_RAW = PREFIX + "source-commit.raw"
EVIDENCE = PREFIX + "stage8b-p1f-o2-handoff-evidence.json"
GATE = PREFIX + "stage8b-p1f-o2-gate.txt"
O1_REVIEW = PREFIX + "reviews/" + O1_REVIEW_NAME
GENERATED = {MARKER, MANIFEST, COMMIT_RAW, EVIDENCE, GATE, O1_REVIEW}

CONTRACT_FILES = {
    "docs/current-status.md",
    "docs/roadmap.md",
    "docs/stage-8/stage8b-p1f-o2-acceptance-matrix.csv",
    "docs/stage-8/stage8b-p1f-o2-execution-package.json",
    "docs/stage-8/stage8b-p1f-o2-execution-package.md",
    "scripts/stage8b_p1f_o2_check.py",
    "scripts/stage8b_p1f_o2_gate.sh",
    "scripts/stage8b_p1f_o2_negative_harness.py",
}
PACKAGING_FILES = {
    "scripts/make_stage8b_p1f_o2_handoff.py",
    "scripts/stage8b_p1f_o2_handoff_safety_check.py",
}
EXPECTED_CHANGES = CONTRACT_FILES | PACKAGING_FILES
REQUIRED = GENERATED | EXPECTED_CHANGES | {
    "docs/stage-8/stage8b-p1f-o1-governance-closure.json",
    "docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.json",
    "docs/stage-8/stage8b-p1e-first-boot-source-plan-v2.json",
    "docs/stage-8/stage8b-p1e-deployment-identity-v2.json",
    "deploy/stage8b-p1e/moex-finam-p1-paper-bootstrap.service",
}
ALLOWED_REPORTS = {
    "reports/stage8b/stage8b-p1f-o0-readonly-probe.txt",
    "reports/stage8b/stage8b-p1f-o1-pre-o0-readonly-probe.txt",
    "reports/stage8b/stage8b-p1f-o1-post-install-readonly-probe.txt",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON key: {key}")
        result[key] = value
    return result


def validate_member_name(name: str) -> None:
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
    if "reports" in path.parts:
        require(name in ALLOWED_REPORTS, f"forbidden reports path: {name}")
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
            "source_short_ref",
            "source_ref",
            "source_parent",
            "source_tree",
            "branch",
            "contract_ref",
            "o1_closure_ref",
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
        require(not (REQUIRED - set(names)), f"missing required members: {sorted(REQUIRED - set(names))}")
        for item in infos:
            validate_member_name(item.filename)
            mode = (item.external_attr >> 16) & 0o177777
            require(mode & 0o170000 != 0o120000, f"symlink member: {item.filename}")
            require(mode & 0o170000 in {0, 0o100000, 0o040000}, f"special member: {item.filename}")

        files = {name: archive.read(name) for name in names}
        marker = parse_marker(files[MARKER])
        require(marker["stage"] == STAGE, "stage mismatch")
        require(marker["archive_name"] == PurePosixPath(path).name, "archive-name mismatch")
        require(marker["source_parent"] == CONTRACT_REF, "source parent mismatch")
        require(marker["contract_ref"] == CONTRACT_REF, "contract ref mismatch")
        require(marker["o1_closure_ref"] == O1_CLOSURE_REF, "O1 closure mismatch")
        require(marker["branch"] == BRANCH, "branch mismatch")
        require(marker["source_ref"].startswith(marker["source_short_ref"]), "short ref mismatch")

        commit_raw = files[COMMIT_RAW]
        require(common.git_object_id("commit", commit_raw) == marker["source_ref"], "commit mismatch")
        commit_lines = commit_raw.decode().splitlines()
        require(commit_lines[0] == f"tree {marker['source_tree']}", "commit tree mismatch")
        require(f"parent {CONTRACT_REF}" in commit_lines, "commit parent mismatch")

        manifest = json.loads(files[MANIFEST], object_pairs_hook=strict_object)
        require(
            manifest["schema_version"] == 2 and manifest["source_ref"] == marker["source_ref"],
            "manifest identity mismatch",
        )
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
        require(common.build_tree_oid(entries, payloads) == marker["source_tree"], "reconstructed tree mismatch")

        evidence = json.loads(files[EVIDENCE], object_pairs_hook=strict_object)
        require(evidence["stage"] == STAGE and evidence["status"] == STATUS, "handoff status mismatch")
        require(evidence["source_ref"] == marker["source_ref"], "evidence source mismatch")
        require(evidence["source_parent"] == CONTRACT_REF, "evidence parent mismatch")
        require(evidence["source_tree"] == marker["source_tree"], "evidence tree mismatch")
        require(evidence["o1_closure_ref"] == O1_CLOSURE_REF, "evidence O1 closure mismatch")
        require(evidence["changed_paths"] == sorted(EXPECTED_CHANGES), "changed paths drift")
        require(evidence["manifest_sha256"] == sha256(files[MANIFEST]), "manifest digest mismatch")
        require(evidence["gate_sha256"] == sha256(files[GATE]), "gate digest mismatch")
        require(evidence["o1_review_sha256"] == O1_REVIEW_SHA256, "review digest record mismatch")
        require(sha256(files[O1_REVIEW]) == O1_REVIEW_SHA256, "review digest mismatch")
        require(evidence["acceptance_rows"] == 30 and evidence["negative_cases"] == 14, "test counts drift")
        require(evidence["rust_tests"] == 3, "Rust test count drift")
        require(evidence["rust_changes"] == 0 and evidence["cargo_changes"] == 0, "Rust/Cargo boundary opened")
        require(all(value is False for value in evidence["effects"].values()), "package declares an effect")
        require(all(value is False for value in evidence["closed_surfaces"].values()), "closed surface opened")

        contract = json.loads(
            files["docs/stage-8/stage8b-p1f-o2-execution-package.json"],
            object_pairs_hook=strict_object,
        )
        require(contract["status"] == "R0_EXECUTION_CONTRACT_REVIEW_CANDIDATE_DO_NOT_EXECUTE", "contract status opened")
        require(all(value is False for value in contract["package_boundary"].values()), "contract performed an effect")
        require(all(value is False for value in contract["closed_surfaces"].values()), "contract surface opened")
        for expected in (
            b"PASS stage8b-p1f-o2-check rows=30 execution=false",
            b"PASS stage8b-p1f-o2-negative-harness 14/14",
            b"PASS stage8b-p1f-o2-gate execution=false",
        ):
            require(expected in files[GATE], f"gate marker missing: {expected!r}")
        require(files[GATE].count(b"test result: ok.") >= 3, "Rust test evidence incomplete")
        return {
            "archive_members": len(names),
            "tracked_members_verified": len(tracked),
            "duplicates": 0,
            "symlinks": 0,
            "unsafe_paths": 0,
            "source_ref": marker["source_ref"],
            "source_tree": marker["source_tree"],
            "contract_ref": CONTRACT_REF,
            "result": "PASS",
        }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: stage8b_p1f_o2_handoff_safety_check.py ARCHIVE")
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
        print(f"stage8b-p1f-o2-handoff-safety: FAIL {error}")
        raise SystemExit(1)
    print("PASS stage8b-p1f-o2-handoff-safety " + json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
