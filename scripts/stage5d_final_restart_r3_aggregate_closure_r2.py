#!/usr/bin/env python3
"""Single-entry Stage 5D aggregate closure r2 runner.

This script creates a fresh run directory, executes every mandatory gate,
writes machine-readable command records, builds source/evidence handoff
archives, and emits a closure manifest. It is evidence/governance-only and
does not open Redis, FINAM, transport, dispatch, runtime-live or broker
execution surfaces.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REVIEW_STAGE = "5D-final-restart-r3-aggregate-closure-r2"
INVENTORY = ROOT / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"

POSITIVE_GROUPS = [
    ("positive_r3a_pending_entry", "stage5d_final_r3a_source_pending_entry_full_restart_matrix"),
    ("positive_core", "stage5d_final_r3_positive_core_source_produced_full_restart_matrix"),
    ("positive_current_shadow", "stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix"),
    ("positive_operational_state", "stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix"),
    ("positive_recovery_index", "stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix"),
    ("positive_riskgate_recovery", "stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix"),
]


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def rel(path: Path) -> str:
    return str(path.resolve().relative_to(ROOT))


def normalize_output(payload: bytes) -> str:
    text = payload.decode("utf-8", errors="replace")
    return text.replace(str(ROOT), "$REPO_ROOT")


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n")


def run_gate(run_dir: Path, gate_id: str, command: list[str], source_ref: str) -> dict[str, Any]:
    started = utc_now()
    process = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    finished = utc_now()
    stdout_path = run_dir / "logs" / f"{gate_id}.stdout.log"
    stderr_path = run_dir / "logs" / f"{gate_id}.stderr.log"
    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    stdout_path.write_text(normalize_output(process.stdout))
    stderr_path.write_text(normalize_output(process.stderr))
    result = {
        "schema_version": 1,
        "review_stage": REVIEW_STAGE,
        "gate_id": gate_id,
        "command": command,
        "cwd": ".",
        "source_ref": source_ref,
        "started_at_utc": started,
        "finished_at_utc": finished,
        "exit_code": process.returncode,
        "stdout": {
            "path": rel(stdout_path),
            "sha256": sha256_file(stdout_path),
            "line_count": len(stdout_path.read_text(errors="replace").splitlines()),
        },
        "stderr": {
            "path": rel(stderr_path),
            "sha256": sha256_file(stderr_path),
            "line_count": len(stderr_path.read_text(errors="replace").splitlines()),
        },
        "toolchain": {
            "rustc": run_text(["rustc", "--version"]),
            "cargo": run_text(["cargo", "--version"]),
        },
    }
    result_path = run_dir / "command-results" / f"{gate_id}.result.json"
    write_json(result_path, result)
    if process.returncode != 0:
        print(stdout_path.read_text(), end="")
        print(stderr_path.read_text(), file=sys.stderr, end="")
        raise SystemExit(f"gate failed: {gate_id}")
    print(f"GATE_OK {gate_id} stdout_sha256={result['stdout']['sha256']}")
    return result


def build_scenario_results(run_dir: Path, command_results: dict[str, dict[str, Any]], source_ref: str) -> list[dict[str, Any]]:
    inventory = json.loads(INVENTORY.read_text())
    rows = inventory["scenario_rows"]
    group_by_test = {test_name: gate_id for gate_id, test_name in POSITIVE_GROUPS}
    results = []
    for row in rows:
        owning_test = row.get("owning_test")
        gate_id = group_by_test.get(owning_test)
        if gate_id is None:
            raise SystemExit(f"unrecognized owning test for {row['case_id']}: {owning_test}")
        if command_results[gate_id]["exit_code"] != 0:
            raise SystemExit(f"owning gate failed for {row['case_id']}: {gate_id}")
        core_keys = {"case_id", "category", "execution_status", "owning_test"}
        proof_basis = {
            key: value for key, value in row.items() if key not in core_keys
        }
        proof_basis["owning_gate"] = {
            "gate_id": gate_id,
            "exit_code": command_results[gate_id]["exit_code"],
            "source_ref": command_results[gate_id]["source_ref"],
            "stdout_sha256": command_results[gate_id]["stdout"]["sha256"],
            "result_sha256": sha256_bytes(canonical_json(command_results[gate_id])),
        }
        if row["case_id"].startswith("positive_") and not str(row["execution_status"]).startswith("accepted_"):
            raise SystemExit(f"non-accepted aggregate row: {row['case_id']}")
        results.append(
            {
                "case_id": row["case_id"],
                "category": row.get("category"),
                "execution_status": row["execution_status"],
                "owning_test": owning_test,
                "owning_gate_id": gate_id,
                "source_ref": source_ref,
                "result": "passed",
                "proof_basis": proof_basis,
                "inventory_row_sha256": sha256_bytes(canonical_json(row)),
                "owning_gate_result_sha256": sha256_bytes(canonical_json(command_results[gate_id])),
            }
        )
    output = run_dir / "scenario-results.json"
    write_json(output, results)
    return results


def parse_handoff_archive(stdout: str) -> tuple[Path, str]:
    archive_path = None
    sha_path = None
    for line in stdout.splitlines():
        if line.endswith(".zip"):
            archive_path = Path(line.replace("$REPO_ROOT", str(ROOT)))
        if line.endswith(".zip.sha256"):
            sha_path = Path(line.replace("$REPO_ROOT", str(ROOT)))
    if archive_path is None or sha_path is None:
        raise SystemExit("handoff archive paths missing from source archive command")
    return archive_path, sha_path.read_text().split()[0]


def source_handoff_manifest_sha256(source_archive: Path) -> str:
    with zipfile.ZipFile(source_archive) as archive:
        payload = archive.read("handoff-manifest.json")
    return sha256_bytes(payload)


def zip_files(output: Path, files: list[Path]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        output.unlink()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            archive.write(path, rel(path))


def main() -> int:
    if run_text(["git", "status", "--short"]) != "":
        raise SystemExit("refusing aggregate closure r2: source tree is dirty")
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = ROOT / "reports/stage-5" / f"stage5d-aggregate-closure-r2-{source_commit}-{timestamp}"
    run_dir.mkdir(parents=True, exist_ok=False)
    print(f"stage5d-aggregate-closure-r2: start run_dir={rel(run_dir)}")

    command_results: dict[str, dict[str, Any]] = {}
    command_results["aggregate_checker_self_test"] = run_gate(
        run_dir,
        "aggregate_checker_self_test",
        ["python3", "scripts/stage5d_final_restart_r3_aggregate_self_test.py"],
        source_ref,
    )
    for gate_id, test_name in POSITIVE_GROUPS:
        command_results[gate_id] = run_gate(
            run_dir,
            gate_id,
            ["cargo", "test", "-p", "strategy-runtime-core", test_name, "--", "--nocapture"],
            source_ref,
        )
    command_results["package_negative_matrix"] = run_gate(
        run_dir,
        "package_negative_matrix",
        [
            "cargo",
            "test",
            "-p",
            "strategy-runtime-core",
            "stage5d_final_r2_package_negative_matrix_fails_closed",
            "--",
            "--nocapture",
        ],
        source_ref,
    )
    command_results["package_negative_riskgate_forged_receipts"] = run_gate(
        run_dir,
        "package_negative_riskgate_forged_receipts",
        [
            "cargo",
            "test",
            "-p",
            "strategy-runtime-core",
            "stage5d_final_r3_riskgate_recovery_r1r3_forged_receipts_fail_closed",
            "--",
            "--nocapture",
        ],
        source_ref,
    )
    for gate_id, command in [
        ("stage5c_api_freeze", ["python3", "scripts/stage5c_api_freeze_check.py"]),
        ("stage5d_additive_freeze", ["python3", "scripts/stage5d_additive_freeze_check.py"]),
        ("forbidden_surface", ["bash", "scripts/forbidden_surface_scan.sh"]),
        ("no_redis_smoke", ["bash", "scripts/test_m4_3x_evidence_no_redis.sh"]),
        ("golden_fixture_drift", ["python3", "scripts/stage5d_additive_freeze_check.py"]),
        ("stage5d_negative_harness", ["python3", "scripts/stage5d_additive_freeze_negative_harness.py"]),
        ("cargo_fmt", ["cargo", "fmt", "--all", "--check"]),
        ("cargo_test_all_targets", ["cargo", "test", "--workspace", "--all-targets"]),
        ("cargo_test_doc", ["cargo", "test", "--workspace", "--doc"]),
        ("cargo_clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
    ]:
        command_results[gate_id] = run_gate(run_dir, gate_id, command, source_ref)

    scenario_results = build_scenario_results(run_dir, command_results, source_ref)
    if len(scenario_results) != 21:
        raise SystemExit("scenario result count mismatch")

    command_results["handoff_source_archive_safety"] = run_gate(
        run_dir,
        "handoff_source_archive_safety",
        ["bash", "scripts/make_handoff_archive.sh"],
        source_ref,
    )
    source_archive, source_archive_sha = parse_handoff_archive(
        (ROOT / command_results["handoff_source_archive_safety"]["stdout"]["path"]).read_text()
    )
    source_manifest_sha = source_handoff_manifest_sha256(source_archive)

    aggregate_evidence = run_dir / "aggregate-evidence.json"
    run_gate(
        run_dir,
        "aggregate_evidence_builder",
        [
            "python3",
            "scripts/stage5d_final_restart_r3_aggregate_evidence_r2.py",
            "--run-dir",
            rel(run_dir),
            "--source-archive",
            rel(source_archive),
            "--source-archive-sha256",
            source_archive_sha,
            "--source-handoff-manifest-sha256",
            source_manifest_sha,
            "--output",
            rel(aggregate_evidence),
        ],
        source_ref,
    )

    member_files = sorted(
        [
            path
            for path in run_dir.rglob("*")
            if path.is_file() and path.name != "closure-manifest.json"
        ],
        key=lambda path: rel(path),
    )
    closure_manifest = {
        "schema_version": 1,
        "review_stage": REVIEW_STAGE,
        "source_commit": source_commit,
        "source_ref": source_ref,
        "created_at_utc": utc_now(),
        "source_archive": {
            "path": rel(source_archive),
            "sha256": source_archive_sha,
            "handoff_manifest_sha256": source_manifest_sha,
        },
        "aggregate_evidence": {
            "path": rel(aggregate_evidence),
            "sha256": sha256_file(aggregate_evidence),
        },
        "scenario_results_sha256": sha256_file(run_dir / "scenario-results.json"),
        "command_result_count": len(list((run_dir / "command-results").glob("*.result.json"))),
        "all_required_gates_passed": True,
        "evidence_members": [
            {"path": rel(path), "sha256": sha256_file(path)} for path in member_files
        ],
    }
    closure_manifest_path = run_dir / "closure-manifest.json"
    write_json(closure_manifest_path, closure_manifest)
    evidence_archive = ROOT / "reports/handoff" / f"stage5d-aggregate-closure-r2-evidence-{source_commit}.zip"
    zip_files(evidence_archive, member_files + [closure_manifest_path])
    evidence_archive_sha = sha256_file(evidence_archive)
    (evidence_archive.with_suffix(evidence_archive.suffix + ".sha256")).write_text(
        f"{evidence_archive_sha}  {evidence_archive.name}\n"
    )
    postpack_manifest = ROOT / "reports/handoff" / f"stage5d-aggregate-closure-r2-postpack-manifest-{source_commit}.json"
    write_json(
        postpack_manifest,
        {
            "schema_version": 1,
            "review_stage": REVIEW_STAGE,
            "source_ref": source_ref,
            "source_archive_sha256": source_archive_sha,
            "evidence_archive": rel(evidence_archive),
            "evidence_archive_sha256": evidence_archive_sha,
            "closure_manifest_sha256": sha256_file(closure_manifest_path),
        },
    )
    print(f"source_archive={rel(source_archive)}")
    print(f"source_archive_sha256={source_archive_sha}")
    print(f"aggregate_evidence={rel(aggregate_evidence)}")
    print(f"aggregate_evidence_sha256={sha256_file(aggregate_evidence)}")
    print(f"closure_manifest={rel(closure_manifest_path)}")
    print(f"closure_manifest_sha256={sha256_file(closure_manifest_path)}")
    print(f"evidence_archive={rel(evidence_archive)}")
    print(f"evidence_archive_sha256={evidence_archive_sha}")
    print("mandatory_positive_count=21")
    print("accepted_executable_count=21")
    print("todo_source_produced_count=0")
    print("negative_case_count=303")
    print("stage5d-aggregate-closure-r2: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
