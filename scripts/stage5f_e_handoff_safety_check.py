#!/usr/bin/env python3
"""Self-contained source, evidence and ZIP verifier for Stage 5F-e."""

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


STAGE = "5F-e-aggregate-acceptance"
ACCEPTED_D = {
    "source_ref": "1a41b530419d39ddc84fff81a9dfdde6ede878ce",
    "archive_name": "moex-trading-project-1a41b53.zip",
    "archive_sha256": "18d7944264ade10ea2f0860b861a7176ba98fe5d82c9beaf1cbcd22b72e5b2b3",
    "review_record_sha256": "3ffeb72698a472f7857b2b430ead81560c886fb77f6a4d3a64e501253b271eec",
    "verdict": "ACCEPTED",
}
SOURCE_MANIFEST = "stage5f-e-source-tree-manifest.json"
EVIDENCE_MANIFEST = "stage5f-e-evidence-manifest.json"
COMMIT_MARKER = "handoff-commit.txt"
COMMIT_OBJECT = "stage5f-e-commit-object.txt"
SAFETY_RESULT = "stage5f-e-archive-safety-result.json"
SAFETY_STDOUT = "stage5f-e-archive-safety.stdout.txt"
SAFETY_STDERR = "stage5f-e-archive-safety.stderr.txt"
REPORTS = {
    "reports/stage5f/stage5f-acceptance-result.json",
    "reports/stage5f/stage5f-fingerprint-reproducibility.json",
    "reports/stage5f/stage5f-negative-result.json",
}
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

EXPECTED_COMMANDS: dict[str, list[str]] = {
    "aggregate-checker": ["python3", "scripts/stage5f_e_aggregate_acceptance_check.py"],
    "aggregate-negative": [
        "python3",
        "scripts/stage5f_e_aggregate_acceptance_negative_harness.py",
    ],
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
    "matrix-checker": ["python3", "scripts/stage5f_d_atomic_matrix_check.py"],
    "matrix-negative": [
        "python3",
        "scripts/stage5f_d_atomic_matrix_negative_harness.py",
    ],
    "matrix-debug": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_",
        "--",
        "--test-threads=1",
    ],
    "matrix-release": [
        "cargo",
        "test",
        "--release",
        "-p",
        "strategy-runtime-core",
        "stage5f_",
        "--",
        "--test-threads=1",
    ],
    "matrix-default-parallel": [
        "cargo",
        "test",
        "-p",
        "strategy-runtime-core",
        "stage5f_",
    ],
    "matrix-reproducibility": [
        "python3",
        "scripts/stage5f_e_reproducibility.py",
    ],
    "r3-snapshot": ["bash", "scripts/stage5f_r3_snapshot_gate.sh"],
    "inherited-b1": ["bash", "scripts/stage5f_inherited_b1_snapshot_gate.sh"],
    "inherited-b3f": [
        "bash",
        "scripts/stage5f_b3f_snapshot_provenance_gate.sh",
    ],
    "inherited-b3f-ui": ["bash", "scripts/stage5f_b3f_snapshot_ui_gate.sh"],
    "stage5c-freeze": ["python3", "scripts/stage5c_api_freeze_check.py"],
    "stage5d-freeze": ["python3", "scripts/stage5d_additive_freeze_check.py"],
    "stage5d-negative": [
        "python3",
        "scripts/stage5d_additive_freeze_negative_harness.py",
    ],
    "forbidden-no-rg": ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"],
    "redis-smoke": ["bash", "scripts/stage5f_e_redis_regression_gate.sh"],
    "functional": ["bash", "scripts/stage5f_functional_development_gate.sh"],
}

