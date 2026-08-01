#!/usr/bin/env python3
"""Verify the self-attesting Stage 5G-c R2-c-a R1 handoff archive."""

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

STAGE = "5G-c-R2-c-a-R1-market-terminal-state-coherence"
BRANCH = "stage5g-lifecycle"
BASE_REF = "581f4f6021dd781e7a5db9177be05feb7d94b12a"
SOURCE_MANIFEST = "stage5g-c-r2ca-r1-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-c-r2ca-r1-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-c-r2ca-r1-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-c-r2ca-r1-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-c-r2ca-r1-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-c-r2ca-r1-archive-safety.stderr.txt"
EVIDENCE_PREFIX = "stage5g-c-r2ca-r1-evidence/"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "authority-check": ["python3", "scripts/stage5g_c_r2ca_r1_authority_check.py"],
    "snapshot-gate": ["python3", "scripts/stage5g_c_r2ca_r1_snapshot_gate.py"],
    "authority-negative": [
        "python3", "scripts/stage5g_c_r2ca_r1_authority_negative_harness.py"
    ],
    "semantic-negative": [
        "python3", "scripts/stage5g_c_r2ca_r1_semantic_negative_harness.py"
    ],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "focused-debug": [
        "cargo", "test", "-p", "strategy-runtime-core", "stage5g_r2ca", "--quiet"
    ],
    "focused-release": [
        "cargo", "test", "-p", "strategy-runtime-core", "--release",
        "stage5g_r2ca", "--quiet"
    ],
    "source-ack-mapping": [
        "cargo", "test", "-p", "broker-core",
        "trading_window_closed_ack_preserves_confirmed_and_deferred_semantics", "--quiet"
    ],
    "stage5g-b-source-path": [
        "cargo", "test", "-p", "strategy-runtime-core",
        "production_public_submitted_then_recovered_resolves_stage5c_once", "--quiet"
    ],
    "stage5c-api-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets", "--quiet"],
    "doctests": ["cargo", "test", "--workspace", "--doc", "--quiet"],
    "clippy": [
        "cargo", "clippy", "--workspace", "--all-targets", "--all-features",
        "--quiet", "--", "-D", "warnings"
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}

EXPECTED_CHANGED_PATHS = sorted(
    {
        "crates/strategy-runtime-core/src/stage5c_paper_host.rs",
        "docs/adr/adr-stage5g-c-r2ca-market-terminal-no-callback-authority.md",
        "docs/adr/adr-stage5g-c-r2ca-r1-market-terminal-state-coherence.md",
        "docs/current-status.md",
        "docs/stage-5/stage5g-c-r2ca-market-terminal-authority.json",
        "docs/stage-5/stage5g-c-r2ca-r1-market-terminal-state-coherence.json",
        "scripts/stage5g_c_r2ca_r1_authority_check.py",
        "scripts/stage5g_c_r2ca_r1_authority_gate.sh",
        "scripts/stage5g_c_r2ca_r1_authority_negative_harness.py",
        "scripts/stage5g_c_r2ca_r1_semantic_negative_harness.py",
        "scripts/stage5g_c_r2ca_r1_snapshot_gate.py",
        "scripts/stage5g_c_r2ca_r1_handoff_safety_check.py",
        "scripts/make_stage5g_c_r2ca_r1_handoff_archive.py",
    }
)
REQUIRED_SOURCE_FILES = set(EXPECTED_CHANGED_PATHS) | {
    "crates/strategy-runtime-core/src/stage5g_mock_ack.rs",
    "crates/broker-core/src/hybrid_strategy_boundary.rs",
    "scripts/stage5g_c_r2a_authority_check.py",
    "scripts/stage5c_api_freeze_check.py",
    "scripts/stage5f_forbidden_no_rg_gate.sh",
}
CLOSED_SURFACES = {
    "stage5g_c_r2cb",
    "stage5g_d",
    "redis_live_consumer_groups",
    "finam_transport",
    "http_post_delete",
    "broker_dispatch_execution",
    "runtime_live",
    "real_orders",
    "stage6",
    "main_merge",
    "deployment",
}


def parse_marker(raw: bytes) -> dict[str, str]:
    try:
        lines = raw.decode("utf-8").splitlines()
    except UnicodeDecodeError as error:
        common.fail(f"handoff marker is not UTF-8: {error}")
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
            "stage", "source_ref", "source_commit", "source_branch", "archive_name",
            "parent_ref", "origin_ref",
        },
        "handoff marker",
    )
    return marker


