#!/usr/bin/env python3
"""Verify a complete, commit-bound Stage 5G-b implementation handoff."""

from __future__ import annotations

import argparse
import json
import re
import sys
import zipfile
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage5g_entry_handoff_safety_check as common


STAGE = "5G-b-mock-ack-attachment"
BRANCH = "stage5g-lifecycle"
PREDECESSOR = "011fd4b7baaa41fffdad7d3c28e463b7977f5989"
SOURCE_MANIFEST = "stage5g-b-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-b-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-b-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-b-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-b-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-b-archive-safety.stderr.txt"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
FORBIDDEN_SOURCE_CONTENT = re.compile(
    rb"(75" rb"02[A-Z0-9]*|190" rb"9892|63" rb"170[A-Z0-9/]*|"
    rb"tapi_[sa]k_[A-Za-z0-9_-]+|"
    rb"eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,})"
)

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "mock-ack-checker": ["python3", "scripts/stage5g_b_mock_ack_check.py"],
    "mock-ack-negative": [
        "python3",
        "scripts/stage5g_b_mock_ack_negative_harness.py",
    ],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5g_mock_ack",
        "--quiet",
    ],
    "focused-release": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "--release",
        "stage5g_mock_ack",
        "--quiet",
    ],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": [
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--quiet",
        "--",
        "-D",
        "warnings",
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}

REQUIRED_SOURCE_FILES = {
    "crates/strategy-runtime-core/src/lib.rs",
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "docs/current-status.md",
    "docs/stage-5/5g-b-mock-ack-attachment.md",
    "docs/stage-5/stage5g-b-mock-ack-contract.json",
    "scripts/stage5g_b_mock_ack_check.py",
    "scripts/stage5g_b_mock_ack_negative_harness.py",
    "scripts/make_stage5g_b_handoff_archive.py",
    "scripts/stage5g_b_handoff_safety_check.py",
}

EXPECTED_CHANGED_PATHS = sorted(REQUIRED_SOURCE_FILES)


def parse_marker(raw: bytes) -> dict[str, str]:
    try:
        lines = raw.decode("utf-8").splitlines()
    except UnicodeDecodeError as exc:
        common.fail(f"handoff marker is not UTF-8: {exc}")
    marker: dict[str, str] = {}
    for line in lines:
        if not line or "=" not in line:
            common.fail("invalid handoff marker line")
        key, value = line.split("=", 1)
        if not key or not value or key in marker:
            common.fail("invalid or duplicate handoff marker key")
        marker[key] = value
    common.exact_keys(
        marker,
        {
            "stage",
            "source_ref",
            "source_commit",
            "source_branch",
            "archive_name",
            "stage5g_a_predecessor",
        },
        "handoff marker",
    )
    return marker


def validate_source(
    files: dict[str, bytes], marker: dict[str, str]
) -> tuple[set[str], dict[str, Any]]:
    manifest = common.decode_json(files[SOURCE_MANIFEST], SOURCE_MANIFEST)
    common.exact_keys(
        manifest,
        {
            "schema_version",
            "stage",
            "source_ref",
            "source_commit",
            "source_branch",
            "parent_ref",
            "head_tree",
            "members",
        },
        "source manifest",
    )
    common.require(manifest["schema_version"], 1, "source manifest schema")
    common.require(manifest["stage"], STAGE, "source manifest stage")
    common.require(manifest["source_ref"], marker["source_ref"], "source manifest ref")
    common.require(
        manifest["source_commit"], marker["source_commit"], "source manifest short ref"
    )
    common.require(manifest["source_branch"], BRANCH, "source branch")
    common.require(manifest["parent_ref"], PREDECESSOR, "source parent")
    if not HEX40.fullmatch(manifest["head_tree"]):
        common.fail("invalid source tree id")

    members = manifest["members"]
    if not isinstance(members, list) or not members:
        common.fail("source manifest members missing")
    source_paths: set[str] = set()
    payloads: dict[str, bytes] = {}
    normalized: list[dict[str, str]] = []
    for index, raw_member in enumerate(members):
        member = common.exact_keys(
            raw_member,
            {"git_mode", "path", "sha256"},
            f"source member[{index}]",
        )
        if member["git_mode"] not in {"100644", "100755"}:
            common.fail(f"unsupported source mode: {member['git_mode']}")
        common.validate_member_name(member["path"])
        if member["path"] in source_paths:
            common.fail(f"duplicate source path: {member['path']}")
        if not HEX64.fullmatch(member["sha256"]):
            common.fail(f"invalid source SHA-256: {member['path']}")
        if member["path"] not in files:
            common.fail(f"source member missing: {member['path']}")
        payload = files[member["path"]]
        common.require(
            common.sha256(payload), member["sha256"], f"source content {member['path']}"
        )
        if b"\0" not in payload and FORBIDDEN_SOURCE_CONTENT.search(payload):
            common.fail(f"forbidden live-like literal in source: {member['path']}")
        source_paths.add(member["path"])
        payloads[member["path"]] = payload
        normalized.append(member)
    missing = REQUIRED_SOURCE_FILES - source_paths
    if missing:
        common.fail(f"required source files missing: {sorted(missing)}")
    common.require(
        common.build_tree_oid(normalized, payloads), manifest["head_tree"], "source tree oid"
    )

    commit_raw = files[COMMIT_OBJECT]
    common.require(
        common.git_object_id("commit", commit_raw), marker["source_ref"], "commit object id"
    )
    commit_lines = commit_raw.decode("utf-8", errors="strict").splitlines()
    common.require(commit_lines[0], f"tree {manifest['head_tree']}", "commit tree binding")
    if f"parent {PREDECESSOR}" not in commit_lines:
        common.fail("accepted Stage 5G-a parent binding missing")
    return source_paths, manifest


