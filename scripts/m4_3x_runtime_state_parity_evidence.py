#!/usr/bin/env python3
"""Generate M4-3x FINAM paper vs ALOR runtime-state parity evidence.

The script performs read-only Redis stream reads. It does not place/cancel
orders and does not export raw broker/runtime payloads; the report contains
only selected normalized fields and comparison results.
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
DEFAULT_ALOR_STREAM = "runtime.state.hybrid_intraday.live.riskgate_shadow.imoexf.PORTFOLIO_ID"
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


def redis_command(prefix: str, stream: str) -> list[str]:
    command = shlex.split(prefix)
    command.extend(["XREVRANGE", stream, "+", "-", "COUNT", "1"])
    return command


def latest_payload(prefix: str, stream: str) -> tuple[str | None, dict[str, Any] | None]:
    stdout = run(redis_command(prefix, stream))
    lines = stdout.splitlines()
    if len(lines) < 3:
        return None, None
    stream_id = lines[0]
    payload = None
    for index, line in enumerate(lines[:-1]):
        if line == "payload":
            payload = lines[index + 1]
            break
    if payload is None:
        return stream_id, None
    return stream_id, json.loads(payload)


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


def compare_fields(finam: dict[str, Any], alor: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for field in HYBRID_FIELDS:
        left = finam.get(field)
        right = alor.get(field)
        rows.append(
            {
                "field": field,
                "finam": left,
                "alor": right,
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


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    finam_id, finam_payload = latest_payload(args.finam_redis_cli_prefix, args.finam_stream)
    alor_id, alor_payload = latest_payload(args.alor_redis_cli_prefix, args.alor_stream)
    if finam_payload is None:
        raise RuntimeError("FINAM runtime-state payload missing")
    if alor_payload is None:
        raise RuntimeError("ALOR runtime-state payload missing")

    finam_runtime, finam_hybrid = unwrap_finam_runtime_state(finam_payload)
    alor_runtime, alor_hybrid = unwrap_alor_runtime_state(alor_payload)
    rows = compare_fields(finam_hybrid, alor_hybrid)
    diff_rows = [row for row in rows if not row["same"]]
    safety = safety_flags(finam_runtime)
    safety_closed = all(value is False for value in safety.values())

    return {
        "schema": "m4_3x_runtime_state_parity_evidence_v1",
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "raw_payload_exported": False,
        "redis_calls_performed": True,
        "finam": {
            "stream": args.finam_stream,
            "stream_id": finam_id,
            "last_bar_key": finam_runtime.get("last_bar_key"),
            "updated_ts": finam_runtime.get("updated_ts"),
            "paper_only": finam_payload.get("paper_only"),
            "safety_flags": safety,
            "safety_closed": safety_closed,
        },
        "alor": {
            "stream": args.alor_stream,
            "stream_id": alor_id,
            "last_processed_bar_ts": alor_runtime.get("last_processed_bar_ts"),
            "ts_utc": alor_runtime.get("ts_utc"),
        },
        "comparison": {
            "field_count": len(rows),
            "same_count": len(rows) - len(diff_rows),
            "diff_count": len(diff_rows),
            "unexplained_divergence_count": len(diff_rows),
            "status": "Synchronized" if not diff_rows and safety_closed else "Diverged",
            "rows": rows,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--finam-redis-cli-prefix", default="redis-cli --raw")
    parser.add_argument("--alor-redis-cli-prefix", default="redis-cli --raw")
    parser.add_argument("--finam-stream", default=DEFAULT_FINAM_STREAM)
    parser.add_argument("--alor-stream", default=DEFAULT_ALOR_STREAM)
    parser.add_argument("--output", type=Path, default=DEFAULT_REPORT)
    args = parser.parse_args()

    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
