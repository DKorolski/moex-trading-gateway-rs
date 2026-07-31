#!/usr/bin/env python3
"""Self-contained safety verifier for a Stage 5F-c R3 review archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
from datetime import datetime
from pathlib import PurePosixPath
from typing import Any


STAGE = "5F-c-R3-runtime-consumed-reachability-closure"
SOURCE_MANIFEST = "stage5f-r3-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5f-r3-evidence-manifest.json"
COMMIT_MARKER = "handoff-commit.txt"
COMMIT_OBJECT = "stage5f-r3-commit-object.txt"
SAFETY_RESULT = "stage5f-r3-archive-safety-result.json"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

EXPECTED_COMMANDS = {
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
    "controlled-checker": [
        "python3",
        "scripts/stage5f_controlled_characterization_check.py",
    ],
    "controlled-negative": [
        "python3",
        "scripts/stage5f_controlled_characterization_negative_harness.py",
    ],
    "reachability-checker": [
        "python3",
        "scripts/stage5f_source_reachability_check.py",
    ],
    "reachability-r2-negative": [
        "python3",
        "scripts/stage5f_source_reachability_negative_harness.py",
        "--r2-compat",
    ],
    "reachability-negative": [
        "python3",
        "scripts/stage5f_source_reachability_negative_harness.py",
    ],
    "seed-matrix": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_v2_all_state_seeds_roundtrip_exact",
        "--",
        "--nocapture",
    ],
    "f19-paired": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_f19_mr_owner_suppresses_paired_source_valid_bo_candidate",
        "--",
        "--nocapture",
    ],
    "f26-chain": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_f26_working_order_reaches_runtime_and_retains_stale_pending",
        "--",
        "--nocapture",
    ],
    "determinism": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_v2_candidate_repeat_is_byte_identical",
        "--",
        "--nocapture",
    ],
    "inherited-b1": ["bash", "scripts/stage5f_inherited_b1_snapshot_gate.sh"],
    "inherited-b3f": ["bash", "scripts/stage5f_b3f_snapshot_provenance_gate.sh"],
}

EXPECTED_CLOSED_SURFACES = {
    "redis": False,
    "finam_transport": False,
    "http_post_delete": False,
    "dispatch": False,
    "broker_execution": False,
    "runtime_live": False,
    "feedback_lifecycle": False,
    "protective_orders": False,
    "stage5f_d": False,
}


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
    framed = f"{kind} {len(raw)}\0".encode() + raw
    return hashlib.sha1(framed).hexdigest()


def parse_utc(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be a UTC timestamp")
    try:
        return datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        fail(f"{label} is invalid: {exc}")


def validate_member_name(name: str) -> None:
    path = PurePosixPath(name)
    if (
        not name
        or name.startswith("/")
        or "\\" in name
        or any(part in {"", ".", ".."} for part in path.parts)
    ):
        fail(f"unsafe archive member: {name!r}")
    parts = path.parts
    basename = parts[-1]
    if any(part in {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"} for part in parts):
        fail(f"forbidden archive path: {name}")
    if basename == ".env" or (basename.startswith(".env.") and basename != ".env.example"):
        fail(f"secret-bearing env member is forbidden: {name}")
    if basename == ".DS_Store" or basename.endswith(".log") or ".local." in basename:
        fail(f"local artifact is forbidden: {name}")


def tree_oid(entries: list[dict[str, str]], payloads: dict[str, bytes]) -> str:
    root: dict[str, Any] = {}
    for entry in entries:
        path = entry["path"]
        cursor = root
        parts = path.split("/")
        for part in parts[:-1]:
            cursor = cursor.setdefault(part, {})
            if not isinstance(cursor, dict):
                fail(f"source tree path collision: {path}")
        if parts[-1] in cursor:
            fail(f"duplicate source tree path: {path}")
        cursor[parts[-1]] = (entry["git_mode"], payloads[path])

    def encode(node: dict[str, Any]) -> str:
        encoded_entries: list[tuple[bytes, bytes]] = []
        for name, value in node.items():
            if isinstance(value, dict):
                mode = "40000"
                oid = encode(value)
                sort_key = name.encode() + b"/"
            else:
                mode, raw = value
                oid = git_object_id("blob", raw)
                sort_key = name.encode() + b"\0"
            body = mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(oid)
            encoded_entries.append((sort_key, body))
        payload = b"".join(body for _, body in sorted(encoded_entries))
        return git_object_id("tree", payload)

    return encode(root)


def validate_archive(archive: str, allow_missing_final_safety: bool) -> tuple[str, int]:
    try:
        handle = zipfile.ZipFile(archive)
    except (OSError, zipfile.BadZipFile) as exc:
        fail(f"cannot open archive: {exc}")
    with handle:
        infos = handle.infolist()
        names = [info.filename for info in infos]
        if len(names) != len(set(names)):
            fail("archive contains duplicate members")
        for info in infos:
            validate_member_name(info.filename.rstrip("/"))
            unix_type = (info.external_attr >> 16) & 0o170000
            if unix_type not in {0, 0o040000, 0o100000}:
                fail(f"archive contains symlink or special member: {info.filename}")
        files = {info.filename: handle.read(info) for info in infos if not info.is_dir()}

    for required in (COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST):
        if required not in files:
            fail(f"required handoff member missing: {required}")
    if not allow_missing_final_safety and SAFETY_RESULT not in files:
        fail(f"required handoff member missing: {SAFETY_RESULT}")

    marker_lines = files[COMMIT_MARKER].decode().splitlines()
    marker = dict(line.split("=", 1) for line in marker_lines if "=" in line)
    exact_keys(marker, {"archive_name", "parent_ref", "source_commit", "source_ref"}, "commit marker")
    source_ref = marker["source_ref"]
    if not HEX40.fullmatch(source_ref):
        fail("source_ref must be a full SHA-1 commit id")
    if not HEX40.fullmatch(marker["parent_ref"]):
        fail("parent_ref must be a full SHA-1 commit id")
    require(marker["source_commit"], source_ref[:7], "short/full commit binding")
    require(marker["archive_name"], PurePosixPath(archive).name, "archive name binding")

    source_manifest = decode_json(files[SOURCE_MANIFEST], SOURCE_MANIFEST)
    exact_keys(
        source_manifest,
        {"head_tree", "members", "parent_ref", "schema_version", "source_commit", "source_ref", "stage"},
        "source-tree manifest",
    )
    require(source_manifest["schema_version"], 1, "source manifest schema")
    require(source_manifest["stage"], STAGE, "source manifest stage")
    require(source_manifest["source_ref"], source_ref, "source manifest ref")
    require(source_manifest["parent_ref"], marker["parent_ref"], "source manifest parent")
    require(source_manifest["source_commit"], source_ref[:7], "source manifest short ref")
    if not isinstance(source_manifest["head_tree"], str) or not HEX40.fullmatch(source_manifest["head_tree"]):
        fail("source manifest head_tree must be SHA-1")
    entries = source_manifest["members"]
    if not isinstance(entries, list) or not entries:
        fail("source manifest members must be non-empty")
    source_paths: list[str] = []
    source_payloads: dict[str, bytes] = {}
    for index, raw_entry in enumerate(entries):
        entry = exact_keys(raw_entry, {"git_mode", "path", "sha256"}, f"source member {index}")
        path = entry["path"]
        if not isinstance(path, str):
            fail(f"source member {index} path must be a string")
        validate_member_name(path)
        if entry["git_mode"] not in {"100644", "100755"}:
            fail(f"unsupported source git mode: {entry['git_mode']}")
        if not isinstance(entry["sha256"], str) or not HEX64.fullmatch(entry["sha256"]):
            fail(f"invalid source member hash: {path}")
        if path not in files:
            fail(f"source member absent from archive: {path}")
        require(sha256(files[path]), entry["sha256"], f"source member hash {path}")
        source_paths.append(path)
        source_payloads[path] = files[path]
    require(source_paths, sorted(source_paths), "source member order")
    if len(source_paths) != len(set(source_paths)):
        fail("source manifest has duplicate paths")
    require(tree_oid(entries, source_payloads), source_manifest["head_tree"], "recomputed Git tree")

    commit_raw = files[COMMIT_OBJECT]
    require(git_object_id("commit", commit_raw), source_ref, "commit object id")
    commit_lines = commit_raw.decode().splitlines()
    tree_lines = [line.split(" ", 1)[1] for line in commit_lines if line.startswith("tree ")]
    require(tree_lines, [source_manifest["head_tree"]], "commit/tree binding")
    parent_lines = [line.split(" ", 1)[1] for line in commit_lines if line.startswith("parent ")]
    require(parent_lines, [marker["parent_ref"]], "commit/parent binding")

    evidence = decode_json(files[EVIDENCE_MANIFEST], EVIDENCE_MANIFEST)
    exact_keys(
        evidence,
        {
            "archive_name",
            "cargo_version",
            "closed_surfaces",
            "commit_object_sha256",
            "gates",
            "head_tree",
            "parent_ref",
            "rustc_version",
            "schema_version",
            "source_commit",
            "source_ref",
            "source_tree_manifest_sha256",
            "stage",
            "status",
        },
        "evidence manifest",
    )
    require(evidence["schema_version"], 1, "evidence schema")
    require(evidence["stage"], STAGE, "evidence stage")
    require(evidence["status"], "review_required_before_5f_d", "evidence status")
    require(evidence["source_ref"], source_ref, "evidence source ref")
    require(evidence["source_commit"], source_ref[:7], "evidence short ref")
    require(evidence["head_tree"], source_manifest["head_tree"], "evidence tree")
    require(evidence["parent_ref"], marker["parent_ref"], "evidence parent")
    require(evidence["archive_name"], marker["archive_name"], "evidence archive name")
    require(evidence["closed_surfaces"], EXPECTED_CLOSED_SURFACES, "closed surfaces")
    require(evidence["source_tree_manifest_sha256"], sha256(files[SOURCE_MANIFEST]), "source manifest evidence hash")
    require(evidence["commit_object_sha256"], sha256(commit_raw), "commit object evidence hash")
    for tool in ("cargo_version", "rustc_version"):
        if not isinstance(evidence[tool], str) or not evidence[tool].strip():
            fail(f"{tool} is missing")

    gates = evidence["gates"]
    if not isinstance(gates, list):
        fail("evidence gates must be an array")
    require([gate.get("label") for gate in gates], list(EXPECTED_COMMANDS), "gate order")
    generated = {COMMIT_MARKER, COMMIT_OBJECT, SOURCE_MANIFEST, EVIDENCE_MANIFEST}
    for gate_binding in gates:
        binding = exact_keys(gate_binding, {"label", "result_member", "result_sha256"}, "gate binding")
        label = binding["label"]
        result_member = binding["result_member"]
        if result_member not in files:
            fail(f"gate result missing: {result_member}")
        require(sha256(files[result_member]), binding["result_sha256"], f"gate result hash {label}")
        result = decode_json(files[result_member], result_member)
        exact_keys(
            result,
            {
                "command",
                "exit_code",
                "finished_at_utc",
                "label",
                "schema_version",
                "source_ref",
                "stage",
                "started_at_utc",
                "stderr_member",
                "stderr_sha256",
                "stdout_member",
                "stdout_sha256",
            },
            f"gate result {label}",
        )
        require(result["schema_version"], 1, f"{label} schema")
        require(result["stage"], STAGE, f"{label} stage")
        require(result["label"], label, f"{label} result label")
        require(result["source_ref"], source_ref, f"{label} source ref")
        require(result["command"], EXPECTED_COMMANDS[label], f"{label} command")
        require(result["exit_code"], 0, f"{label} exit code")
        started = parse_utc(result["started_at_utc"], f"{label} started")
        finished = parse_utc(result["finished_at_utc"], f"{label} finished")
        if finished < started:
            fail(f"{label} timestamps are reversed")
        for stream in ("stdout", "stderr"):
            member = result[f"{stream}_member"]
            if member not in files:
                fail(f"{label} {stream} member missing")
            require(sha256(files[member]), result[f"{stream}_sha256"], f"{label} {stream} hash")
            generated.add(member)
        generated.add(result_member)

    negative_stdout = files["stage5f-r3-controlled-negative-stdout.txt"].decode(errors="replace")
    require(sum(line.startswith("PASS ") for line in negative_stdout.splitlines()), 51, "inherited R1 negative PASS count")
    reachability_negative_stdout = files[
        "stage5f-r3-reachability-negative-stdout.txt"
    ].decode(errors="replace")
    require(
        sum(line.startswith("PASS ") for line in reachability_negative_stdout.splitlines()),
        45,
        "R3 reachability negative PASS count",
    )
    r2_negative_stdout = files[
        "stage5f-r3-reachability-r2-negative-stdout.txt"
    ].decode(errors="replace")
    require(
        sum(line.startswith("PASS ") for line in r2_negative_stdout.splitlines()),
        27,
        "R2 reachability negative PASS count",
    )
    f19_stdout = files["stage5f-r3-f19-paired-stdout.txt"].decode(errors="replace")
    if "stage5f_f19_mr_owner_suppresses_paired_source_valid_bo_candidate ... ok" not in f19_stdout:
        fail("F19 paired source-valid counterfactual marker missing")
    f26_stdout = files["stage5f-r3-f26-chain-stdout.txt"].decode(errors="replace")
    if "stage5f_f26_working_order_reaches_runtime_and_retains_stale_pending ... ok" not in f26_stdout:
        fail("F26 broker-truth/runtime/restart chain marker missing")
    b1_stdout = files["stage5f-r3-inherited-b1-stdout.txt"].decode(errors="replace")
    if "stage5f-inherited-b1-snapshot-gate: ok" not in b1_stdout:
        fail("inherited B1 closure marker missing")
    b3f_stdout = files["stage5f-r3-inherited-b3f-stdout.txt"].decode(errors="replace")
    if "stage5f-b3f-snapshot-provenance-gate: ok" not in b3f_stdout:
        fail("inherited B3F closure marker missing")

    if SAFETY_RESULT in files:
        safety = decode_json(files[SAFETY_RESULT], SAFETY_RESULT)
        exact_keys(
            safety,
            {
                "checked_evidence_manifest_sha256",
                "checked_source_tree_manifest_sha256",
                "command",
                "finished_at_utc",
                "preseal_exit_code",
                "schema_version",
                "source_ref",
                "stage",
                "started_at_utc",
                "stderr_member",
                "stderr_sha256",
                "stdout_member",
                "stdout_sha256",
            },
            "archive safety result",
        )
        require(safety["schema_version"], 1, "safety schema")
        require(safety["stage"], STAGE, "safety stage")
        require(safety["source_ref"], source_ref, "safety source ref")
        require(safety["preseal_exit_code"], 0, "preseal exit")
        require(
            safety["command"],
            [
                "python3",
                "scripts/stage5f_c_r3_handoff_safety_check.py",
                "--archive",
                marker["archive_name"],
                "--allow-missing-final-safety",
            ],
            "safety command",
        )
        started = parse_utc(safety["started_at_utc"], "safety started")
        finished = parse_utc(safety["finished_at_utc"], "safety finished")
        if finished < started:
            fail("safety timestamps are reversed")
        for stream in ("stdout", "stderr"):
            member = safety[f"{stream}_member"]
            if member not in files:
                fail(f"safety {stream} member missing")
            require(
                sha256(files[member]),
                safety[f"{stream}_sha256"],
                f"safety {stream} hash",
            )
            generated.add(member)
        safety_stdout = files[safety["stdout_member"]].decode(errors="replace")
        if f"stage5f-c-r3-handoff-safety: ok source_ref={source_ref} gates=15" not in safety_stdout:
            fail("preseal archive-safety success marker missing")
        require(safety["checked_evidence_manifest_sha256"], sha256(files[EVIDENCE_MANIFEST]), "safety evidence binding")
        require(safety["checked_source_tree_manifest_sha256"], sha256(files[SOURCE_MANIFEST]), "safety source binding")
        generated.add(SAFETY_RESULT)

    expected_files = set(source_paths).union(generated)
    require(set(files), expected_files, "archive member inventory")
    return source_ref, len(gates)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True)
    parser.add_argument("--allow-missing-final-safety", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        source_ref, gate_count = validate_archive(args.archive, args.allow_missing_final_safety)
    except (SafetyFailure, OSError, UnicodeDecodeError, ValueError) as exc:
        print(f"stage5f-c-r3-handoff-safety: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"stage5f-c-r3-handoff-safety: ok source_ref={source_ref} gates={gate_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