def validate_source(files: dict[str, bytes], marker: dict[str, str]) -> set[str]:
    manifest = common.decode_json(files[SOURCE_MANIFEST], SOURCE_MANIFEST)
    common.exact_keys(
        manifest,
        {
            "schema_version", "stage", "source_ref", "source_commit", "source_branch",
            "parent_ref", "origin_ref", "head_tree", "members",
        },
        "source manifest",
    )
    common.require(manifest["schema_version"], 1, "source manifest schema")
    for key in ("stage", "source_ref", "source_commit", "source_branch", "parent_ref", "origin_ref"):
        common.require(manifest[key], marker[key], f"source manifest {key}")
    common.require(manifest["source_branch"], BRANCH, "source branch")
    common.require(manifest["parent_ref"], BASE_REF, "R1 direct parent")
    if not HEX40.fullmatch(manifest["head_tree"]):
        common.fail("invalid source tree id")

    source_paths: set[str] = set()
    payloads: dict[str, bytes] = {}
    normalized: list[dict[str, str]] = []
    members = manifest["members"]
    if not isinstance(members, list) or not members:
        common.fail("source manifest members missing")
    for index, raw_member in enumerate(members):
        member = common.exact_keys(
            raw_member, {"git_mode", "path", "sha256"}, f"source[{index}]"
        )
        if member["git_mode"] not in {"100644", "100755"}:
            common.fail(f"unsupported source mode: {member['git_mode']}")
        common.validate_member_name(member["path"])
        if member["path"] in source_paths or not HEX64.fullmatch(member["sha256"]):
            common.fail(f"invalid or duplicate source member: {member['path']}")
        if member["path"] not in files:
            common.fail(f"source member missing: {member['path']}")
        payload = files[member["path"]]
        common.require(common.sha256(payload), member["sha256"], f"source hash {member['path']}")
        source_paths.add(member["path"])
        payloads[member["path"]] = payload
        normalized.append(member)
    missing = REQUIRED_SOURCE_FILES - source_paths
    if missing:
        common.fail(f"required source files missing: {sorted(missing)}")
    common.require(common.build_tree_oid(normalized, payloads), manifest["head_tree"], "tree oid")

    commit_raw = files[COMMIT_OBJECT]
    common.require(common.git_object_id("commit", commit_raw), marker["source_ref"], "commit oid")
    commit_lines = commit_raw.decode("utf-8", errors="strict").splitlines()
    common.require(commit_lines[0], f"tree {manifest['head_tree']}", "commit tree")
    if f"parent {BASE_REF}" not in commit_lines:
        common.fail("R1 direct parent binding missing")
    return source_paths


def validate_gate_marker(label: str, stdout: str) -> None:
    markers = {
        "authority-check": "stage5g-c-r2ca-r1-authority-check: PASS",
        "snapshot-gate": "stage5g-c-r2ca-r1-snapshot-gate: PASS",
        "authority-negative": "stage5g-c-r2ca-r1-authority-negative-harness: PASS (1/1)",
        "semantic-negative": "stage5g-c-r2ca-r1-semantic-negative-harness: PASS 6/6",
        "stage5c-api-freeze": "stage5c-api-freeze-check: ok",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok",
    }
    if label in markers and markers[label] not in stdout:
        common.fail(f"gate success marker missing: {label}")


