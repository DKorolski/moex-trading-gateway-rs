#!/usr/bin/env python3
"""Generate M3j-20 working LimitCancel micro evidence.

No broker calls are performed by this script. It aggregates already-created
M3j-20 preflight, actual and post-run reports.
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


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()


def raw_capture_shape(path: Path) -> dict[str, Any]:
    result = artifact(path)
    if not path.exists():
        return result
    payload = load(path)
    raw_body = payload.get("raw_body")
    parsed: Any | None = None
    parse_error = None
    if isinstance(raw_body, str):
        try:
            parsed = json.loads(raw_body)
        except json.JSONDecodeError as error:
            parse_error = error.__class__.__name__
    result.update(
        {
            "capture_kind": payload.get("capture_kind"),
            "context": payload.get("context"),
            "status": payload.get("status"),
            "http_response_present": payload.get("http_response_present"),
            "raw_body_exported": False,
            "raw_body_len": len(raw_body) if isinstance(raw_body, str) else None,
            "raw_body_sha256": sha256_bytes(raw_body.encode()) if isinstance(raw_body, str) else None,
            "body_kind": "object" if isinstance(parsed, dict) else type(parsed).__name__ if parsed is not None else None,
            "body_top_level_keys": sorted(parsed.keys()) if isinstance(parsed, dict) else [],
            "body_parse_error": parse_error,
        }
    )
    return result


def broker_truth(report: dict[str, Any]) -> dict[str, Any]:
    truth = report.get("pre_boundary_broker_truth", {})
    return {
        "active_orders_count": truth.get("active_orders_count"),
        "unknown_active_orders_count": truth.get("unknown_active_orders_count"),
        "orphan_active_orders_count": truth.get("orphan_active_orders_count"),
        "positions_count": truth.get("positions_count"),
        "orders_total": truth.get("orders_total"),
        "terminal_or_ignored_orders_count": truth.get("terminal_or_ignored_orders_count"),
        "broker_truth_clean": truth.get("broker_truth_clean"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--preflight-report", type=Path, required=True)
    parser.add_argument("--actual-report", type=Path, required=True)
    parser.add_argument("--post-run-report", type=Path, required=True)
    parser.add_argument("--raw-place", type=Path, required=True)
    parser.add_argument("--raw-cancel", type=Path, required=True)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j20-working-limitcancel-evidence.json"),
    )
    args = parser.parse_args()

    preflight = load(args.preflight_report)
    actual = load(args.actual_report)
    post_run = load(args.post_run_report)
    execution = actual.get("execution_redacted", {})
    report = actual.get("report", {})
    pre_truth = broker_truth(preflight)
    post_truth = broker_truth(post_run)

    preflight_ok = (
        preflight.get("report", {}).get("actual_send_allowed") is True
        and pre_truth.get("active_orders_count") == 0
        and pre_truth.get("unknown_active_orders_count") == 0
        and pre_truth.get("orphan_active_orders_count") == 0
        and pre_truth.get("positions_count") == 0
        and pre_truth.get("broker_truth_clean") is True
    )
    actual_ok = (
        report.get("boundary_invocation_performed") is True
        and report.get("real_finam_order_endpoint_used") is True
        and execution.get("place_attempted") is True
        and execution.get("cancel_attempted") is True
        and execution.get("broker_order_id_present") is True
        and execution.get("place_response_kind") == "Accepted"
        and execution.get("cancel_response_kind") == "Accepted"
    )
    working_ok = (
        execution.get("working_observation_attempted") is True
        and execution.get("working_observed") is True
        and execution.get("working_observation_order_found") is True
        and (execution.get("working_observation_poll_count") or 0) >= 1
    )
    post_run_clean = (
        post_truth.get("active_orders_count") == 0
        and post_truth.get("unknown_active_orders_count") == 0
        and post_truth.get("orphan_active_orders_count") == 0
        and post_truth.get("positions_count") == 0
        and post_truth.get("broker_truth_clean") is True
    )
    boundaries_disabled = (
        report.get("runtime_live_attachment_allowed") is False
        and report.get("command_consumer_to_real_finam_allowed") is False
        and report.get("stop_sltp_bracket_replace_multileg_allowed") is False
    )
    raw_place = raw_capture_shape(args.raw_place)
    raw_cancel = raw_capture_shape(args.raw_cancel)
    raw_shapes_ok = (
        raw_place.get("status") == 200
        and raw_cancel.get("status") == 200
        and raw_place.get("raw_body_exported") is False
        and raw_cancel.get("raw_body_exported") is False
        and raw_place.get("body_parse_error") is None
        and raw_cancel.get("body_parse_error") is None
    )
    evidence_ready = all(
        [
            args.source_archive.exists(),
            preflight_ok,
            actual_ok,
            working_ok,
            post_run_clean,
            boundaries_disabled,
            raw_shapes_ok,
        ]
    )
    head = git_head()
    payload = {
        "evidence_kind": "m3j20-working-limitcancel-micro-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "source_archive": artifact(args.source_archive),
        "artifact_manifest": {
            "preflight_report": artifact(args.preflight_report),
            "actual_report": artifact(args.actual_report),
            "post_run_report": artifact(args.post_run_report),
        },
        "scope": {
            "symbol": "IMOEXF@RTSX",
            "side": "buy",
            "order_type": "limit",
            "qty": "1",
            "limit_price": "2210",
            "max_orders": 1,
            "place_observe_working_cancel_only": True,
            "no_stop_sltp_bracket_replace_multileg": True,
        },
        "preflight_broker_truth": pre_truth,
        "actual_execution_redacted": execution,
        "post_run_broker_truth": post_truth,
        "redacted_raw_response_shapes": {
            "place": raw_place,
            "cancel": raw_cancel,
            "raw_body_files_in_handoff": False,
            "raw_body_exported_to_evidence": False,
        },
        "checks": {
            "preflight_ok": preflight_ok,
            "actual_ok": actual_ok,
            "working_observation_ok": working_ok,
            "post_run_clean": post_run_clean,
            "boundaries_disabled": boundaries_disabled,
            "raw_shapes_ok": raw_shapes_ok,
        },
        "evidence_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["checks"] | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
