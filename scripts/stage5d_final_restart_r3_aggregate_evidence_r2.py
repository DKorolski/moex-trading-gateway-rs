#!/usr/bin/env python3
"""Fail-closed evidence builder for Stage 5D aggregate closure r2.

The builder consumes only fresh machine-readable command result records from a
single run directory. It refuses missing records, non-zero exits, source-ref
drift, stale log hashes, missing per-case results, and opened execution
surfaces.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
MANIFEST = ROOT / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
REVIEW_STAGE = "5D-final-restart-r3-aggregate-closure-r2"

REQUIRED_GATE_IDS = [
    "aggregate_checker_self_test",
    "positive_r3a_pending_entry",
    "positive_core",
    "positive_current_shadow",
    "positive_operational_state",
    "positive_recovery_index",
    "positive_riskgate_recovery",
    "package_negative_matrix",
    "package_negative_riskgate_forged_receipts",
    "stage5c_api_freeze",
    "stage5d_additive_freeze",
    "forbidden_surface",
    "no_redis_smoke",
    "golden_fixture_drift",
    "stage5d_negative_harness",
    "cargo_fmt",
    "cargo_test_all_targets",
    "cargo_test_doc",
    "cargo_clippy",
    "handoff_source_archive_safety",
]


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def read_result(run_dir: Path, gate_id: str, source_ref: str) -> dict[str, Any]:
    path = run_dir / "command-results" / f"{gate_id}.result.json"
    if not path.is_file():
        raise SystemExit(f"missing command result: {path}")
    result = json.loads(path.read_text())
    if result.get("gate_id") != gate_id:
        raise SystemExit(f"gate_id mismatch in {path}")
    if result.get("source_ref") != source_ref:
        raise SystemExit(f"source_ref mismatch in {path}")
    if result.get("exit_code") != 0:
        raise SystemExit(f"gate {gate_id} did not pass: exit_code={result.get('exit_code')}")
    for stream_name in ("stdout", "stderr"):
        stream = result.get(stream_name, {})
        stream_path = ROOT / stream.get("path", "")
        if not stream_path.is_file():
            raise SystemExit(f"{gate_id} {stream_name} log missing")
        if sha256_file(stream_path) != stream.get("sha256"):
            raise SystemExit(f"{gate_id} {stream_name} log hash mismatch")
    return result


def validate_scenario_results(run_dir: Path, source_ref: str) -> list[dict[str, Any]]:
    path = run_dir / "scenario-results.json"
    if not path.is_file():
        raise SystemExit("scenario-results.json missing")
    scenario_results = json.loads(path.read_text())
    inventory = json.loads(INVENTORY.read_text())
    rows = inventory.get("scenario_rows", [])
    if len(rows) != 21 or len(scenario_results) != 21:
        raise SystemExit("Stage 5D aggregate scenario count mismatch")
    rows_by_id = {row["case_id"]: row for row in rows}
    if set(rows_by_id) != {result.get("case_id") for result in scenario_results}:
        raise SystemExit("Stage 5D aggregate scenario result id mismatch")
    for result in scenario_results:
        case_id = result["case_id"]
        row = rows_by_id[case_id]
        if result.get("source_ref") != source_ref:
            raise SystemExit(f"{case_id}: source_ref mismatch")
        if result.get("result") != "passed":
            raise SystemExit(f"{case_id}: result is not passed")
        if result.get("execution_status") != row.get("execution_status"):
            raise SystemExit(f"{case_id}: execution status drift")
        if result.get("owning_test") != row.get("owning_test"):
            raise SystemExit(f"{case_id}: owning test drift")
        if not str(row.get("execution_status", "")).startswith("accepted_"):
            raise SystemExit(f"{case_id}: non-accepted row cannot be in aggregate closure")
        if not row.get("owning_test"):
            raise SystemExit(f"{case_id}: owning test missing")
        if not result.get("proof_basis"):
            raise SystemExit(f"{case_id}: proof basis missing")
        if result.get("owning_gate_id") not in REQUIRED_GATE_IDS:
            raise SystemExit(f"{case_id}: owning gate id is not mandatory")
        if result.get("inventory_row_sha256") != sha256_bytes(canonical_json(row)):
            raise SystemExit(f"{case_id}: inventory row hash mismatch")
    return scenario_results


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", required=True)
    parser.add_argument("--source-archive", required=True)
    parser.add_argument("--source-archive-sha256", required=True)
    parser.add_argument("--source-handoff-manifest-sha256", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    run_dir = Path(args.run_dir).resolve()
    output = Path(args.output).resolve()
    source_archive = Path(args.source_archive)
    if not source_archive.is_absolute():
        source_archive = ROOT / source_archive
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    if run_text(["git", "status", "--short"]) != "":
        raise SystemExit("dirty source tree before aggregate evidence generation")

    manifest = json.loads(MANIFEST.read_text())
    inventory = json.loads(INVENTORY.read_text())
    if manifest.get("stage") != REVIEW_STAGE:
        raise SystemExit("manifest stage mismatch for aggregate closure r2")
    if inventory.get("status") != "aggregate_closure_r2_candidate":
        raise SystemExit("inventory status mismatch for aggregate closure r2")
    closed = inventory.get("closed_surfaces", {})
    for surface in ("stage5e", "redis", "finam", "transport", "dispatch", "runtime_live", "broker_execution"):
        if closed.get(surface) is not False:
            raise SystemExit(f"closed surface opened: {surface}")

    command_results = {
        gate_id: read_result(run_dir, gate_id, source_ref) for gate_id in REQUIRED_GATE_IDS
    }
    scenario_results = validate_scenario_results(run_dir, source_ref)
    negative_harness = command_results["stage5d_negative_harness"]
    negative_stdout = (ROOT / negative_harness["stdout"]["path"]).read_text(errors="replace")
    if "cases_declared=303" not in negative_stdout or "stage5d-negative-harness: ok" not in negative_stdout:
        raise SystemExit("Stage 5D negative harness summary missing or incomplete")

    aggregate = {
        "schema_version": 2,
        "review_stage": REVIEW_STAGE,
        "source_commit": source_commit,
        "source_ref": source_ref,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_archive": {
            "path": str(source_archive.resolve().relative_to(ROOT)),
            "sha256": args.source_archive_sha256,
            "handoff_manifest_sha256": args.source_handoff_manifest_sha256,
        },
        "mandatory_positive_count": len(scenario_results),
        "accepted_executable_count": len(scenario_results),
        "todo_source_produced_count": 0,
        "negative_case_count": 303,
        "closed_surfaces": closed,
        "stage5e_closed": closed.get("stage5e") is False,
        "clean_worktree_before": True,
        "clean_worktree_after": run_text(["git", "status", "--short"]) == "",
        "toolchain": {
            "rustc": run_text(["rustc", "--version"]),
            "cargo": run_text(["cargo", "--version"]),
        },
        "required_gate_ids": REQUIRED_GATE_IDS,
        "command_results": command_results,
        "scenario_results": scenario_results,
        "aggregate_fingerprints": {
            "inventory_sha256": sha256_file(INVENTORY),
            "manifest_sha256": sha256_file(MANIFEST),
            "scenario_results_sha256": sha256_bytes(canonical_json(scenario_results)),
            "command_results_sha256": sha256_bytes(canonical_json(command_results)),
        },
        "freeze_oracle_sidecar": {
            "bundle_id": "imoexf_hybrid_mr_bo_handoff_2026_04",
            "classification": ["research_target", "ALOR_operational_oracle", "later_overlays"],
            "sidecar_only": True,
            "production_dependency": False,
            "stage5h_parity_claimed": False,
        },
    }
    if not aggregate["clean_worktree_after"]:
        raise SystemExit("dirty source tree after aggregate evidence generation")
    output.write_text(json.dumps(aggregate, indent=2, ensure_ascii=False) + "\n")
    print(f"aggregate_evidence_path={output.relative_to(ROOT)}")
    print(f"aggregate_evidence_sha256={sha256_file(output)}")
    print("mandatory_positive_count=21")
    print("accepted_executable_count=21")
    print("todo_source_produced_count=0")
    print("negative_case_count=303")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
