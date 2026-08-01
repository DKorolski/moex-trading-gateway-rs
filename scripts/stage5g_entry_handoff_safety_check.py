#!/usr/bin/env python3
"""Self-contained verifier for a commit-bound Stage 5G-a review handoff."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any


STAGE = "5G-a-lifecycle-entry"
BRANCH = "stage5g-lifecycle"
ACCEPTED_STAGE5F = "fb8245e2f91cfc1678548a1228e8558d9adc2181"
CLOSURE_COMMIT = "cac83da38725aeadd6d029a3078157c2ab7fa004"
SOURCE_MANIFEST = "stage5g-a-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5g-a-evidence-manifest.json"
COMMIT_OBJECT = "stage5g-a-commit-object.txt"
COMMIT_MARKER = "handoff-commit.txt"
SAFETY_RESULT = "stage5g-a-archive-safety-result.json"
SAFETY_STDOUT = "stage5g-a-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5g-a-archive-safety.stderr.txt"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "entry-checker": ["python3", "scripts/stage5g_entry_plan_check.py"],
    "entry-negative": ["python3", "scripts/stage5g_entry_plan_negative_harness.py"],
    "fmt": ["cargo", "fmt", "--all", "--", "--check"],
    "workspace-tests": ["cargo", "test", "--workspace", "--all-targets"],
    "doctests": ["cargo", "test", "--workspace", "--doc"],
    "clippy": [
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
}

REQUIRED_SOURCE_FILES = {
    "docs/stage-5/stage5f-closure-descriptor.json",
    "docs/stage-5/5g-lifecycle-design-and-implementation-plan.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "docs/adr/adr-stage5g-paper-mock-development-governance.md",
    "scripts/stage5g_entry_plan_check.py",
    "scripts/stage5g_entry_plan_negative_harness.py",
    "scripts/make_stage5g_entry_handoff_archive.py",
    "scripts/stage5g_entry_handoff_safety_check.py",
}

EXPECTED_CHANGED_PATHS = [
    "docs/adr/adr-stage5g-paper-mock-development-governance.md",
    "docs/current-status.md",
    "docs/stage-5/5g-lifecycle-design-and-implementation-plan.md",
    "docs/stage-5/stage5g-lifecycle-entry-inventory.json",
    "scripts/make_stage5g_entry_handoff_archive.py",
    "scripts/stage5g_entry_handoff_safety_check.py",
    "scripts/stage5g_entry_plan_check.py",
    "scripts/stage5g_entry_plan_negative_harness.py",
]


class SafetyFailure(RuntimeError):
    pass


def fail(message: str) -> None:
    raise SafetyFailure(message)


def strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            fail(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def decode_json(raw: bytes, label: str) -> dict[str, Any]:
    try:
        value = json.loads(raw, object_pairs_hook=strict_object)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {label}: {exc}")
    if not isinstance(value, dict):
        fail(f"{label} must contain an object")
    return value


def exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != expected:
        fail(f"{label} key-set drift")
    return value


def require(actual: object, expected: object, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def git_object_id(kind: str, raw: bytes) -> str:
    return hashlib.sha1(f"{kind} {len(raw)}\0".encode() + raw).hexdigest()


def validate_member_name(name: str) -> None:
    path = PurePosixPath(name)
    if (
        not name
        or name.startswith("/")
        or "\\" in name
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        fail(f"unsafe archive member: {name!r}")
    if any(
        part in {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
        for part in path.parts
    ):
        fail(f"forbidden archive path: {name}")
    basename = path.name
    if basename == ".env" or (
        basename.startswith(".env.") and basename != ".env.example"
    ):
        fail(f"secret-bearing env member is forbidden: {name}")
    if basename in {".DS_Store"} or basename.endswith(".log") or ".local." in basename:
        fail(f"local artifact is forbidden: {name}")


def parse_marker(raw: bytes) -> dict[str, str]:
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        fail(f"handoff marker is not UTF-8: {exc}")
    result: dict[str, str] = {}
    for line in text.splitlines():
        if not line or "=" not in line:
            fail("invalid handoff marker line")
        key, value = line.split("=", 1)
        if key in result or not key or not value:
            fail("invalid or duplicate handoff marker key")
        result[key] = value
    exact_keys(
        result,
        {
            "stage",
            "source_ref",
            "source_commit",
            "source_branch",
            "archive_name",
            "stage5f_predecessor",
            "stage5f_closure_commit",
        },
        "handoff marker",
    )
    return result


def build_tree_oid(entries: list[dict[str, str]], payloads: dict[str, bytes]) -> str:
    root: dict[str, Any] = {}
    for entry in entries:
        cursor = root
        parts = entry["path"].split("/")
        for part in parts[:-1]:
            child = cursor.setdefault(part, {})
            if not isinstance(child, dict):
                fail(f"source tree collision at {entry['path']}")
            cursor = child
        if parts[-1] in cursor:
            fail(f"duplicate source path: {entry['path']}")
        cursor[parts[-1]] = (entry["git_mode"], payloads[entry["path"]])

    def encode(node: dict[str, Any]) -> str:
        records: list[tuple[bytes, bytes]] = []
        for name, value in node.items():
            if isinstance(value, dict):
                mode = "40000"
                oid = encode(value)
                sort_key = name.encode() + b"/"
            else:
                mode, body = value
                oid = git_object_id("blob", body)
                sort_key = name.encode() + b"\0"
            record = mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(oid)
            records.append((sort_key, record))
        payload = b"".join(record for _, record in sorted(records))
        return git_object_id("tree", payload)

    return encode(root)


def validate_source(
    files: dict[str, bytes], marker: dict[str, str]
) -> tuple[dict[str, Any], set[str]]:
    manifest = decode_json(files[SOURCE_MANIFEST], SOURCE_MANIFEST)
    exact_keys(
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
    require(manifest["schema_version"], 1, "source manifest schema")
    require(manifest["stage"], STAGE, "source manifest stage")
    require(manifest["source_ref"], marker["source_ref"], "source manifest ref")
    require(manifest["source_commit"], marker["source_commit"], "source manifest short ref")
    require(manifest["source_branch"], BRANCH, "source branch")
    if not HEX40.fullmatch(manifest["parent_ref"]):
        fail("source parent ref is invalid")
    if not HEX40.fullmatch(manifest["head_tree"]):
        fail("source tree id is invalid")
    members = manifest["members"]
    if not isinstance(members, list) or not members:
        fail("source manifest members missing")
    source_paths: set[str] = set()
    payloads: dict[str, bytes] = {}
    normalized: list[dict[str, str]] = []
    for index, item in enumerate(members):
        member = exact_keys(item, {"git_mode", "path", "sha256"}, f"source member[{index}]")
        if member["git_mode"] not in {"100644", "100755"}:
            fail(f"unsupported source mode: {member['git_mode']}")
        validate_member_name(member["path"])
        if member["path"] in source_paths:
            fail(f"duplicate source path: {member['path']}")
        if not HEX64.fullmatch(member["sha256"]):
            fail(f"invalid source SHA-256: {member['path']}")
        if member["path"] not in files:
            fail(f"source member missing from archive: {member['path']}")
        body = files[member["path"]]
        require(sha256(body), member["sha256"], f"source content {member['path']}")
        source_paths.add(member["path"])
        payloads[member["path"]] = body
        normalized.append(member)
    if not REQUIRED_SOURCE_FILES.issubset(source_paths):
        fail(f"required source files missing: {sorted(REQUIRED_SOURCE_FILES - source_paths)}")
    require(build_tree_oid(normalized, payloads), manifest["head_tree"], "source tree oid")

    commit_raw = files[COMMIT_OBJECT]
    require(git_object_id("commit", commit_raw), marker["source_ref"], "commit object id")
    first_line = commit_raw.decode("utf-8", errors="strict").splitlines()[0]
    require(first_line, f"tree {manifest['head_tree']}", "commit tree binding")
    if f"parent {manifest['parent_ref']}" not in commit_raw.decode("utf-8", errors="strict").splitlines():
        fail("commit parent binding missing")
    return manifest, source_paths


def validate_evidence(files: dict[str, bytes], marker: dict[str, str]) -> None:
    manifest = decode_json(files[EVIDENCE_MANIFEST], EVIDENCE_MANIFEST)
    exact_keys(
        manifest,
        {
            "schema_version",
            "stage",
            "source_ref",
            "source_branch",
            "stage5f_predecessor",
            "stage5f_closure_commit",
            "gate_count",
            "gates",
            "repository_state",
            "closed_surfaces",
        },
        "evidence manifest",
    )
    require(manifest["schema_version"], 1, "evidence schema")
    require(manifest["stage"], STAGE, "evidence stage")
    require(manifest["source_ref"], marker["source_ref"], "evidence source ref")
    require(manifest["source_branch"], BRANCH, "evidence source branch")
    require(manifest["stage5f_predecessor"], ACCEPTED_STAGE5F, "evidence predecessor")
    require(manifest["stage5f_closure_commit"], CLOSURE_COMMIT, "evidence closure")
    repository_state = exact_keys(
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
    require(repository_state["git_status_clean"], True, "repository clean state")
    require(repository_state["changed_paths_base_ref"], CLOSURE_COMMIT, "changed paths base")
    for key in ("git_status", "changed_paths"):
        member_name = repository_state[f"{key}_member"]
        if member_name not in files:
            fail(f"repository-state member missing: {member_name}")
        require(
            sha256(files[member_name]),
            repository_state[f"{key}_sha256"],
            f"repository-state hash {key}",
        )
    require(files[repository_state["git_status_member"]], b"", "embedded git status")
    changed_paths = files[repository_state["changed_paths_member"]].decode("utf-8").splitlines()
    require(changed_paths, EXPECTED_CHANGED_PATHS, "embedded changed paths")
    gates = manifest["gates"]
    if not isinstance(gates, list):
        fail("evidence gates must be an array")
    require(manifest["gate_count"], len(EXPECTED_COMMANDS), "evidence gate count")
    require(len(gates), len(EXPECTED_COMMANDS), "evidence gate array count")
    seen: set[str] = set()
    for index, item in enumerate(gates):
        gate = exact_keys(
            item,
            {"label", "result_member", "result_sha256"},
            f"gate[{index}]",
        )
        label = gate["label"]
        if label in seen or label not in EXPECTED_COMMANDS:
            fail(f"unknown or duplicate gate: {label}")
        seen.add(label)
        result_member = gate["result_member"]
        if result_member not in files:
            fail(f"gate result missing: {result_member}")
        require(sha256(files[result_member]), gate["result_sha256"], f"gate result hash {label}")
        result = decode_json(files[result_member], result_member)
        exact_keys(
            result,
            {
                "schema_version",
                "stage",
                "label",
                "command",
                "source_ref",
                "exit_code",
                "stdout_member",
                "stdout_sha256",
                "stderr_member",
                "stderr_sha256",
            },
            f"gate result {label}",
        )
        require(result["stage"], STAGE, f"gate stage {label}")
        require(result["label"], label, f"gate label {label}")
        require(result["command"], EXPECTED_COMMANDS[label], f"gate command {label}")
        require(result["source_ref"], marker["source_ref"], f"gate source {label}")
        require(result["exit_code"], 0, f"gate exit code {label}")
        for stream in ("stdout", "stderr"):
            member_name = result[f"{stream}_member"]
            if member_name not in files:
                fail(f"gate {stream} missing: {label}")
            require(sha256(files[member_name]), result[f"{stream}_sha256"], f"gate {stream} hash {label}")
        stdout = files[result["stdout_member"]].decode(errors="replace")
        if label == "entry-checker" and "stage5g-entry-plan-check: ok cases=54" not in stdout:
            fail("entry checker success marker missing")
        if label == "entry-negative":
            pass_count = sum(line.startswith("PASS ") for line in stdout.splitlines())
            require(pass_count, 30, "entry negative PASS count")
            if "stage5g-entry-plan-negative-harness: ok cases=30" not in stdout:
                fail("entry negative completion marker missing")
    require(seen, set(EXPECTED_COMMANDS), "evidence gate labels")
    closed = manifest["closed_surfaces"]
    if not isinstance(closed, dict) or not closed or any(value is not False for value in closed.values()):
        fail("all Stage 5G-a execution surfaces must remain false")


def validate_archive(
    archive_path: Path,
    *,
    allow_missing_final_safety: bool,
) -> tuple[dict[str, Any], int]:
    try:
        with zipfile.ZipFile(archive_path) as archive:
            infos = archive.infolist()
            names = [info.filename for info in infos]
            if len(names) != len(set(names)):
                fail("archive contains duplicate members")
            for info in infos:
                name = info.filename.rstrip("/")
                if name:
                    validate_member_name(name)
                unix_type = (info.external_attr >> 16) & 0o170000
                if unix_type not in {0, 0o040000, 0o100000}:
                    fail(f"archive contains symlink or special member: {info.filename}")
            files = {
                info.filename: archive.read(info)
                for info in infos
                if not info.is_dir()
            }
    except (OSError, zipfile.BadZipFile) as exc:
        fail(f"cannot open archive: {exc}")

    required = {COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST}
    if not required.issubset(files):
        fail(f"required generated members missing: {sorted(required - set(files))}")
    final_safety = {SAFETY_RESULT, SAFETY_STDOUT, SAFETY_STDERR}
    if not allow_missing_final_safety and not final_safety.issubset(files):
        fail("final archive safety evidence missing")

    marker = parse_marker(files[COMMIT_MARKER])
    require(marker["stage"], STAGE, "marker stage")
    require(marker["source_branch"], BRANCH, "marker branch")
    require(marker["stage5f_predecessor"], ACCEPTED_STAGE5F, "marker predecessor")
    require(marker["stage5f_closure_commit"], CLOSURE_COMMIT, "marker closure")
    if not HEX40.fullmatch(marker["source_ref"]):
        fail("marker source_ref is invalid")
    require(marker["source_commit"], marker["source_ref"][:7], "marker short ref")
    require(marker["archive_name"], archive_path.name, "marker archive name")
    _, source_paths = validate_source(files, marker)
    validate_evidence(files, marker)

    generated = {
        COMMIT_MARKER,
        COMMIT_OBJECT,
        SOURCE_MANIFEST,
        EVIDENCE_MANIFEST,
        *final_safety.intersection(files),
    }
    generated.update(
        name for name in files if name.startswith("stage5g-a-evidence/")
    )
    unexpected = set(files) - source_paths - generated
    if unexpected:
        fail(f"unexpected archive members: {sorted(unexpected)}")

    if not allow_missing_final_safety:
        result = decode_json(files[SAFETY_RESULT], SAFETY_RESULT)
        require(result.get("stage"), STAGE, "safety result stage")
        require(result.get("source_ref"), marker["source_ref"], "safety result source")
        require(result.get("archive_name"), archive_path.name, "safety result archive")
        require(result.get("verdict"), "PASS", "safety result verdict")
        require(result.get("preseal_exit_code"), 0, "preseal exit code")
    return marker, len(files)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


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
    except SafetyFailure as exc:
        print(f"stage5g-entry-handoff-safety: failed: {exc}", file=sys.stderr)
        return 1
    result = {
        "schema_version": 1,
        "stage": STAGE,
        "source_ref": marker["source_ref"],
        "archive_name": args.archive.name,
        "checked_at_utc": utc_now(),
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
        "stage5g-entry-handoff-safety: ok "
        f"source_ref={marker['source_ref']} members={member_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
