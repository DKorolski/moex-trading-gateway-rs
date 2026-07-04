#!/usr/bin/env python3
"""Generate M4-2a ALOR operational oracle evidence.

This script performs no broker calls. It validates that the M4-2a no-live
oracle package is present, that the parity gap matrix names the required P0
gaps, that synthetic ALOR/FINAM/expected fixtures are committed, and that the
broker-neutral operational snapshot tests pass.
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

REQUIRED_DOCS = [
    Path("docs/m4-1c-a-alor-parity-gap-audit.md"),
    Path("docs/m4-2a-alor-operational-oracle.md"),
    Path("docs/m4-2a-parity-test-gap-matrix.md"),
]

REQUIRED_FIXTURES = [
    Path("fixtures/alor/positions_snapshot_zero_qty.json"),
    Path("fixtures/alor/orders_active_terminal.json"),
    Path("fixtures/alor/readiness_snapshot_synced.json"),
    Path("fixtures/finam/equivalent_positions_snapshot_zero_qty.json"),
    Path("fixtures/finam/equivalent_orders_active_terminal.json"),
    Path("fixtures/expected/canonical_truth_zero_qty_flat_summary.json"),
    Path("fixtures/expected/canonical_truth_order_summary.json"),
]

P0_IDS = [f"P0-{idx}" for idx in range(1, 11)]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def run(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_bytes(completed.stdout.encode()),
        "stderr_sha256": sha256_bytes(completed.stderr.encode()),
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def load_json(path: Path) -> Any:
    return json.loads((ROOT / path).read_text())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2a-alor-oracle-evidence.json"),
    )
    args = parser.parse_args()

    docs = [artifact(path) for path in REQUIRED_DOCS]
    fixtures = [artifact(path) for path in REQUIRED_FIXTURES]
    matrix_text = (ROOT / "docs/m4-2a-parity-test-gap-matrix.md").read_text()
    p0_matrix_complete = all(p0_id in matrix_text for p0_id in P0_IDS)
    no_live_boundary_documented = "No live calls" in (
        ROOT / "docs/m4-2a-alor-operational-oracle.md"
    ).read_text()

    expected_position = load_json(Path("fixtures/expected/canonical_truth_zero_qty_flat_summary.json"))
    expected_order = load_json(Path("fixtures/expected/canonical_truth_order_summary.json"))
    fixture_invariants_ok = (
        expected_position.get("target_open_positions_count") == 0
        and expected_position.get("account_open_positions_count") == 1
        and expected_order.get("target_active_orders_count") == 0
        and expected_order.get("target_terminal_orders_count") == 1
        and expected_order.get("account_active_orders_count") == 1
        and expected_order.get("other_symbol_active_orders_count") == 1
    )

    core_tests = run(["cargo", "test", "-p", "broker-core", "operational_snapshot"])

    checks = {
        "docs_present": all(item["exists"] for item in docs),
        "fixtures_present": all(item["exists"] for item in fixtures),
        "p0_matrix_complete": p0_matrix_complete,
        "no_live_boundary_documented": no_live_boundary_documented,
        "fixture_invariants_ok": fixture_invariants_ok,
        "broker_core_operational_tests_ok": core_tests["exit_code"] == 0,
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload = {
        "evidence_kind": "m4-2a-alor-operational-oracle-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "runtime_live_attachment_allowed": False,
        },
        "docs": docs,
        "fixtures": fixtures,
        "p0_gap_ids": P0_IDS,
        "test_commands": {
            "broker_core_operational_snapshot": core_tests,
        },
        "checks": checks,
        "evidence_ready_for_review": evidence_ready,
    }

    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(checks | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
