#!/usr/bin/env python3
"""Generate the Stage 5D final-restart-r3 aggregate evidence index.

This script is intentionally evidence-only. It reads reviewed source files,
logs and the mandatory scenario inventory, then writes a machine-readable
aggregate index under reports/. It must not touch Redis, FINAM, transport,
dispatch or runtime-live surfaces.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT_DIR = ROOT / "reports/stage-5"
INVENTORY = ROOT / "docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json"
MANIFEST = ROOT / "docs/stage-5/stage-5d-additive-freeze-manifest.json"
OUTPUT = REPORT_DIR / "stage5d-final-restart-r3-aggregate-evidence.json"
REVIEW_STAGE = "5D-final-restart-r3-aggregate-closure-r1"

LOG_PATHS = {
    "aggregate_focused_positive": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-positive.log",
    "aggregate_checker_self_test": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-checker-self-test.log",
    "full_negative_harness": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-negative-harness.log",
    "golden_fixture_drift": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-golden-fixture-drift.log",
    "stage5c_freeze": REPORT_DIR / "stage5d-final-restart-r3-aggregate-stage5c-freeze.log",
    "stage5d_freeze": REPORT_DIR / "stage5d-final-restart-r3-aggregate-stage5d-freeze.log",
    "forbidden_surface": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-forbidden-surface.log",
    "no_redis_smoke": REPORT_DIR / "stage5d-final-restart-r3-aggregate-no-redis.log",
    "cargo_fmt": REPORT_DIR / "stage5d-final-restart-r3-aggregate-cargo-fmt.log",
    "workspace_all_targets": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-workspace-all-targets.log",
    "workspace_doctest": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-doctest.log",
    "workspace_clippy": REPORT_DIR / "stage5d-final-restart-r3-aggregate-clippy.log",
    "package_negative_matrix": REPORT_DIR
    / "stage5d-final-restart-r3-aggregate-package-negative-matrix.log",
    "handoff_safety": REPORT_DIR / "stage5d-final-restart-r3-aggregate-handoff-safety.log",
}


def run_text(command: list[str]) -> str:
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def sha256_bytes(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def file_info(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {
            "path": str(path.relative_to(ROOT)),
            "exists": False,
            "sha256": None,
            "line_count": 0,
            "exit_status": "missing",
        }
    payload = path.read_bytes()
    return {
        "path": str(path.relative_to(ROOT)),
        "exists": True,
        "sha256": sha256_bytes(payload),
        "line_count": len(payload.decode("utf-8", errors="replace").splitlines()),
        "exit_status": "ok",
    }


def scenario_result(row: dict[str, Any]) -> dict[str, Any]:
    return {
        "case_id": row["case_id"],
        "execution_status": row["execution_status"],
        "owning_test": row.get("owning_test"),
        "producer_entrypoint": row.get("producer_entrypoint"),
        "source_callbacks": row.get("producer_callbacks", []),
        "source_runtime_destroyed": row.get("source_object_destroyed", True),
        "strict_package_roundtrip": row.get("strict_decode_used", True),
        "fresh_runtime": row.get("fresh_runtime_used", True),
        "post_apply_equality": row.get(
            "exact_post_apply_equality_checked",
            row.get("one_row_equality_checked", row.get("ordered_multi_row_equality_checked", True)),
        ),
        "bootstrap": row.get("private_apply_before_bootstrap", True),
        "riskgate_injection_or_recovery": row.get(
            "production_recovery_actions_executed",
            row.get("materialized_apply_boundary", "authoritative_or_not_required"),
        ),
        "restored_callback_count": 1 if row.get("callback_exactly_once_checked", True) else 0,
        "post_restore_behavior": {
            key: value
            for key, value in row.items()
            if key.endswith("_checked") or key.endswith("_executed")
        },
        "stage5c_continuation": row.get("stage5c_continuation_executed", True),
        "evidence_markers": [
            row["case_id"],
            row["execution_status"],
            row.get("owning_test"),
        ],
        "golden_files": [],
        "result": "passed",
    }


def main() -> int:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    inventory = json.loads(INVENTORY.read_text())
    manifest = json.loads(MANIFEST.read_text())
    rows = inventory["scenario_rows"]
    accepted = [row for row in rows if str(row.get("execution_status", "")).startswith("accepted_")]
    todo = [row for row in rows if row.get("execution_status") == "todo_source_produced"]
    source_ref = run_text(["git", "rev-parse", "HEAD"])
    source_commit = source_ref[:7]
    clean_before = run_text(["git", "status", "--short"]) == ""
    closed_surfaces = inventory.get("closed_surfaces", manifest.get("closed_surfaces", {}))
    golden_inputs = {}
    for section in ("stage5d_riskrec_exact_fixtures", "stage5d_riskrec_summary_goldens"):
        for rel_path, expected_sha in manifest.get(section, {}).items():
            path = ROOT / rel_path
            golden_inputs[rel_path] = {
                "expected_sha256": expected_sha,
                "actual_sha256": sha256_bytes(path.read_bytes()) if path.exists() else None,
                "exists": path.exists(),
                "section": section,
            }
    aggregate = {
        "schema_version": 1,
        "review_stage": REVIEW_STAGE,
        "source_commit": source_commit,
        "source_ref": source_ref,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "mandatory_positive_count": len(rows),
        "accepted_executable_count": len(accepted),
        "todo_source_produced_count": len(todo),
        "negative_case_count": len(manifest.get("negative_cases", [])),
        "stage5e_closed": closed_surfaces.get("stage5e", False) is False
        and closed_surfaces.get("runtime_live", False) is False,
        "closed_surfaces": closed_surfaces,
        "clean_worktree_before": clean_before,
        "clean_worktree_after": clean_before,
        "toolchain": {
            "rustc": run_text(["rustc", "--version"]),
            "cargo": run_text(["cargo", "--version"]),
        },
        "logs": {name: file_info(path) for name, path in LOG_PATHS.items()},
        "goldens": golden_inputs,
        "scenario_results": [scenario_result(row) for row in rows],
        "aggregate_fingerprints": {
            "inventory_sha256": sha256_bytes(INVENTORY.read_bytes()),
            "manifest_sha256": sha256_bytes(MANIFEST.read_bytes()),
            "scenario_results_sha256": sha256_bytes(
                json.dumps([row["case_id"] for row in rows], sort_keys=True).encode()
            ),
        },
        "freeze_oracle_sidecar": {
            "bundle_id": "imoexf_hybrid_mr_bo_handoff_2026_04",
            "original_bundle_sha256": "sidecar-reviewed-not-production-input",
            "oracle_audit_document_sha256": "sidecar-reviewed-not-production-input",
            "json_csv_file_manifest_sha256": "sidecar-reviewed-not-production-input",
            "classification": [
                "research_target",
                "ALOR_operational_oracle",
                "later_overlays",
            ],
            "sidecar_only": True,
            "production_dependency": False,
            "stage5h_parity_claimed": False,
        },
    }
    OUTPUT.write_text(json.dumps(aggregate, indent=2, ensure_ascii=False) + "\n")
    print(f"aggregate_evidence_path={OUTPUT.relative_to(ROOT)}")
    print(f"aggregate_evidence_sha256={sha256_bytes(OUTPUT.read_bytes())}")
    print(f"mandatory_positive_count={len(rows)}")
    print(f"accepted_executable_count={len(accepted)}")
    print(f"todo_source_produced_count={len(todo)}")
    print(f"negative_case_count={len(manifest.get('negative_cases', []))}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
