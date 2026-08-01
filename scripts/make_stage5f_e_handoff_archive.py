#!/usr/bin/env python3
"""Build the complete commit-bound Stage 5F-e aggregate review package."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
STAGE = "5F-e-aggregate-acceptance"
ACCEPTED_D = "1a41b530419d39ddc84fff81a9dfdde6ede878ce"
FINAL_INVENTORY = ROOT / "docs/stage-5/stage5f-final-scenario-inventory.json"
GOLDEN = ROOT / "docs/stage-5/stage5f-d-golden-results.json"
REPORT_DIR = ROOT / "reports/stage5f"
HANDOFF_DIR = ROOT / "reports/handoff"
ACCEPTANCE_REPORT = REPORT_DIR / "stage5f-acceptance-result.json"
REPRO_REPORT = REPORT_DIR / "stage5f-fingerprint-reproducibility.json"
NEGATIVE_REPORT = REPORT_DIR / "stage5f-negative-result.json"
SOURCE_MANIFEST_MEMBER = "stage5f-e-source-tree-manifest.json"
EVIDENCE_MANIFEST_MEMBER = "stage5f-e-evidence-manifest.json"
COMMIT_OBJECT_MEMBER = "stage5f-e-commit-object.txt"
COMMIT_MARKER_MEMBER = "handoff-commit.txt"
SAFETY_RESULT_MEMBER = "stage5f-e-archive-safety-result.json"

GATES: list[tuple[str, list[str]]] = [
    ("aggregate-checker", ["python3", "scripts/stage5f_e_aggregate_acceptance_check.py"]),
    (
        "aggregate-negative",
        ["python3", "scripts/stage5f_e_aggregate_acceptance_negative_harness.py"],
    ),
    ("fmt", ["cargo", "fmt", "--all", "--", "--check"]),
    ("workspace-tests", ["cargo", "test", "--workspace", "--all-targets"]),
    ("doctests", ["cargo", "test", "--workspace", "--doc"]),
    (
        "clippy",
        [
            "cargo",
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    ),
    ("matrix-checker", ["python3", "scripts/stage5f_d_atomic_matrix_check.py"]),
    (
        "matrix-negative",
        ["python3", "scripts/stage5f_d_atomic_matrix_negative_harness.py"],
    ),
    (
        "matrix-debug",
        [
            "cargo",
            "test",
            "-p",
            "strategy-runtime-core",
            "stage5f_",
            "--",
            "--test-threads=1",
        ],
    ),
    (
        "matrix-release",
        [
            "cargo",
            "test",
            "--release",
            "-p",
            "strategy-runtime-core",
            "stage5f_",
            "--",
            "--test-threads=1",
        ],
    ),
    (
        "matrix-default-parallel",
        ["cargo", "test", "-p", "strategy-runtime-core", "stage5f_"],
    ),
    ("matrix-reproducibility", ["python3", "scripts/stage5f_e_reproducibility.py"]),
    ("r3-snapshot", ["bash", "scripts/stage5f_r3_snapshot_gate.sh"]),
    ("inherited-b1", ["bash", "scripts/stage5f_inherited_b1_snapshot_gate.sh"]),
    (
        "inherited-b3f",
        ["bash", "scripts/stage5f_b3f_snapshot_provenance_gate.sh"],
    ),
    ("inherited-b3f-ui", ["bash", "scripts/stage5f_b3f_snapshot_ui_gate.sh"]),
    ("stage5c-freeze", ["python3", "scripts/stage5c_api_freeze_check.py"]),
    ("stage5d-freeze", ["python3", "scripts/stage5d_additive_freeze_check.py"]),
    (
        "stage5d-negative",
        ["python3", "scripts/stage5d_additive_freeze_negative_harness.py"],
    ),
    ("forbidden-no-rg", ["bash", "scripts/stage5f_forbidden_no_rg_gate.sh"]),
    ("redis-smoke", ["bash", "scripts/stage5f_e_redis_regression_gate.sh"]),
    ("functional", ["bash", "scripts/stage5f_functional_development_gate.sh"]),
]

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


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n"
    )


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def run_gate(
    temp: Path,
    label: str,
    command: list[str],
    source_ref: str,
) -> dict[str, Any]:
    evidence_dir = temp / "stage5f-e-evidence/gates"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    stdout_path = evidence_dir / f"{label}.stdout.txt"
    stderr_path = evidence_dir / f"{label}.stderr.txt"
    result_path = evidence_dir / f"{label}.result.json"
    started_at = utc_now()
    completed = subprocess.run(
        command,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    finished_at = utc_now()
    stdout_path.write_bytes(completed.stdout)
    stderr_path.write_bytes(completed.stderr)
    result = {
        "schema_version": 1,
        "stage": STAGE,
        "label": label,
        "command": command,
        "source_ref": source_ref,
        "started_at_utc": started_at,
        "finished_at_utc": finished_at,
        "exit_code": completed.returncode,
        "stdout_member": str(stdout_path.relative_to(temp)),
        "stdout_sha256": sha256_file(stdout_path),
        "stderr_member": str(stderr_path.relative_to(temp)),
        "stderr_sha256": sha256_file(stderr_path),
    }
    write_json(result_path, result)
    if completed.returncode != 0:
        sys.stdout.buffer.write(completed.stdout)
        sys.stderr.buffer.write(completed.stderr)
        raise SystemExit(f"Stage 5F-e gate failed: {label}")
    print(f"GATE_OK {label} stdout_sha256={result['stdout_sha256']}")
    return result


def count_pass_lines(result: dict[str, Any], temp: Path) -> int:
    text = (temp / result["stdout_member"]).read_text(errors="replace")
    return sum(line.startswith("PASS ") for line in text.splitlines())


def require_marker(result: dict[str, Any], temp: Path, marker: str) -> None:
    text = (temp / result["stdout_member"]).read_text(errors="replace")
    if marker not in text:
        raise SystemExit(f"gate marker missing for {result['label']}: {marker}")


def build_source_manifest(
    path: Path,
    source_ref: str,
    parent_ref: str,
    source_commit: str,
    head_tree: str,
) -> None:
    raw = subprocess.check_output(
        ["git", "ls-tree", "-r", "-z", source_ref], cwd=ROOT
    )
    members = []
    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, path_raw = record.split(b"\t", 1)
        mode, kind, object_id = metadata.decode("ascii").split()
        relative = path_raw.decode("utf-8")
        if kind != "blob" or mode not in {"100644", "100755"}:
            raise SystemExit(
                f"unsupported tracked entry: {mode} {kind} {relative}"
            )
        body = subprocess.check_output(
            ["git", "cat-file", "blob", object_id], cwd=ROOT
        )
        members.append(
            {
                "git_mode": mode,
                "path": relative,
                "sha256": sha256_bytes(body),
            }
        )
    members.sort(key=lambda item: item["path"])
    write_json(
        path,
        {
            "schema_version": 1,
            "stage": STAGE,
            "source_ref": source_ref,
            "source_commit": source_commit,
            "parent_ref": parent_ref,
            "head_tree": head_tree,
            "members": members,
        },
    )


def append_members(archive: Path, temp: Path, members: list[str]) -> None:
    with zipfile.ZipFile(
        archive, "a", compression=zipfile.ZIP_DEFLATED
    ) as handle:
        for member in members:
            handle.write(temp / member, member)


def validate_gate_evidence(
    results: dict[str, dict[str, Any]], temp: Path
) -> None:
    expected_counts = {
        "aggregate-negative": 15,
        "matrix-negative": 40,
        "inherited-b3f": 580,
        "inherited-b3f-ui": 8,
        "stage5d-negative": 303,
        "forbidden-no-rg": 87,
    }
    for label, count in expected_counts.items():
        actual = count_pass_lines(results[label], temp)
        if actual != count:
            raise SystemExit(
                f"{label} PASS count mismatch: expected {count}, got {actual}"
            )
    markers = {
        "aggregate-checker": "stage5f-e-aggregate-acceptance-check: ok rows=34 groups=16 frozen=true",
        "aggregate-negative": "stage5f-e-aggregate-negative-harness: ok cases=15",
        "matrix-checker": "stage5f-d-atomic-matrix-check: ok rows=34 groups=16 golden=true",
        "matrix-negative": "stage5f-d-atomic-matrix-negative-harness: ok cases=40",
        "matrix-reproducibility": "stage5f-e-reproducibility: ok runs=3",
        "r3-snapshot": "stage5f-r3-snapshot-gate: ok",
        "inherited-b1": "stage5f-inherited-b1-snapshot-gate: ok",
        "inherited-b3f": "stage5f-b3f-snapshot-provenance-gate: ok",
        "inherited-b3f-ui": "stage5f-b3f-snapshot-ui-gate: ok",
        "stage5d-negative": "stage5d-negative-harness: ok",
        "forbidden-no-rg": "stage5f-forbidden-no-rg-gate: ok source_ref=86b43c448fb65a3c54b6118d04d3f40e08e74ad7 rg_absent=true cases=87",
        "redis-smoke": "stage5f-e-redis-regression-gate: ok isolated=true",
        "functional": "stage5f-functional-development-gate: ok",
    }
    for label, marker in markers.items():
        require_marker(results[label], temp, marker)
    for label in ("matrix-debug", "matrix-release", "matrix-default-parallel"):
        require_marker(
            results[label],
            temp,
            "stage5f_d_full_matrix_matches_frozen_golden ... ok",
        )
        require_marker(
            results[label],
            temp,
            "stage5f_f34_stage5c_pending_request_mismatch_terminal ... ok",
        )


def main() -> int:
    if run_text(["git", "status", "--porcelain", "--untracked-files=all"]):
        raise SystemExit("refusing Stage 5F-e handoff: source tree is dirty")
    if run_text(["git", "rev-parse", "--show-object-format"]) != "sha1":
        raise SystemExit("Stage 5F-e handoff currently requires SHA-1 Git objects")
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    parent_ref = run_text(["git", "rev-parse", "HEAD^"])
    head_tree = run_text(["git", "rev-parse", "HEAD^{tree}"])
    subprocess.run(
        ["git", "merge-base", "--is-ancestor", ACCEPTED_D, source_ref],
        cwd=ROOT,
        check=True,
    )

    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    HANDOFF_DIR.mkdir(parents=True, exist_ok=True)
    for report in (ACCEPTANCE_REPORT, REPRO_REPORT, NEGATIVE_REPORT):
        report.unlink(missing_ok=True)
    archive_name = f"moex-trading-project-{source_commit}.zip"
    archive_path = HANDOFF_DIR / archive_name
    sha_path = Path(str(archive_path) + ".sha256")

    with tempfile.TemporaryDirectory(prefix="stage5f-e-handoff-") as raw_temp:
        temp = Path(raw_temp)
        results: dict[str, dict[str, Any]] = {}
        for label, command in GATES:
            results[label] = run_gate(temp, label, command, source_ref)
        validate_gate_evidence(results, temp)

        reproducibility = json.loads(REPRO_REPORT.read_text())
        if (
            reproducibility.get("source_ref") != source_ref
            or reproducibility.get("run_count") != 3
            or reproducibility.get("all_runs_passed") is not True
            or reproducibility.get("all_fingerprints_identical") is not True
            or reproducibility.get("all_semantic_evidence_identical") is not True
        ):
            raise SystemExit("Stage 5F-e reproducibility report is incomplete")

        inventory = json.loads(FINAL_INVENTORY.read_text())
        gate_bindings = [
            {
                "label": label,
                "result_member": results[label]["stdout_member"].replace(
                    ".stdout.txt", ".result.json"
                ),
                "result_sha256": sha256_file(
                    temp
                    / results[label]["stdout_member"].replace(
                        ".stdout.txt", ".result.json"
                    )
                ),
            }
            for label, _ in GATES
        ]
        generated_at = utc_now()
        acceptance = {
            "schema_version": 1,
            "stage": STAGE,
            "status": "aggregate_review_candidate",
            "source_ref": source_ref,
            "source_commit": source_commit,
            "generated_at_utc": generated_at,
            "accepted_stage5f_d": inventory["accepted_stage5f_d"],
            "matrix": inventory["matrix"],
            "final_scenario_inventory_sha256": sha256_file(FINAL_INVENTORY),
            "golden_results_sha256": sha256_file(GOLDEN),
            "all_required_gates_passed": True,
            "gate_count": len(GATES),
            "gates": gate_bindings,
            "closed_surfaces": CLOSED_SURFACES,
            "independent_final_review_required": True,
        }
        write_json(ACCEPTANCE_REPORT, acceptance)

        negative = {
            "schema_version": 1,
            "stage": STAGE,
            "source_ref": source_ref,
            "generated_at_utc": generated_at,
            "all_negative_matrices_passed": True,
            "matrices": [
                {"label": "stage5f_e_aggregate", "passed": 15, "required": 15},
                {"label": "stage5f_d_atomic", "passed": 40, "required": 40},
                {"label": "stage5d_additive_freeze", "passed": 303, "required": 303},
                {"label": "forbidden_surface_no_rg", "passed": 87, "required": 87},
                {"label": "b3f_detached_provenance", "passed": 580, "required": 580},
                {"label": "b3f_production_ui", "passed": 8, "required": 8},
            ],
            "rg_absent_for_forbidden_surface_matrix": True,
        }
        write_json(NEGATIVE_REPORT, negative)

        source_manifest = temp / SOURCE_MANIFEST_MEMBER
        build_source_manifest(
            source_manifest,
            source_ref,
            parent_ref,
            source_commit,
            head_tree,
        )
        commit_object = temp / COMMIT_OBJECT_MEMBER
        commit_object.write_bytes(
            subprocess.check_output(["git", "cat-file", "commit", source_ref], cwd=ROOT)
        )
        marker = temp / COMMIT_MARKER_MEMBER
        marker.write_text(
            f"archive_name={archive_name}\n"
            f"source_commit={source_commit}\n"
            f"source_ref={source_ref}\n"
            f"parent_ref={parent_ref}\n"
        )

        report_members = {
            "reports/stage5f/stage5f-acceptance-result.json": ACCEPTANCE_REPORT,
            "reports/stage5f/stage5f-fingerprint-reproducibility.json": REPRO_REPORT,
            "reports/stage5f/stage5f-negative-result.json": NEGATIVE_REPORT,
        }
        for member, source in report_members.items():
            destination = temp / member
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)

        evidence_manifest = temp / EVIDENCE_MANIFEST_MEMBER
        write_json(
            evidence_manifest,
            {
                "schema_version": 1,
                "stage": STAGE,
                "status": "independent_review_required_before_stage5g",
                "archive_name": archive_name,
                "source_ref": source_ref,
                "source_commit": source_commit,
                "parent_ref": parent_ref,
                "head_tree": head_tree,
                "accepted_stage5f_d": inventory["accepted_stage5f_d"],
                "source_tree_manifest_sha256": sha256_file(source_manifest),
                "commit_object_sha256": sha256_file(commit_object),
                "final_scenario_inventory_sha256": sha256_file(FINAL_INVENTORY),
                "golden_results_sha256": sha256_file(GOLDEN),
                "reports": {
                    member: sha256_file(path) for member, path in report_members.items()
                },
                "gates": gate_bindings,
                "closed_surfaces": CLOSED_SURFACES,
                "toolchain": {
                    "rustc": run_text(["rustc", "--version"]),
                    "cargo": run_text(["cargo", "--version"]),
                    "python": sys.version.splitlines()[0],
                },
            },
        )

        temp_archive = temp / archive_name
        subprocess.run(
            [
                "git",
                "archive",
                "--format=zip",
                f"--output={temp_archive}",
                source_ref,
            ],
            cwd=ROOT,
            check=True,
        )
        generated_members = [
            COMMIT_MARKER_MEMBER,
            COMMIT_OBJECT_MEMBER,
            SOURCE_MANIFEST_MEMBER,
            EVIDENCE_MANIFEST_MEMBER,
            *report_members,
        ]
        for label, _ in GATES:
            generated_members.extend(
                [
                    f"stage5f-e-evidence/gates/{label}.result.json",
                    f"stage5f-e-evidence/gates/{label}.stdout.txt",
                    f"stage5f-e-evidence/gates/{label}.stderr.txt",
                ]
            )
        append_members(temp_archive, temp, generated_members)

        safety_stdout = temp / "stage5f-e-archive-safety.stdout.txt"
        safety_stderr = temp / "stage5f-e-archive-safety.stderr.txt"
        safety_started = utc_now()
        preseal = subprocess.run(
            [
                "python3",
                "scripts/stage5f_e_handoff_safety_check.py",
                "--archive",
                str(temp_archive),
                "--allow-missing-final-safety",
            ],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        safety_finished = utc_now()
        safety_stdout.write_bytes(preseal.stdout)
        safety_stderr.write_bytes(preseal.stderr)
        if preseal.returncode != 0:
            sys.stdout.buffer.write(preseal.stdout)
            sys.stderr.buffer.write(preseal.stderr)
            raise SystemExit("Stage 5F-e preseal archive safety failed")
        safety_result = temp / SAFETY_RESULT_MEMBER
        write_json(
            safety_result,
            {
                "schema_version": 1,
                "stage": STAGE,
                "source_ref": source_ref,
                "started_at_utc": safety_started,
                "finished_at_utc": safety_finished,
                "preseal_exit_code": preseal.returncode,
                "command": [
                    "python3",
                    "scripts/stage5f_e_handoff_safety_check.py",
                    "--archive",
                    archive_name,
                    "--allow-missing-final-safety",
                ],
                "stdout_member": "stage5f-e-archive-safety.stdout.txt",
                "stdout_sha256": sha256_file(safety_stdout),
                "stderr_member": "stage5f-e-archive-safety.stderr.txt",
                "stderr_sha256": sha256_file(safety_stderr),
                "checked_source_tree_manifest_sha256": sha256_file(source_manifest),
                "checked_evidence_manifest_sha256": sha256_file(evidence_manifest),
            },
        )
        append_members(
            temp_archive,
            temp,
            [
                SAFETY_RESULT_MEMBER,
                "stage5f-e-archive-safety.stdout.txt",
                "stage5f-e-archive-safety.stderr.txt",
            ],
        )
        subprocess.run(
            [
                "python3",
                "scripts/stage5f_e_handoff_safety_check.py",
                "--archive",
                str(temp_archive),
            ],
            cwd=ROOT,
            check=True,
        )
        shutil.move(str(temp_archive), archive_path)

    sha_path.write_text(f"{sha256_file(archive_path)}  {archive_name}\n")
    print(f"Stage 5F-e handoff archive: {archive_path}")
    print(f"Stage 5F-e handoff SHA-256: {sha_path}")
    print(sha_path.read_text(), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