def validate_evidence(files: dict[str, bytes], marker: dict[str, str]) -> None:
    manifest = common.decode_json(files[EVIDENCE_MANIFEST], EVIDENCE_MANIFEST)
    common.exact_keys(
        manifest,
        {
            "schema_version", "stage", "source_ref", "source_branch", "parent_ref",
            "origin_ref", "gate_count", "gates", "repository_state", "closed_surfaces",
        },
        "evidence manifest",
    )
    common.require(manifest["schema_version"], 1, "evidence schema")
    for key in ("stage", "source_ref", "source_branch", "parent_ref", "origin_ref"):
        common.require(manifest[key], marker[key], f"evidence {key}")
    closed = manifest["closed_surfaces"]
    if not isinstance(closed, dict) or set(closed) != CLOSED_SURFACES:
        common.fail("closed-surface key-set drift")
    if any(value is not False for value in closed.values()):
        common.fail("closed surface opened")

    repository = common.exact_keys(
        manifest["repository_state"],
        {
            "git_status_member", "git_status_sha256", "git_status_clean",
            "changed_paths_base_ref", "changed_paths_member", "changed_paths_sha256",
        },
        "repository state",
    )
    common.require(repository["git_status_clean"], True, "repository clean")
    common.require(repository["changed_paths_base_ref"], BASE_REF, "changed paths base")
    for stem in ("git_status", "changed_paths"):
        member = repository[f"{stem}_member"]
        if member not in files:
            common.fail(f"repository evidence missing: {member}")
        common.require(
            common.sha256(files[member]), repository[f"{stem}_sha256"],
            f"repository evidence hash {stem}",
        )
    common.require(files[repository["git_status_member"]], b"", "embedded git status")
    common.require(
        files[repository["changed_paths_member"]].decode().splitlines(),
        EXPECTED_CHANGED_PATHS,
        "embedded changed paths",
    )

    gates = manifest["gates"]
    common.require(manifest["gate_count"], len(EXPECTED_COMMANDS), "gate count")
    if not isinstance(gates, list) or len(gates) != len(EXPECTED_COMMANDS):
        common.fail("gate array count mismatch")
    seen: set[str] = set()
    for index, raw_gate in enumerate(gates):
        gate = common.exact_keys(
            raw_gate, {"label", "result_member", "result_sha256"}, f"gate[{index}]"
        )
        label = gate["label"]
        if label in seen or label not in EXPECTED_COMMANDS:
            common.fail(f"unknown or duplicate gate: {label}")
        seen.add(label)
        member = gate["result_member"]
        if member not in files:
            common.fail(f"gate result missing: {member}")
        common.require(common.sha256(files[member]), gate["result_sha256"], f"gate hash {label}")
        result = common.decode_json(files[member], member)
        common.require(result.get("stage"), STAGE, f"gate stage {label}")
        common.require(result.get("label"), label, f"gate label {label}")
        common.require(result.get("command"), EXPECTED_COMMANDS[label], f"gate command {label}")
        common.require(result.get("source_ref"), marker["source_ref"], f"gate source {label}")
        common.require(result.get("exit_code"), 0, f"gate exit {label}")
        for stream in ("stdout", "stderr"):
            stream_member = result.get(f"{stream}_member")
            if stream_member not in files:
                common.fail(f"gate stream missing: {label}/{stream}")
            common.require(
                common.sha256(files[stream_member]), result.get(f"{stream}_sha256"),
                f"gate stream hash {label}/{stream}",
            )
        validate_gate_marker(label, files[result["stdout_member"]].decode(errors="replace"))
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
    except (OSError, zipfile.BadZipFile) as error:
        common.fail(f"cannot open archive: {error}")

    required = {COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST}
    if not required.issubset(files):
        common.fail(f"required generated members missing: {sorted(required - set(files))}")
    final_safety = {SAFETY_RESULT, SAFETY_STDOUT, SAFETY_STDERR}
    if not allow_missing_final_safety and not final_safety.issubset(files):
        common.fail("final archive safety evidence missing")

    marker = parse_marker(files[COMMIT_MARKER])
    common.require(marker["stage"], STAGE, "marker stage")
    common.require(marker["source_branch"], BRANCH, "marker branch")
    common.require(marker["parent_ref"], BASE_REF, "marker parent")
    common.require(marker["origin_ref"], marker["source_ref"], "origin/source binding")
    if not HEX40.fullmatch(marker["source_ref"]):
        common.fail("invalid marker source ref")
    common.require(marker["source_commit"], marker["source_ref"][:7], "marker short ref")
    common.require(marker["archive_name"], archive_path.name, "marker archive name")
    source_paths = validate_source(files, marker)
    validate_evidence(files, marker)

    generated = {COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST}
    generated.update(final_safety.intersection(files))
    generated.update(name for name in files if name.startswith(EVIDENCE_PREFIX))
    unexpected = set(files) - source_paths - generated
    if unexpected:
        common.fail(f"unexpected archive members: {sorted(unexpected)}")
    if not allow_missing_final_safety:
        result = common.decode_json(files[SAFETY_RESULT], SAFETY_RESULT)
        common.require(result.get("stage"), STAGE, "safety result stage")
        common.require(result.get("source_ref"), marker["source_ref"], "safety source")
        common.require(result.get("archive_name"), archive_path.name, "safety archive")
        common.require(result.get("verdict"), "PASS", "safety verdict")
        common.require(result.get("preseal_exit_code"), 0, "preseal exit")
    return marker, len(files)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--allow-missing-final-safety", action="store_true")
    parser.add_argument("--result-out", type=Path)
    args = parser.parse_args()
    try:
        marker, member_count = validate_archive(args.archive, args.allow_missing_final_safety)
    except common.SafetyFailure as error:
        print(f"stage5g-c-r2ca-r1-handoff-safety: FAIL: {error}", file=sys.stderr)
        return 1
    result: dict[str, Any] = {
        "schema_version": 1,
        "stage": STAGE,
        "source_ref": marker["source_ref"],
        "archive_name": args.archive.name,
        "preseal_exit_code": 0,
        "member_count_before_final_safety": member_count,
        "verdict": "PASS",
    }
    if args.result_out is not None:
        args.result_out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        "stage5g-c-r2ca-r1-handoff-safety: PASS "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
