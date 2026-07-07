#!/usr/bin/env python3
"""Generate M4-3x FINAM paper vs ALOR runtime-state parity evidence.

The script performs read-only Redis stream reads. It does not place/cancel
orders and does not export raw broker/runtime payloads; the report contains
only selected normalized fields, stream diagnostics, and comparison results.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import shlex
import subprocess
from pathlib import Path
from typing import Any


DEFAULT_FINAM_STREAM = "finam_imoexf_paper:runtime:state:hybrid_intraday:imoexf"
DEFAULT_FINAM_WS_SOURCE_STREAM = "finam_imoexf_paper:ws:market_data"
DEFAULT_FINAM_DLQ_STREAM = "finam_imoexf_paper:runtime:dlq"
DEFAULT_ALOR_STREAM = "runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.PORTFOLIO_ID"
DEFAULT_CONSUMER_GROUP = "finam-imoexf-paper-runtime-m1"
DEFAULT_REPORT = Path("reports/parity/finam-vs-alor-runtime-state/m4-3x-runtime-state-parity.json")

HYBRID_FIELDS = [
    "active_cycle_id",
    "next_cycle_seq",
    "last_position_qty",
    "current_owner",
    "current_side",
    "pending_entry_owner",
    "pending_entry_side",
    "pending_entry_cycle_id",
    "pending_entry_request_id",
    "pending_exit_request_id",
    "tp_order_id",
    "sl_stop_order_id",
    "mr_take_price",
    "mr_stop_price",
    "safe_mode_close_only",
    "safe_mode_reason",
    "deferred_entry_state",
    "deferred_exit_state",
    "position_adoption_state",
    "dirty_start_marker",
    "manual_intervention_required",
    "manual_intervention_reason",
    "entry_ready",
    "last_bar_close",
    "prev_day_close",
    "last_day_local",
    "current_day_high",
    "current_day_low",
    "current_day_close",
    "prev_day_range",
    "prev_day_return",
    "day_before_close",
    "today_start_local",
    "was_long_today",
    "was_short_today",
    "overnight_exit_armed_date",
    "risk_gate_shadow_session_date",
    "risk_gate_shadow_pnl_points",
    "risk_gate_shadow_trade_count",
    "risk_gate_mr_enabled_current_session",
    "risk_gate_mr_enabled_next_session",
    "risk_gate_rolling_sum_lb120",
    "risk_gate_last_finalized_session_date",
    "risk_gate_ledger_rows_count",
]

OHLCV_DIAGNOSTIC_FIELDS = [
    "last_bar_close",
    "current_day_high",
    "current_day_low",
    "current_day_close",
]


def run(command: list[str], timeout: int = 60) -> str:
    completed = subprocess.run(
        command,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
    )
    return completed.stdout


def run_safe(command: list[str], timeout: int = 60) -> tuple[bool, str]:
    try:
        return True, run(command, timeout=timeout)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        return False, f"{type(error).__name__}: {error}"


def source_commit_from_git() -> str | None:
    ok, stdout = run_safe(["git", "rev-parse", "HEAD"], timeout=10)
    return stdout.strip() if ok else None


def redis_command(prefix: str, *args: str) -> list[str]:
    command = shlex.split(prefix)
    command.extend(args)
    return command


def latest_payload(prefix: str, stream: str) -> dict[str, Any]:
    ok, stdout = run_safe(redis_command(prefix, "XREVRANGE", stream, "+", "-", "COUNT", "1"))
    if not ok:
        return {"stream": stream, "stream_id": None, "payload": None, "error": stdout}
    lines = stdout.splitlines()
    if len(lines) < 3:
        return {"stream": stream, "stream_id": None, "payload": None, "error": "empty_stream"}
    stream_id = lines[0]
    payload = None
    for index, line in enumerate(lines[:-1]):
        if line == "payload":
            payload = lines[index + 1]
            break
    if payload is None:
        return {"stream": stream, "stream_id": stream_id, "payload": None, "error": "missing_payload"}
    try:
        parsed = json.loads(payload)
    except json.JSONDecodeError as error:
        return {"stream": stream, "stream_id": stream_id, "payload": None, "error": str(error)}
    return {"stream": stream, "stream_id": stream_id, "payload": parsed, "error": None}


def redis_xlen(prefix: str, stream: str) -> dict[str, Any]:
    ok, stdout = run_safe(redis_command(prefix, "XLEN", stream), timeout=30)
    if not ok:
        return {"stream": stream, "length": None, "error": stdout}
    try:
        length = int(stdout.strip() or "0")
    except ValueError:
        return {"stream": stream, "length": None, "error": f"unexpected_xlen_output:{stdout!r}"}
    return {"stream": stream, "length": length, "error": None}


def redis_xpending(prefix: str, stream: str, group: str) -> dict[str, Any]:
    ok, stdout = run_safe(redis_command(prefix, "XPENDING", stream, group), timeout=30)
    if not ok:
        return {"stream": stream, "consumer_group": group, "pending_count": None, "error": stdout}
    lines = stdout.splitlines()
    if not lines:
        return {"stream": stream, "consumer_group": group, "pending_count": 0, "error": None}
    try:
        pending_count = int(lines[0])
    except ValueError:
        return {
            "stream": stream,
            "consumer_group": group,
            "pending_count": None,
            "error": f"unexpected_xpending_output:{stdout!r}",
        }
    return {
        "stream": stream,
        "consumer_group": group,
        "pending_count": pending_count,
        "oldest_pending_id": lines[1] if len(lines) > 1 and lines[1] != "" else None,
        "newest_pending_id": lines[2] if len(lines) > 2 and lines[2] != "" else None,
        "error": None,
    }


def redis_xinfo_group(prefix: str, stream: str, group: str) -> dict[str, Any]:
    ok, stdout = run_safe(redis_command(prefix, "XINFO", "GROUPS", stream), timeout=30)
    if not ok:
        return {"stream": stream, "consumer_group": group, "lag": None, "error": stdout}
    lines = stdout.splitlines()
    current: dict[str, Any] = {}
    groups: list[dict[str, Any]] = []
    index = 0
    while index + 1 < len(lines):
        key = lines[index]
        value = lines[index + 1]
        if key == "name" and current:
            groups.append(current)
            current = {}
        current[key] = value
        index += 2
    if current:
        groups.append(current)
    for item in groups:
        if item.get("name") == group:
            return {
                "stream": stream,
                "consumer_group": group,
                "consumers": parse_int_or_none(item.get("consumers")),
                "pending": parse_int_or_none(item.get("pending")),
                "last_delivered_id": item.get("last-delivered-id"),
                "entries_read": parse_int_or_none(item.get("entries-read")),
                "lag": parse_int_or_none(item.get("lag")),
                "error": None,
            }
    return {
        "stream": stream,
        "consumer_group": group,
        "lag": None,
        "error": "consumer_group_not_found",
    }


def parse_int_or_none(value: Any) -> int | None:
    if value in (None, ""):
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def unwrap_finam_runtime_state(payload: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    runtime_state = payload.get("payload", {}).get("RuntimeState", {})
    return runtime_state, runtime_state.get("hybrid_intraday", {})


def unwrap_alor_runtime_state(payload: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    outer = payload.get("payload", payload)
    hybrid = (
        outer.get("strategy_state", {})
        .get("payload", {})
        .get("HybridIntradayRuntime", {})
    )
    return outer, hybrid


def parse_field_set(value: str | None) -> set[str]:
    if not value:
        return set()
    return {item.strip() for item in value.split(",") if item.strip()}


def compare_fields(
    finam: dict[str, Any],
    alor: dict[str, Any],
    expected_fields: set[str],
    waived_fields: set[str],
) -> list[dict[str, Any]]:
    rows = []
    for field in HYBRID_FIELDS:
        left = finam.get(field)
        right = alor.get(field)
        same = left == right
        if same:
            classification = "Same"
            readiness_impact = "None"
        elif field in waived_fields:
            classification = "Waived"
            readiness_impact = "DoesNotBlock"
        elif field in expected_fields:
            classification = "Expected"
            readiness_impact = "DoesNotBlock"
        else:
            classification = "Blocker"
            readiness_impact = "BlocksReadiness"
        rows.append(
            {
                "field": field,
                "finam": left,
                "alor": right,
                "same": same,
                "classification": classification,
                "readiness_impact": readiness_impact,
            }
        )
    return rows


def numeric_delta(left: Any, right: Any) -> str | None:
    if left is None or right is None:
        return None
    try:
        return str(float(left) - float(right))
    except (TypeError, ValueError):
        return None


def ohlcv_deltas(finam: dict[str, Any], alor: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for field in OHLCV_DIAGNOSTIC_FIELDS:
        left = finam.get(field)
        right = alor.get(field)
        rows.append(
            {
                "field": field,
                "finam": left,
                "alor": right,
                "delta": numeric_delta(left, right),
                "same": left == right,
            }
        )
    return rows


def safety_flags(runtime_state: dict[str, Any]) -> dict[str, Any]:
    boundary = runtime_state.get("safety_boundary", {})
    return {
        "live_orders_enabled": boundary.get("live_orders_enabled"),
        "runtime_live_ready_enabled": boundary.get("runtime_live_ready_enabled"),
        "command_consumer_to_real_finam_enabled": boundary.get(
            "command_consumer_to_real_finam_enabled"
        ),
        "external_order_endpoint_enabled": boundary.get("external_order_endpoint_enabled"),
        "stop_sltp_bracket_enabled": boundary.get("stop_sltp_bracket_enabled"),
    }


def infer_unseeded(args: argparse.Namespace, finam_hybrid: dict[str, Any], alor_hybrid: dict[str, Any]) -> bool:
    if not args.seed_required:
        return False
    finam_rows = finam_hybrid.get("risk_gate_ledger_rows_count")
    alor_rows = alor_hybrid.get("risk_gate_ledger_rows_count")
    finam_seq = finam_hybrid.get("next_cycle_seq")
    alor_seq = alor_hybrid.get("next_cycle_seq")
    return (finam_rows in (None, 0) and alor_rows not in (None, 0)) or (
        finam_seq in (None, 0) and alor_seq not in (None, 0)
    )


def final_status(
    evidence_complete: bool,
    safety_closed: bool,
    unseeded: bool,
    blocker_count: int,
    expected_count: int,
    waived_count: int,
) -> str:
    if not evidence_complete:
        return "EvidenceIncomplete"
    if not safety_closed:
        return "SafetyBoundaryOpen"
    if unseeded:
        return "Unseeded"
    if blocker_count:
        return "BlockedDivergence"
    if expected_count or waived_count:
        return "ExpectedDivergenceOnly"
    return "Synchronized"


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    expected_fields = parse_field_set(args.expected_divergence_fields)
    waived_fields = parse_field_set(args.waived_divergence_fields)
    finam_latest = latest_payload(args.finam_redis_cli_prefix, args.finam_stream)
    alor_latest = latest_payload(args.alor_redis_cli_prefix, args.alor_stream)
    finam_payload = finam_latest["payload"]
    alor_payload = alor_latest["payload"]
    evidence_complete = finam_payload is not None and alor_payload is not None

    finam_runtime: dict[str, Any] = {}
    finam_hybrid: dict[str, Any] = {}
    alor_runtime: dict[str, Any] = {}
    alor_hybrid: dict[str, Any] = {}
    if evidence_complete:
        finam_runtime, finam_hybrid = unwrap_finam_runtime_state(finam_payload)
        alor_runtime, alor_hybrid = unwrap_alor_runtime_state(alor_payload)

    rows = compare_fields(finam_hybrid, alor_hybrid, expected_fields, waived_fields)
    diff_rows = [row for row in rows if not row["same"]]
    blocker_count = sum(1 for row in diff_rows if row["classification"] == "Blocker")
    expected_count = sum(1 for row in diff_rows if row["classification"] == "Expected")
    waived_count = sum(1 for row in diff_rows if row["classification"] == "Waived")
    safety = safety_flags(finam_runtime)
    safety_closed = evidence_complete and all(value is False for value in safety.values())
    unseeded = evidence_complete and infer_unseeded(args, finam_hybrid, alor_hybrid)
    status = final_status(
        evidence_complete=evidence_complete,
        safety_closed=safety_closed,
        unseeded=unseeded,
        blocker_count=blocker_count,
        expected_count=expected_count,
        waived_count=waived_count,
    )

    source_commit = args.source_commit or source_commit_from_git()
    stream_diagnostics = {
        "finam_ws_source": redis_xlen(args.finam_redis_cli_prefix, args.finam_ws_source_stream),
        "finam_runtime_state": redis_xlen(args.finam_redis_cli_prefix, args.finam_stream),
        "finam_runtime_dlq": redis_xlen(args.finam_redis_cli_prefix, args.finam_dlq_stream),
        "finam_source_pending": redis_xpending(
            args.finam_redis_cli_prefix,
            args.finam_ws_source_stream,
            args.consumer_group,
        ),
        "finam_source_consumer_group": redis_xinfo_group(
            args.finam_redis_cli_prefix,
            args.finam_ws_source_stream,
            args.consumer_group,
        ),
    }

    return {
        "schema": "m4_3x_runtime_state_parity_evidence_v2",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "source_commit": source_commit,
        "vps_host": args.vps_host,
        "raw_payload_exported": False,
        "redis_calls_performed": True,
        "seed_required": args.seed_required,
        "streams": {
            "finam_ws_source_stream": args.finam_ws_source_stream,
            "finam_runtime_state_stream": args.finam_stream,
            "finam_runtime_dlq_stream": args.finam_dlq_stream,
            "alor_runtime_state_stream": args.alor_stream,
            "consumer_group": args.consumer_group,
        },
        "stream_diagnostics": stream_diagnostics,
        "finam": {
            "stream": args.finam_stream,
            "stream_id": finam_latest["stream_id"],
            "read_error": finam_latest["error"],
            "last_bar_key": finam_runtime.get("last_bar_key"),
            "updated_ts": finam_runtime.get("updated_ts"),
            "paper_only": finam_payload.get("paper_only") if finam_payload else None,
            "safety_flags": safety,
            "safety_closed": safety_closed,
        },
        "alor": {
            "stream": args.alor_stream,
            "stream_id": alor_latest["stream_id"],
            "read_error": alor_latest["error"],
            "last_processed_bar_ts": alor_runtime.get("last_processed_bar_ts"),
            "ts_utc": alor_runtime.get("ts_utc"),
        },
        "bar_comparison": {
            "finam_last_bar_key": finam_runtime.get("last_bar_key"),
            "alor_last_processed_bar_ts": alor_runtime.get("last_processed_bar_ts"),
            "ohlcv_deltas": ohlcv_deltas(finam_hybrid, alor_hybrid),
        },
        "comparison": {
            "field_count": len(rows),
            "same_count": len(rows) - len(diff_rows),
            "diff_count": len(diff_rows),
            "expected_divergence_count": expected_count,
            "waived_divergence_count": waived_count,
            "blocker_divergence_count": blocker_count,
            "unexplained_divergence_count": blocker_count,
            "status": status,
            "rows": rows,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--finam-redis-cli-prefix", default="redis-cli --raw")
    parser.add_argument("--alor-redis-cli-prefix", default="redis-cli --raw")
    parser.add_argument("--finam-ws-source-stream", default=DEFAULT_FINAM_WS_SOURCE_STREAM)
    parser.add_argument("--finam-stream", default=DEFAULT_FINAM_STREAM)
    parser.add_argument("--finam-dlq-stream", default=DEFAULT_FINAM_DLQ_STREAM)
    parser.add_argument("--alor-stream", default=DEFAULT_ALOR_STREAM)
    parser.add_argument("--consumer-group", default=DEFAULT_CONSUMER_GROUP)
    parser.add_argument("--source-commit")
    parser.add_argument("--vps-host", default="<VPS_HOST>")
    parser.add_argument("--seed-required", action="store_true")
    parser.add_argument("--expected-divergence-fields")
    parser.add_argument("--waived-divergence-fields")
    parser.add_argument("--output", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()

    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