def validate_evidence(files: dict[str, bytes], marker: dict[str, str]) -> None:
    manifest = common.decode_json(files[EVIDENCE_MANIFEST], EVIDENCE_MANIFEST)
    common.exact_keys(
        manifest,
        {
            "schema_version",
            "stage",
            "source_ref",
            "source_branch",
            "stage5g_a_predecessor",
            "gate_count",
            "gates",
            "repository_state",
            "closed_surfaces",
        },
        "evidence manifest",
    )
    common.require(manifest["schema_version"], 1, "evidence schema")
    common.require(manifest["stage"], STAGE, "evidence stage")
    common.require(manifest["source_ref"], marker["source_ref"], "evidence source")
    common.require(manifest["source_branch"], BRANCH, "evidence branch")
    common.require(manifest["stage5g_a_predecessor"], PREDECESSOR, "evidence predecessor")
    closed = manifest["closed_surfaces"]
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        common.fail("all execution surfaces must remain closed")

    repository = common.exact_keys(
        manifest["repository_state"],
        {
            "git_status_member",
            "git_status_sha256",
            "git_status_clean",
            "changed_paths_base_ref",
            "changed_paths_member",
            "changed_paths_sha256",
        },
        "repository state",
    )
    common.require(repository["git_status_clean"], True, "repository clean")
    common.require(repository["changed_paths_base_ref"], PREDECESSOR, "changed paths base")
    for stem in ("git_status", "changed_paths"):
        member_name = repository[f"{stem}_member"]
        if member_name not in files:
            common.fail(f"repository evidence missing: {member_name}")
        common.require(
            common.sha256(files[member_name]),
            repository[f"{stem}_sha256"],
            f"repository evidence hash {stem}",
        )
    common.require(files[repository["git_status_member"]], b"", "embedded git status")
    changed_paths = files[repository["changed_paths_member"]].decode("utf-8").splitlines()
    common.require(changed_paths, EXPECTED_CHANGED_PATHS, "embedded changed paths")

    gates = manifest["gates"]
    common.require(manifest["gate_count"], len(EXPECTED_COMMANDS), "gate count")
    if not isinstance(gates, list) or len(gates) != len(EXPECTED_COMMANDS):
        common.fail("evidence gate array count mismatch")
    seen: set[str] = set()
    for index, raw_gate in enumerate(gates):
        gate = common.exact_keys(
            raw_gate,
            {"label", "result_member", "result_sha256"},
            f"gate[{index}]",
        )
        label = gate["label"]
        if label in seen or label not in EXPECTED_COMMANDS:
            common.fail(f"unknown or duplicate gate: {label}")
        seen.add(label)
        result_member = gate["result_member"]
        if result_member not in files:
            common.fail(f"gate result missing: {result_member}")
        common.require(
            common.sha256(files[result_member]), gate["result_sha256"], f"gate hash {label}"
        )
        result = common.decode_json(files[result_member], result_member)
        common.require(result.get("stage"), STAGE, f"gate stage {label}")
        common.require(result.get("label"), label, f"gate label {label}")
        common.require(result.get("command"), EXPECTED_COMMANDS[label], f"gate command {label}")
        common.require(result.get("source_ref"), marker["source_ref"], f"gate source {label}")
        common.require(result.get("exit_code"), 0, f"gate exit {label}")
        for stream in ("stdout", "stderr"):
            stream_member = result.get(f"{stream}_member")
            if stream_member not in files:
                common.fail(f"gate {stream} missing: {label}")
            common.require(
                common.sha256(files[stream_member]),
                result.get(f"{stream}_sha256"),
                f"gate {stream} hash {label}",
            )
        stdout = files[result["stdout_member"]].decode(errors="replace")
        if label == "mock-ack-checker" and "stage5g-b-mock-ack-check: PASS" not in stdout:
            common.fail("mock ACK checker success marker missing")
        if label == "mock-ack-negative" and "stage5g-b-negative-harness: PASS 15/15" not in stdout:
            common.fail("mock ACK negative marker missing")
        if label in {"focused-debug", "focused-release"} and "14 passed" not in stdout:
            common.fail(f"focused test count marker missing: {label}")
        if label == "forbidden-no-rg" and "stage5f-forbidden-no-rg-gate: ok" not in stdout:
            common.fail("forbidden no-rg marker missing")
    common.require(seen, set(EXPECTED_COMMANDS), "evidence gate labels")