CLOSED_SURFACES = {
    "redis_command_consumption": False,
    "finam_transport": False,
    "http_post_delete": False,
    "dispatch": False,
    "broker_execution": False,
    "runtime_live": False,
    "real_orders": False,
    "ack_order_trade_position_timer_restart_feedback": False,
    "protective_order_lifecycle": False,
    "stage5g_authorized": False,
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
    return hashlib.sha1(f"{kind} {len(raw)}\0".encode() + raw).hexdigest()


def parse_utc(value: object, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be a UTC timestamp")
    try:
        return datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        fail(f"{label} is invalid: {exc}")


def validate_member_name(name: str, *, generated: bool = False) -> None:
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
    if any(part in {".git", "target", "tmp", "__pycache__", "__MACOSX"} for part in parts):
        fail(f"forbidden archive path: {name}")
    if "reports" in parts and name not in REPORTS:
        fail(f"unapproved reports member: {name}")
    if basename == ".env" or (
        basename.startswith(".env.") and basename != ".env.example"
    ):
        fail(f"secret-bearing env member is forbidden: {name}")
    if basename == ".DS_Store" or basename.endswith(".log") or ".local." in basename:
        fail(f"local artifact is forbidden: {name}")
    if generated and name.startswith("reports/") and name not in REPORTS:
        fail(f"unapproved generated report: {name}")


def tree_oid(entries: list[dict[str, str]], payloads: dict[str, bytes]) -> str:
    root: dict[str, Any] = {}
    for entry in entries:
        cursor = root
        parts = entry["path"].split("/")
        for part in parts[:-1]:
            cursor = cursor.setdefault(part, {})
            if not isinstance(cursor, dict):
                fail(f"source tree collision: {entry['path']}")
        if parts[-1] in cursor:
            fail(f"duplicate source path: {entry['path']}")
        cursor[parts[-1]] = (entry["git_mode"], payloads[entry["path"]])

    def encode(node: dict[str, Any]) -> str:
        records: list[tuple[bytes, bytes]] = []
        for name, value in node.items():
            if isinstance(value, dict):
                mode = "40000"
                oid = encode(value)
                key = name.encode() + b"/"
            else:
                mode, body = value
                oid = git_object_id("blob", body)
                key = name.encode() + b"\0"
            record = mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(oid)
            records.append((key, record))
        payload = b"".join(record for _, record in sorted(records))
        return git_object_id("tree", payload)

    return encode(root)


def pass_count(raw: bytes) -> int:
    return sum(line.startswith("PASS ") for line in raw.decode(errors="replace").splitlines())


def validate_archive(
    archive_path: str, allow_missing_final_safety: bool
) -> tuple[str, int]:
    try:
        with zipfile.ZipFile(archive_path) as archive:
            infos = archive.infolist()
            names = [info.filename for info in infos]
            if len(names) != len(set(names)):
                fail("archive contains duplicate members")
            for info in infos:
                validate_member_name(info.filename.rstrip("/"))
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

    for required in (
        COMMIT_MARKER,
        COMMIT_OBJECT,
        SOURCE_MANIFEST,
        EVIDENCE_MANIFEST,
        *REPORTS,
    ):
        if required not in files:
            fail(f"required member missing: {required}")
    safety_members = {SAFETY_RESULT, SAFETY_STDOUT, SAFETY_STDERR}
    if not allow_missing_final_safety and not safety_members.issubset(files):
        fail("final archive-safety evidence is incomplete")
    if allow_missing_final_safety and safety_members.intersection(files):
        fail("preseal archive must not contain partial safety evidence")

    marker_lines = files[COMMIT_MARKER].decode().splitlines()
    marker = dict(line.split("=", 1) for line in marker_lines if "=" in line)
    exact_keys(
        marker,
        {"archive_name", "source_commit", "source_ref", "parent_ref"},
        "commit marker",
    )
    source_ref = marker["source_ref"]
    if not HEX40.fullmatch(source_ref) or not HEX40.fullmatch(marker["parent_ref"]):
        fail("commit marker must use full SHA-1 ids")
    require(marker["source_commit"], source_ref[:7], "short/full commit binding")
    require(
        marker["archive_name"],
        PurePosixPath(archive_path).name,
        "archive-name binding",
    )

    source_manifest = decode_json(files[SOURCE_MANIFEST], SOURCE_MANIFEST)
    exact_keys(
        source_manifest,
        {
            "schema_version",
            "stage",
            "source_ref",
            "source_commit",
            "parent_ref",
            "head_tree",
            "members",
        },
        "source manifest",
    )
    require(source_manifest["schema_version"], 1, "source manifest schema")
    require(source_manifest["stage"], STAGE, "source manifest stage")
    require(source_manifest["source_ref"], source_ref, "source manifest ref")
    require(source_manifest["source_commit"], source_ref[:7], "source manifest short")
    require(source_manifest["parent_ref"], marker["parent_ref"], "source manifest parent")
    if not isinstance(source_manifest["head_tree"], str) or not HEX40.fullmatch(
        source_manifest["head_tree"]
    ):
        fail("source manifest head_tree must be SHA-1")
    entries = source_manifest["members"]
    if not isinstance(entries, list) or not entries:
        fail("source manifest must contain members")
    source_paths: list[str] = []
    source_payloads: dict[str, bytes] = {}
    for index, raw in enumerate(entries):
        entry = exact_keys(raw, {"git_mode", "path", "sha256"}, f"source member {index}")
        path = entry["path"]
        if not isinstance(path, str):
            fail(f"source member {index} path must be a string")
        validate_member_name(path)
        if path.startswith("reports/"):
            fail(f"reports must not be tracked source: {path}")
        if entry["git_mode"] not in {"100644", "100755"}:
            fail(f"unsupported source mode: {path}")
        if not isinstance(entry["sha256"], str) or not HEX64.fullmatch(entry["sha256"]):
            fail(f"invalid source member hash: {path}")
        if path not in files:
            fail(f"source member absent from archive: {path}")
        require(sha256(files[path]), entry["sha256"], f"source hash {path}")
        source_paths.append(path)
        source_payloads[path] = files[path]
    require(source_paths, sorted(source_paths), "source member order")
    if len(source_paths) != len(set(source_paths)):
        fail("source manifest contains duplicate paths")
    require(
        tree_oid(entries, source_payloads),
        source_manifest["head_tree"],
        "recomputed Git tree",
    )

    commit_raw = files[COMMIT_OBJECT]
    require(git_object_id("commit", commit_raw), source_ref, "commit object id")
    commit_lines = commit_raw.decode().splitlines()
    require(
        [line.split(" ", 1)[1] for line in commit_lines if line.startswith("tree ")],
        [source_manifest["head_tree"]],
        "commit/tree binding",
    )
    require(
        [line.split(" ", 1)[1] for line in commit_lines if line.startswith("parent ")],
        [marker["parent_ref"]],
        "commit/parent binding",
    )

    evidence = decode_json(files[EVIDENCE_MANIFEST], EVIDENCE_MANIFEST)
    exact_keys(
        evidence,
        {
            "schema_version",
            "stage",
            "status",
            "archive_name",
            "source_ref",
            "source_commit",
            "parent_ref",
            "head_tree",
            "accepted_stage5f_d",
            "source_tree_manifest_sha256",
            "commit_object_sha256",
            "final_scenario_inventory_sha256",
            "golden_results_sha256",
            "reports",
            "gates",
            "closed_surfaces",
            "toolchain",
        },
        "evidence manifest",
    )
    require(evidence["schema_version"], 1, "evidence schema")
    require(evidence["stage"], STAGE, "evidence stage")
    require(
        evidence["status"],
        "independent_review_required_before_stage5g",
        "evidence status",
    )
    require(evidence["archive_name"], marker["archive_name"], "evidence archive")
    require(evidence["source_ref"], source_ref, "evidence source ref")
    require(evidence["source_commit"], source_ref[:7], "evidence short ref")
    require(evidence["parent_ref"], marker["parent_ref"], "evidence parent")
    require(evidence["head_tree"], source_manifest["head_tree"], "evidence tree")
    require(evidence["accepted_stage5f_d"], ACCEPTED_D, "accepted Stage 5F-d")
    require(evidence["closed_surfaces"], CLOSED_SURFACES, "closed surfaces")
    require(
        evidence["source_tree_manifest_sha256"],
        sha256(files[SOURCE_MANIFEST]),
        "source manifest evidence hash",
    )
    require(
        evidence["commit_object_sha256"],
        sha256(commit_raw),
        "commit object evidence hash",
    )
    require(
        evidence["final_scenario_inventory_sha256"],
        sha256(files["docs/stage-5/stage5f-final-scenario-inventory.json"]),
        "final inventory evidence hash",
    )
    require(
        evidence["golden_results_sha256"],
        sha256(files["docs/stage-5/stage5f-d-golden-results.json"]),
        "golden evidence hash",
    )
    require(
        evidence["reports"],
        {member: sha256(files[member]) for member in REPORTS},
        "report hashes",
    )
    toolchain = exact_keys(evidence["toolchain"], {"rustc", "cargo", "python"}, "toolchain")
    if any(not isinstance(value, str) or not value.strip() for value in toolchain.values()):
        fail("toolchain evidence is incomplete")

    gates = evidence["gates"]
    if not isinstance(gates, list):
        fail("evidence gates must be an array")
    require([gate.get("label") for gate in gates], list(EXPECTED_COMMANDS), "gate order")
    generated = {
        COMMIT_MARKER,
        COMMIT_OBJECT,
        SOURCE_MANIFEST,
        EVIDENCE_MANIFEST,
        *REPORTS,
    }
    gate_results: dict[str, dict[str, Any]] = {}
    gate_outputs: dict[str, bytes] = {}
    for binding in gates:
        binding = exact_keys(
            binding,
            {"label", "result_member", "result_sha256"},
            "gate binding",
        )
        label = binding["label"]
        result_member = binding["result_member"]
        if result_member not in files:
            fail(f"gate result missing: {label}")
        require(sha256(files[result_member]), binding["result_sha256"], f"gate result hash {label}")
        result = decode_json(files[result_member], result_member)
        exact_keys(
            result,
            {
                "schema_version",
                "stage",
                "label",
                "command",
                "source_ref",
                "started_at_utc",
                "finished_at_utc",
                "exit_code",
                "stdout_member",
                "stdout_sha256",
                "stderr_member",
                "stderr_sha256",
            },
            f"gate result {label}",
        )
        require(result["schema_version"], 1, f"{label} schema")
        require(result["stage"], STAGE, f"{label} stage")
        require(result["label"], label, f"{label} label")
        require(result["command"], EXPECTED_COMMANDS[label], f"{label} command")
        require(result["source_ref"], source_ref, f"{label} source ref")
        require(result["exit_code"], 0, f"{label} exit")
        if parse_utc(result["finished_at_utc"], f"{label} finish") < parse_utc(
            result["started_at_utc"], f"{label} start"
        ):
            fail(f"{label} timestamps are reversed")
        for stream in ("stdout", "stderr"):
            member = result[f"{stream}_member"]
            if member not in files:
                fail(f"{label} {stream} member missing")
            require(sha256(files[member]), result[f"{stream}_sha256"], f"{label} {stream} hash")
            generated.add(member)
        generated.add(result_member)
        gate_results[label] = result
        gate_outputs[label] = files[result["stdout_member"]]

    expected_counts = {
        "aggregate-negative": 15,
        "matrix-negative": 40,
        "inherited-b3f": 580,
        "inherited-b3f-ui": 8,
        "stage5d-negative": 303,
        "forbidden-no-rg": 87,
    }
    for label, expected in expected_counts.items():
        require(pass_count(gate_outputs[label]), expected, f"{label} PASS count")
    markers = {
        "aggregate-checker": "stage5f-e-aggregate-acceptance-check: ok rows=34 groups=16 frozen=true",
        "aggregate-negative": "stage5f-e-aggregate-negative-harness: ok cases=15",
        "matrix-checker": "stage5f-d-atomic-matrix-check: ok rows=34 groups=16 golden=true",
        "matrix-negative": "stage5f-d-atomic-matrix-negative-harness: ok cases=40",
        "matrix-reproducibility": "stage5f-e-reproducibility: ok runs=3",
        "inherited-b1": "stage5f-inherited-b1-snapshot-gate: ok",
        "inherited-b3f": "stage5f-b3f-snapshot-provenance-gate: ok",
        "inherited-b3f-ui": "stage5f-b3f-snapshot-ui-gate: ok",
        "stage5d-negative": "stage5d-negative-harness: ok",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok source_ref=86b43c448fb65a3c54b6118d04d3f40e08e74ad7 rg_absent=true cases=87",
        "redis-smoke": "stage5f-e-redis-regression-gate: ok isolated=true",
        "functional": "stage5f-functional-development-gate: ok",
    }
    for label, marker_text in markers.items():
        if marker_text not in gate_outputs[label].decode(errors="replace"):
            fail(f"gate success marker missing: {label}")

    acceptance = decode_json(
        files["reports/stage5f/stage5f-acceptance-result.json"],
        "acceptance result",
    )
    exact_keys(
        acceptance,
        {
            "schema_version",
            "stage",
            "status",
            "source_ref",
            "source_commit",
            "generated_at_utc",
            "accepted_stage5f_d",
            "matrix",
            "final_scenario_inventory_sha256",
            "golden_results_sha256",
            "all_required_gates_passed",
            "gate_count",
            "gates",
            "closed_surfaces",
            "independent_final_review_required",
        },
        "acceptance result",
    )
    require(acceptance["schema_version"], 1, "acceptance schema")
    require(acceptance["stage"], STAGE, "acceptance stage")
    require(acceptance["status"], "aggregate_review_candidate", "acceptance status")
    require(acceptance["source_ref"], source_ref, "acceptance source")
    require(acceptance["source_commit"], source_ref[:7], "acceptance short source")
    parse_utc(acceptance["generated_at_utc"], "acceptance generated")
    require(acceptance["accepted_stage5f_d"], ACCEPTED_D, "acceptance predecessor")
    require(acceptance["all_required_gates_passed"], True, "acceptance gates")
    require(acceptance["gate_count"], len(EXPECTED_COMMANDS), "acceptance gate count")
    require(acceptance["gates"], gates, "acceptance/evidence gates")
    require(acceptance["closed_surfaces"], CLOSED_SURFACES, "acceptance closed surfaces")
    require(acceptance["independent_final_review_required"], True, "final review requirement")

    reproducibility = decode_json(
        files["reports/stage5f/stage5f-fingerprint-reproducibility.json"],
        "reproducibility report",
    )
    require(reproducibility.get("schema_version"), 1, "reproducibility schema")
    require(reproducibility.get("stage"), STAGE, "reproducibility stage")
    require(reproducibility.get("source_ref"), source_ref, "reproducibility source")
    require(reproducibility.get("run_count"), 3, "reproducibility run count")
    for flag in (
        "all_runs_passed",
        "all_fingerprints_identical",
        "all_semantic_evidence_identical",
    ):
        require(reproducibility.get(flag), True, f"reproducibility {flag}")
    runs = reproducibility.get("runs")
    if not isinstance(runs, list) or len(runs) != 3:
        fail("reproducibility runs must contain exactly three records")
    fingerprints = {run.get("fingerprint_vector_sha256") for run in runs}
    semantic = {run.get("semantic_evidence_sha256") for run in runs}
    require(fingerprints, {reproducibility.get("fingerprint_vector_sha256")}, "fingerprint equality")
    require(semantic, {reproducibility.get("semantic_evidence_sha256")}, "semantic equality")
    if any(not isinstance(value, str) or not HEX64.fullmatch(value) for value in fingerprints | semantic):
        fail("reproducibility hashes must be SHA-256")
    for index, run in enumerate(runs, start=1):
        require(run.get("run_index"), index, f"reproducibility run {index} index")
        require(
            run.get("command"),
            [
                "cargo",
                "test",
                "-q",
                "-p",
                "strategy-runtime-core",
                "stage5f_d_full_matrix_matches_frozen_golden",
                "--",
                "--test-threads=1",
            ],
            f"reproducibility run {index} command",
        )
        require(run.get("exit_code"), 0, f"reproducibility run {index} exit")
        require(run.get("row_count"), 34, f"reproducibility run {index} rows")

    negative = decode_json(
        files["reports/stage5f/stage5f-negative-result.json"],
        "negative result",
    )
    require(negative.get("schema_version"), 1, "negative schema")
    require(negative.get("stage"), STAGE, "negative stage")
    require(negative.get("source_ref"), source_ref, "negative source")
    require(negative.get("all_negative_matrices_passed"), True, "negative status")
    require(negative.get("rg_absent_for_forbidden_surface_matrix"), True, "no-rg evidence")
    require(
        negative.get("matrices"),
        [
            {"label": "stage5f_e_aggregate", "passed": 15, "required": 15},
            {"label": "stage5f_d_atomic", "passed": 40, "required": 40},
            {"label": "stage5d_additive_freeze", "passed": 303, "required": 303},
            {"label": "forbidden_surface_no_rg", "passed": 87, "required": 87},
            {"label": "b3f_detached_provenance", "passed": 580, "required": 580},
            {"label": "b3f_production_ui", "passed": 8, "required": 8},
        ],
        "negative matrix summary",
    )

    if SAFETY_RESULT in files:
        safety = decode_json(files[SAFETY_RESULT], SAFETY_RESULT)
        exact_keys(
            safety,
            {
                "schema_version",
                "stage",
                "source_ref",
                "started_at_utc",
                "finished_at_utc",
                "preseal_exit_code",
                "command",
                "stdout_member",
                "stdout_sha256",
                "stderr_member",
                "stderr_sha256",
                "checked_source_tree_manifest_sha256",
                "checked_evidence_manifest_sha256",
            },
            "archive safety result",
        )
        require(safety["schema_version"], 1, "safety schema")
        require(safety["stage"], STAGE, "safety stage")
        require(safety["source_ref"], source_ref, "safety source")
        require(safety["preseal_exit_code"], 0, "preseal exit")
        require(
            safety["command"],
            [
                "python3",
                "scripts/stage5f_e_handoff_safety_check.py",
                "--archive",
                marker["archive_name"],
                "--allow-missing-final-safety",
            ],
            "safety command",
        )
        for stream in ("stdout", "stderr"):
            member = safety[f"{stream}_member"]
            if member not in files:
                fail(f"safety {stream} member missing")
            require(sha256(files[member]), safety[f"{stream}_sha256"], f"safety {stream} hash")
            generated.add(member)
        if (
            f"stage5f-e-handoff-safety: ok source_ref={source_ref} gates={len(EXPECTED_COMMANDS)}"
            not in files[safety["stdout_member"]].decode(errors="replace")
        ):
            fail("preseal archive safety marker missing")
        require(
            safety["checked_source_tree_manifest_sha256"],
            sha256(files[SOURCE_MANIFEST]),
            "safety source manifest binding",
        )
        require(
            safety["checked_evidence_manifest_sha256"],
            sha256(files[EVIDENCE_MANIFEST]),
            "safety evidence manifest binding",
        )
        generated.add(SAFETY_RESULT)

    expected_files = set(source_paths).union(generated)
    require(set(files), expected_files, "archive member inventory")
    return source_ref, len(gates)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", required=True)
    parser.add_argument("--allow-missing-final-safety", action="store_true")
    args = parser.parse_args()
    try:
        source_ref, gate_count = validate_archive(
            args.archive, args.allow_missing_final_safety
        )
    except (SafetyFailure, OSError, UnicodeDecodeError, ValueError) as exc:
        print(f"stage5f-e-handoff-safety: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        f"stage5f-e-handoff-safety: ok source_ref={source_ref} gates={gate_count}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
