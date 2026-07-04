#!/usr/bin/env python3
"""Generate M4-1c tiny position market lifecycle evidence.

The script performs no broker calls. It aggregates the already-created M4-1c
CLI report and local-only raw capture metadata.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path), "exists": path.exists()}
    if path.exists():
        data = path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def raw_shape(path: Path) -> dict[str, Any]:
    result = artifact(path)
    if not path.exists():
        return result
    payload = load(path)
    body = payload.get("raw_body")
    parsed: Any | None = None
    parse_error = None
    if isinstance(body, str):
        try:
            parsed = json.loads(body)
        except json.JSONDecodeError as error:
            parse_error = error.__class__.__name__
    result.update(
        {
            "capture_kind": payload.get("capture_kind"),
            "context": payload.get("context"),
            "status": payload.get("status"),
            "http_response_present": payload.get("http_response_present"),
            "raw_body_exported": False,
            "raw_body_len": len(body) if isinstance(body, str) else None,
            "raw_body_sha256": sha256_bytes(body.encode()) if isinstance(body, str) else None,
            "body_top_level_keys": sorted(parsed.keys()) if isinstance(parsed, dict) else [],
            "body_parse_error": parse_error,
        }
    )
    return result


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--actual-report", type=Path, required=True)
    parser.add_argument("--post-run-report", type=Path)
    parser.add_argument("--raw-entry", type=Path, required=True)
    parser.add_argument("--raw-exit", type=Path, required=True)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-1c-tiny-position-market-evidence.json"),
    )
    args = parser.parse_args()

    report_payload = load(args.actual_report)
    post_run_payload = load(args.post_run_report) if args.post_run_report else None
    report = report_payload.get("report", {})
    execution = report_payload.get("execution_redacted", {})
    pre_truth = report_payload.get("pre_boundary_broker_truth", {})
    post_run_report = (post_run_payload or {}).get("report", {})
    post_run_truth = (post_run_payload or {}).get("pre_boundary_broker_truth", {})
    actual_ok = (
        report.get("boundary_invocation_performed") is True
        and report.get("real_finam_order_endpoint_used") is True
        and execution.get("entry_attempted") is True
        and execution.get("exit_attempted") is True
        and execution.get("entry_response_kind") == "Accepted"
        and execution.get("exit_response_kind") == "Accepted"
        and execution.get("entry_broker_order_id_present") is True
        and execution.get("exit_broker_order_id_present") is True
    )
    immediate_lifecycle_ok = (
        execution.get("position_observation_attempted") is True
        and execution.get("position_observed") is True
        and (execution.get("observed_positions_count") or 0) > 0
        and execution.get("final_positions_count") == 0
        and execution.get("final_active_orders_count") == 0
        and report.get("final_flat") is True
        and report.get("final_no_active_orders") is True
    )
    post_run_reconciliation_ok = (
        post_run_truth.get("positions_count") == 0
        and post_run_truth.get("active_orders_count") == 0
        and post_run_truth.get("unknown_active_orders_count") == 0
        and post_run_truth.get("broker_truth_clean") is True
        and post_run_report.get("final_flat") is True
        and post_run_report.get("final_no_active_orders") is True
        and post_run_report.get("boundary_invocation_performed") is False
        and post_run_report.get("real_finam_order_endpoint_used") is False
    )
    lifecycle_ok = (
        execution.get("position_observation_attempted") is True
        and execution.get("position_observed") is True
        and (execution.get("observed_positions_count") or 0) > 0
        and (immediate_lifecycle_ok or post_run_reconciliation_ok)
    )
    preflight_ok = (
        pre_truth.get("positions_count") == 0
        and pre_truth.get("active_orders_count") == 0
        and pre_truth.get("unknown_active_orders_count") == 0
        and pre_truth.get("broker_truth_clean") is True
    )
    boundaries_disabled = (
        report.get("runtime_live_attachment_allowed") is False
        and report.get("command_consumer_to_real_finam_allowed") is False
        and report.get("stop_sltp_bracket_replace_multileg_allowed") is False
    )
    raw_entry = raw_shape(args.raw_entry)
    raw_exit = raw_shape(args.raw_exit)
    raw_ok = (
        raw_entry.get("status") == 200
        and raw_exit.get("status") == 200
        and raw_entry.get("raw_body_exported") is False
        and raw_exit.get("raw_body_exported") is False
        and raw_entry.get("body_parse_error") is None
        and raw_exit.get("body_parse_error") is None
    )
    evidence_ready = (
        args.source_archive.exists()
        and actual_ok
        and lifecycle_ok
        and preflight_ok
        and boundaries_disabled
        and raw_ok
    )
    head = git_head()
    payload = {
        "evidence_kind": "m4-1c-tiny-position-market-lifecycle-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "source_archive": artifact(args.source_archive),
        "artifact_manifest": {
            "actual_report": artifact(args.actual_report),
            "post_run_report": artifact(args.post_run_report) if args.post_run_report else None,
            "raw_entry_local_only": artifact(args.raw_entry),
            "raw_exit_local_only": artifact(args.raw_exit),
        },
        "scope": report_payload.get("operator_scope", {}),
        "pre_boundary_broker_truth": pre_truth,
        "execution_redacted": execution,
        "report": report,
        "post_run_reconciliation": {
            "post_run_report_provided": args.post_run_report is not None,
            "immediate_lifecycle_ok": immediate_lifecycle_ok,
            "post_run_reconciliation_ok": post_run_reconciliation_ok,
            "post_run_broker_truth": post_run_truth,
            "post_run_report": post_run_report,
            "operator_broker_journal_summary": {
                "provided_by_operator": True,
                "instrument": "IMOEXF",
                "exchange": "MOEX",
                "buy_qty": 1,
                "buy_price": "2227.5",
                "buy_time_local": "2026-07-04 17:57:17 Europe/Moscow",
                "sell_qty": 1,
                "sell_price": "2227.0",
                "sell_time_local": "2026-07-04 17:57:18 Europe/Moscow",
                "round_trip_qty_net": 0,
            },
        },
        "redacted_raw_response_shapes": {
            "entry": raw_entry,
            "exit": raw_exit,
            "raw_body_files_in_handoff": False,
            "raw_body_exported_to_evidence": False,
        },
        "checks": {
            "actual_ok": actual_ok,
            "immediate_lifecycle_ok": immediate_lifecycle_ok,
            "post_run_reconciliation_ok": post_run_reconciliation_ok,
            "lifecycle_ok": lifecycle_ok,
            "preflight_ok": preflight_ok,
            "boundaries_disabled": boundaries_disabled,
            "raw_ok": raw_ok,
        },
        "evidence_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["checks"] | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