def validate_archive(archive_path: Path, allow_missing_final_safety: bool) -> tuple[dict[str, str], int]:
    try:
        with zipfile.ZipFile(archive_path) as archive:
            infos = archive.infolist()
            names = [info.filename for info in infos]
            if len(names) != len(set(names)):
                common.fail("archive contains duplicate members")
            for info in infos:
                name = info.filename.rstrip("/")
                if name:
                    common.validate_member_name(name)
                unix_type = (info.external_attr >> 16) & 0o170000
                if unix_type not in {0, 0o040000, 0o100000}:
                    common.fail(f"archive contains symlink or special member: {info.filename}")
            files = {info.filename: archive.read(info) for info in infos if not info.is_dir()}
    except (OSError, zipfile.BadZipFile) as exc:
        common.fail(f"cannot open archive: {exc}")

    required = {COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST}
    if not required.issubset(files):
        common.fail(f"required generated members missing: {sorted(required - set(files))}")
    final_safety = {SAFETY_RESULT, SAFETY_STDOUT, SAFETY_STDERR}
    if not allow_missing_final_safety and not final_safety.issubset(files):
        common.fail("final archive safety evidence missing")

    marker = parse_marker(files[COMMIT_MARKER])
    common.require(marker["stage"], STAGE, "marker stage")
    common.require(marker["source_branch"], BRANCH, "marker branch")
    common.require(marker["stage5g_a_predecessor"], PREDECESSOR, "marker predecessor")
    if not HEX40.fullmatch(marker["source_ref"]):
        common.fail("invalid marker source ref")
    common.require(marker["source_commit"], marker["source_ref"][:7], "marker short ref")
    common.require(marker["archive_name"], archive_path.name, "marker archive name")
    source_paths, _ = validate_source(files, marker)
    validate_evidence(files, marker)

    generated = {
        COMMIT_MARKER,
        COMMIT_OBJECT,
        SOURCE_MANIFEST,
        EVIDENCE_MANIFEST,
        *final_safety.intersection(files),
    }
    generated.update(name for name in files if name.startswith("stage5g-b-evidence/"))
    unexpected = set(files) - source_paths - generated
    if unexpected:
        common.fail(f"unexpected archive members: {sorted(unexpected)}")
    if not allow_missing_final_safety:
        safety = common.decode_json(files[SAFETY_RESULT], SAFETY_RESULT)
        common.require(safety.get("stage"), STAGE, "safety result stage")
        common.require(safety.get("source_ref"), marker["source_ref"], "safety result source")
        common.require(safety.get("archive_name"), archive_path.name, "safety result archive")
        common.require(safety.get("verdict"), "PASS", "safety verdict")
        common.require(safety.get("preseal_exit_code"), 0, "preseal exit")
    return marker, len(files)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--allow-missing-final-safety", action="store_true")
    parser.add_argument("--result-out", type=Path)
    args = parser.parse_args()
    try:
        marker, member_count = validate_archive(
            args.archive,
            allow_missing_final_safety=args.allow_missing_final_safety,
        )
    except common.SafetyFailure as exc:
        print(f"stage5g-b-handoff-safety: failed: {exc}", file=sys.stderr)
        return 1
    result = {
        "schema_version": 1,
        "stage": STAGE,
        "source_ref": marker["source_ref"],
        "archive_name": args.archive.name,
        "preseal_exit_code": 0,
        "member_count_before_final_safety": member_count,
        "verdict": "PASS",
    }
    if args.result_out is not None:
        args.result_out.write_text(
            json.dumps(result, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    print(
        "stage5g-b-handoff-safety: ok "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
