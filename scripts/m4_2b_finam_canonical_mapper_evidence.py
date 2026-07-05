#!/usr/bin/env python3
"""Generate M4-2b-1 FINAM canonical mapper evidence.

No broker calls are performed. The script validates that the M4-2b document is
present, that the synthetic FINAM/expected fixtures are present, and that the
FINAM mapper parity tests pass.
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

REQUIRED_ARTIFACTS = [
    Path("docs/m4-2b-finam-canonical-broker-truth-mapper.md"),
    Path("fixtures/finam/equivalent_positions_snapshot_zero_qty.json"),
    Path("fixtures/finam/equivalent_orders_active_terminal.json"),
    Path("fixtures/expected/canonical_truth_zero_qty_flat_summary.json"),
    Path("fixtures/expected/canonical_truth_order_summary.json"),
]


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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2b-finam-canonical-mapper-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [artifact(path) for path in REQUIRED_ARTIFACTS]
    broker_finam_m4_2b_tests = run(["cargo", "test", "-p", "broker-finam", "m4_2b"])
    broker_core_operational_tests = run(["cargo", "test", "-p", "broker-core", "operational_snapshot"])
    doc_text = (ROOT / "docs/m4-2b-finam-canonical-broker-truth-mapper.md").read_text()
    no_live_boundary_documented = "no new live position tests" in doc_text
    scope_limit_documented = "Trades and instrument specs remain explicit follow-up work" in doc_text

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "broker_finam_m4_2b_tests_ok": broker_finam_m4_2b_tests["exit_code"] == 0,
        "broker_core_operational_tests_ok": broker_core_operational_tests["exit_code"] == 0,
        "no_live_boundary_documented": no_live_boundary_documented,
        "scope_limit_documented": scope_limit_documented,
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload = {
        "evidence_kind": "m4-2b-1-finam-canonical-mapper-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "runtime_live_attachment_allowed": False,
        },
        "artifacts": artifacts,
        "test_commands": {
            "broker_finam_m4_2b": broker_finam_m4_2b_tests,
            "broker_core_operational_snapshot": broker_core_operational_tests,
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
