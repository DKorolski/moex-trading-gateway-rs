#!/usr/bin/env python3
"""Generate M4-2d canonical BrokerTruthSnapshot integration evidence.

No broker calls are performed. The script validates that M4-2d code/doc
artifacts are present, that M4-1c evidence/preflight is canonical-truth gated,
and that scoped Rust tests pass.
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

DOC = Path("docs/m4-2d-canonical-broker-truth-integration.md")
MAPPER = Path("crates/broker-finam/src/mapper.rs")
CLI = Path("crates/broker-cli/src/main.rs")
M4_1C_EVIDENCE = Path("scripts/m4_1c_tiny_position_market_evidence.py")

DOC_MARKERS = [
    "No live calls",
    "Live expansion remains blocked",
    "map_finam_broker_truth_snapshot_with_readonly_artifacts",
    "BrokerTradeSnapshot",
    "BrokerInstrumentSpec",
    "BrokerReadinessSnapshot",
    "truth_source",
    "BrokerTruthSnapshot",
    "Margin sufficiency waiver",
    "explicit M4-2d waiver",
]

MAPPER_MARKERS = [
    "map_finam_broker_truth_snapshot_with_readonly_artifacts",
    "map_account_trade_to_broker_trade_snapshot",
    "map_finam_instrument_spec",
    "map_finam_broker_readiness_snapshot",
    "FinamInstrumentSpecArtifacts",
    "m4_2d_enriched_broker_truth_maps_trades_instrument_spec_and_readiness",
    "m4_2d_same_ticker_different_mic_is_not_same_instrument",
    "m4_2d_round_trip_trades_explain_flat_position_delta",
]

CLI_MARKERS = [
    "map_finam_broker_truth_snapshot",
    "summarize_for_instrument",
    "target_is_flat",
    "target_position_qty",
    '"truth_source": "BrokerTruthSnapshot"',
    '"canonical_summary"',
    '"final_truth_source": "BrokerTruthSnapshot"',
]

M4_1C_EVIDENCE_MARKERS = [
    "canonical_truth_ok",
    'truth.get("truth_source") == "BrokerTruthSnapshot"',
    "pre_truth_canonical_ok",
    "post_run_truth_canonical_ok",
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


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    text = (ROOT / path).read_text()
    missing = [marker for marker in markers if marker not in text]
    return {"path": str(path), "ok": not missing, "missing": missing, "checked": markers}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2d-canonical-integration-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [artifact(path) for path in [DOC, MAPPER, CLI, M4_1C_EVIDENCE]]
    doc_check = marker_check(DOC, DOC_MARKERS)
    mapper_check = marker_check(MAPPER, MAPPER_MARKERS)
    cli_check = marker_check(CLI, CLI_MARKERS)
    m4_1c_evidence_check = marker_check(M4_1C_EVIDENCE, M4_1C_EVIDENCE_MARKERS)

    broker_finam_m4_2d = run(["cargo", "test", "-p", "broker-finam", "m4_2d"])
    broker_finam_m4_2b = run(["cargo", "test", "-p", "broker-finam", "m4_2b"])
    broker_core_operational = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_cli_m4_1c = run(
        ["cargo", "test", "-p", "broker-cli", "m4_1c", "--no-default-features"]
    )
    m4_1c_py_compile = run(["python3", "-m", "py_compile", str(M4_1C_EVIDENCE)])

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_live_calls_performed": True,
        "finam_trades_to_broker_trade_snapshot_ok": mapper_check["ok"]
        and broker_finam_m4_2d["exit_code"] == 0,
        "finam_instrument_spec_mapper_ok": mapper_check["ok"]
        and broker_finam_m4_2d["exit_code"] == 0,
        "finam_readiness_snapshot_ok": mapper_check["ok"]
        and broker_finam_m4_2d["exit_code"] == 0,
        "m4_1c_canonical_preflight_reconciliation_ok": cli_check["ok"]
        and m4_1c_evidence_check["ok"]
        and broker_cli_m4_1c["exit_code"] == 0
        and m4_1c_py_compile["exit_code"] == 0,
        "same_ticker_different_mic_test_ok": broker_finam_m4_2d["exit_code"] == 0,
        "trade_fill_flat_delta_test_ok": broker_finam_m4_2d["exit_code"] == 0,
        "margin_sufficiency_waiver_documented": doc_check["ok"],
        "broker_finam_m4_2b_regression_ok": broker_finam_m4_2b["exit_code"] == 0,
        "broker_core_operational_tests_ok": broker_core_operational["exit_code"] == 0,
        "live_expansion_blocked": doc_check["ok"],
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2d-canonical-broker-truth-integration-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "mapper": mapper_check,
            "broker_cli_m4_1c": cli_check,
            "m4_1c_evidence": m4_1c_evidence_check,
        },
        "test_commands": {
            "broker_finam_m4_2d": broker_finam_m4_2d,
            "broker_finam_m4_2b": broker_finam_m4_2b,
            "broker_core_operational": broker_core_operational,
            "broker_cli_m4_1c_no_default_features": broker_cli_m4_1c,
            "m4_1c_evidence_py_compile": m4_1c_py_compile,
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
