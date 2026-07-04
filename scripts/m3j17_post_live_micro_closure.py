#!/usr/bin/env python3
"""Generate M3j-17 post-live-micro closure evidence.

This script intentionally does not export raw FINAM response bodies. It reads
local-only raw capture files, emits redacted body shape/keys/kinds and hashes,
and binds the closure package to a clean source archive.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
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


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def source_commit() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()


def json_kind(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int) and not isinstance(value, bool):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return "array"
    if isinstance(value, dict):
        return "object"
    return type(value).__name__


def redact_shape(value: Any, depth: int = 0, max_depth: int = 3) -> Any:
    if depth >= max_depth:
        return {"kind": json_kind(value)}
    if isinstance(value, dict):
        return {
            "kind": "object",
            "keys": sorted(value.keys()),
            "fields": {key: redact_shape(item, depth + 1, max_depth) for key, item in value.items()},
        }
    if isinstance(value, list):
        first = value[0] if value else None
        return {
            "kind": "array",
            "len": len(value),
            "first_item_shape": redact_shape(first, depth + 1, max_depth) if value else None,
        }
    return {"kind": json_kind(value)}


def recursive_keys(value: Any) -> list[str]:
    keys: list[str] = []
    if isinstance(value, dict):
        for key, item in value.items():
            keys.append(key)
            keys.extend(recursive_keys(item))
    elif isinstance(value, list):
        for item in value:
            keys.extend(recursive_keys(item))
    return keys


def has_broker_order_id_key(value: Any) -> bool:
    for key in recursive_keys(value):
        normalized = key.replace("_", "").replace("-", "").lower()
        if "order" in normalized and "id" in normalized:
            return True
    return False


def raw_capture_shape(path: Path) -> dict[str, Any]:
    result = artifact(path)
    if not path.exists():
        return result
    payload = load_json(path)
    raw_body = payload.get("raw_body")
    parsed_body: Any | None = None
    body_parse_error = None
    if isinstance(raw_body, str):
        try:
            parsed_body = json.loads(raw_body)
        except json.JSONDecodeError as error:
            body_parse_error = error.__class__.__name__
    result.update(
        {
            "capture_kind": payload.get("capture_kind"),
            "context": payload.get("context"),
            "http_response_present": payload.get("http_response_present"),
            "status": payload.get("status"),
            "retry_after_ms": payload.get("retry_after_ms"),
            "raw_body_exported": False,
            "raw_body_present": isinstance(raw_body, str),
            "raw_body_len": len(raw_body) if isinstance(raw_body, str) else None,
            "raw_body_sha256": sha256_bytes(raw_body.encode()) if isinstance(raw_body, str) else None,
            "body_parse_error": body_parse_error,
            "body_kind": json_kind(parsed_body) if parsed_body is not None else None,
            "body_top_level_keys": sorted(parsed_body.keys()) if isinstance(parsed_body, dict) else [],
            "broker_order_id_key_present": has_broker_order_id_key(parsed_body),
            "redacted_body_shape": redact_shape(parsed_body) if parsed_body is not None else None,
        }
    )
    return result


def truth_snapshot(report: dict[str, Any]) -> dict[str, Any]:
    truth = report.get("pre_boundary_broker_truth", {})
    return {
        "source_field_name": "pre_boundary_broker_truth",
        "semantic_role": "broker_truth_snapshot",
        "positions_count": truth.get("positions_count"),
        "orders_total": truth.get("orders_total"),
        "active_orders_count": truth.get("active_orders_count"),
        "unknown_active_orders_count": truth.get("unknown_active_orders_count"),
        "orphan_active_orders_count": truth.get("orphan_active_orders_count"),
        "terminal_or_ignored_orders_count": truth.get("terminal_or_ignored_orders_count"),
        "broker_truth_clean": truth.get("broker_truth_clean"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--pre-send-report", type=Path, required=True)
    parser.add_argument("--actual-report", type=Path, required=True)
    parser.add_argument("--post-run-report", type=Path, required=True)
    parser.add_argument("--auth-report", type=Path, required=True)
    parser.add_argument("--raw-place", type=Path, required=True)
    parser.add_argument("--raw-cancel", type=Path, required=True)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j17-post-live-micro-closure-evidence.json"),
    )
    args = parser.parse_args()

    pre_send = load_json(args.pre_send_report)
    actual = load_json(args.actual_report)
    post_run = load_json(args.post_run_report)
    auth = load_json(args.auth_report)

    actual_report = actual.get("report", {})
    actual_execution = actual.get("execution_redacted", {})
    post_truth = truth_snapshot(post_run)
    auth_ok = (
        auth.get("auth", {}).get("auth_http") == 200
        and auth.get("details", {}).get("details_http") == 200
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
    post_run_clean = (
        post_truth.get("active_orders_count") == 0
        and post_truth.get("unknown_active_orders_count") == 0
        and post_truth.get("orphan_active_orders_count") == 0
        and post_truth.get("positions_count") == 0
        and post_truth.get("broker_truth_clean") is True
    )
    actual_ok = (
        actual_report.get("boundary_invocation_performed") is True
        and actual_report.get("real_finam_order_endpoint_used") is True
        and actual_execution.get("place_attempted") is True
        and actual_execution.get("cancel_attempted") is True
        and actual_execution.get("broker_order_id_present") is True
        and actual_execution.get("place_response_kind") == "Accepted"
        and actual_execution.get("cancel_response_kind") == "Accepted"
    )
    continuous_runtime_still_disabled = (
        actual_report.get("runtime_live_attachment_allowed") is False
        and actual_report.get("command_consumer_to_real_finam_allowed") is False
        and actual_report.get("stop_sltp_bracket_replace_multileg_allowed") is False
    )

    commit = source_commit()
    evidence_ready = (
        args.source_archive.exists()
        and auth_ok
        and raw_shapes_ok
        and actual_ok
        and post_run_clean
        and continuous_runtime_still_disabled
    )
    payload = {
        "evidence_kind": "m3j17-post-live-micro-closure-v1",
        "source_commit_full_sha": commit,
        "source_commit_short_sha": commit[:7],
        "source_archive": artifact(args.source_archive),
        "artifact_manifest": {
            "pre_send_report": artifact(args.pre_send_report),
            "actual_report": artifact(args.actual_report),
            "post_run_report": artifact(args.post_run_report),
            "auth_report": artifact(args.auth_report),
        },
        "auth_evidence": {
            "single_json_object": isinstance(auth, dict),
            "auth_http": auth.get("auth", {}).get("auth_http"),
            "details_http": auth.get("details", {}).get("details_http"),
            "raw_secret_exported": auth.get("raw_secret_exported"),
            "raw_jwt_exported": auth.get("raw_jwt_exported"),
            "auth_ok": auth_ok,
        },
        "pre_send_broker_truth_snapshot": truth_snapshot(pre_send),
        "actual_execution_redacted": actual_execution,
        "actual_report": actual_report,
        "post_run_broker_truth_snapshot": post_truth,
        "redacted_raw_response_shapes": {
            "place": raw_place,
            "cancel": raw_cancel,
            "raw_body_files_in_handoff": False,
            "raw_body_exported_to_evidence": False,
        },
        "scope": {
            "symbol": "IMOEXF@RTSX",
            "side": "buy",
            "order_type": "limit",
            "qty": "1",
            "limit_price": "2210",
            "max_orders": 1,
            "place_then_cancel_only": True,
            "no_stop_sltp_bracket_replace_multileg": True,
        },
        "closure_checks": {
            "actual_ok": actual_ok,
            "post_run_clean": post_run_clean,
            "raw_shapes_ok": raw_shapes_ok,
            "auth_ok": auth_ok,
            "continuous_runtime_still_disabled": continuous_runtime_still_disabled,
            "operator_signoff_status": "PendingOperatorSignoff",
        },
        "closure_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["closure_checks"], ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
