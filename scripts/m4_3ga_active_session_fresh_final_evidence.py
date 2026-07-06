#!/usr/bin/env python3
"""Generate M4-3g-a active-session FINAM WS fresh-final evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = Path("docs/m4-3g-a-active-session-fresh-final-evidence.md")
README = Path("README.md")
CLI_SOURCE = Path("crates/broker-cli/src/main.rs")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def run(cmd: list[str], timeout: int | None = None) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            cmd,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout,
        )
        return {
            "cmd": redact_cmd(cmd),
            "exit_code": completed.returncode,
            "timeout": False,
            "stdout_sha256": sha256_bytes(completed.stdout.encode()),
            "stderr_sha256": sha256_bytes(completed.stderr.encode()),
            "stdout_tail": completed.stdout[-4000:],
            "stderr_tail": completed.stderr[-4000:],
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout if isinstance(error.stdout, str) else ""
        stderr = error.stderr if isinstance(error.stderr, str) else ""
        return {
            "cmd": redact_cmd(cmd),
            "exit_code": 124,
            "timeout": True,
            "stdout_sha256": sha256_bytes(stdout.encode()),
            "stderr_sha256": sha256_bytes(stderr.encode()),
            "stdout_tail": stdout[-4000:],
            "stderr_tail": stderr[-4000:],
            "stdout": stdout,
            "stderr": stderr,
        }


def redact_cmd(cmd: list[str]) -> list[str]:
    return ["<redacted>" if "TOKEN" in part or "SECRET" in part else part for part in cmd]


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if not full_path.exists():
        result.update({"ok": False, "missing": markers, "checked": markers})
        return result
    text = full_path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update({"ok": not missing, "missing": missing, "checked": markers})
    return result


def parse_json_object(stdout: str) -> dict[str, Any] | None:
    start = stdout.find("{")
    end = stdout.rfind("}")
    if start < 0 or end < start:
        return None
    try:
        return json.loads(stdout[start : end + 1])
    except json.JSONDecodeError:
        return None


def redis_xrevrange(stream: str, count: int = 500) -> list[dict[str, Any]]:
    completed = subprocess.run(
        ["redis-cli", "--raw", "XREVRANGE", stream, "+", "-", "COUNT", str(count)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    entries: list[dict[str, Any]] = []
    lines = completed.stdout.splitlines()
    for index, line in enumerate(lines):
        if line != "payload" or index + 1 >= len(lines):
            continue
        entry_id = lines[index - 1] if index >= 1 else None
        try:
            payload = json.loads(lines[index + 1])
        except json.JSONDecodeError:
            continue
        entries.append({"entry_id": entry_id, "payload": payload})
    return entries


def latest_payload(stream: str) -> dict[str, Any] | None:
    entries = redis_xrevrange(stream, 20)
    return entries[0] if entries else None


def latest_final_bar(stream: str, symbol: str) -> dict[str, Any] | None:
    for entry in redis_xrevrange(stream, 1000):
        payload = entry.get("payload", {})
        bar = payload.get("payload", {}).get("Bar")
        if not bar:
            continue
        instrument = bar.get("instrument", {})
        if (
            bar.get("is_final") is True
            and bar.get("timeframe_sec") == 60
            and bar.get("source_kind") == "LiveStream"
            and instrument.get("venue_symbol") == symbol
        ):
            return entry
    return None


def nested(obj: dict[str, Any], path: list[str], default: Any = None) -> Any:
    value: Any = obj
    for key in path:
        if not isinstance(value, dict) or key not in value:
            return default
        value = value[key]
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", default="IMOEXF@RTSX")
    parser.add_argument("--timeframe", default="TIME_FRAME_M1")
    parser.add_argument("--min-final-close-ts", default="2026-07-06T06:01:00Z")
    parser.add_argument("--max-duration-seconds", default="35")
    parser.add_argument("--max-messages", default="120")
    parser.add_argument("--command-timeout-seconds", type=int, default=55)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3g-a-active-session-fresh-final-evidence.json"),
    )
    args = parser.parse_args()

    load_dotenv(ROOT / ".env")

    checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3g-a active-session fresh final evidence",
                "BARS status = DataConfirmed",
                "latest Redis Bar is final M1 LiveStream",
                "live orders",
            ],
        ),
        "readme_markers": marker_check(README, ["M4-3g-a", "active-session fresh final"]),
        "cli_markers": marker_check(
            CLI_SOURCE,
            [
                "fn finam_ws_generation_subscription_state",
                "fresh_live_final_bar_seen",
                "published_strategy_bar_count",
            ],
        ),
    }

    probe_cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "broker-cli",
        "--",
        "finam-ws-shadow-once",
        "--symbol",
        args.symbol,
        "--timeframe",
        args.timeframe,
        "--subscribe-bars",
        "--subscribe-quotes",
        "--max-duration-seconds",
        args.max_duration_seconds,
        "--max-messages",
        args.max_messages,
    ]
    probe = run(probe_cmd, timeout=args.command_timeout_seconds)
    report = parse_json_object(probe.get("stdout", ""))

    latest_health = latest_payload("finam:health")
    latest_readiness = latest_payload("finam:readiness")
    latest_bar = latest_final_bar("finam:market-data", args.symbol)

    bars_confirmed = False
    quotes_confirmed = False
    if report:
        confirmations = nested(report, ["ws_generation", "confirmations"], [])
        for confirmation in confirmations:
            if (
                confirmation.get("subscription_type") == "BARS"
                and confirmation.get("status") == "DataConfirmed"
            ):
                bars_confirmed = True
            if (
                confirmation.get("subscription_type") == "QUOTES"
                and confirmation.get("status") == "DataConfirmed"
            ):
                quotes_confirmed = True

    latest_bar_payload = latest_bar.get("payload", {}) if latest_bar else {}
    latest_bar_body = latest_bar_payload.get("payload", {}).get("Bar") if latest_bar_payload else None
    latest_health_payload = latest_health.get("payload", {}).get("payload", {}) if latest_health else {}
    latest_readiness_payload = (
        latest_readiness.get("payload", {}).get("payload", {}) if latest_readiness else {}
    )

    runtime_checks = {
        "probe_exit_ok": probe["exit_code"] == 0,
        "probe_json_parsed": report is not None,
        "bars_data_confirmed": bars_confirmed,
        "quotes_data_confirmed": quotes_confirmed,
        "fresh_live_final_bar_seen": bool(nested(report or {}, ["market_data", "fresh_live_final_bar_seen"])),
        "published_strategy_bar_count_positive": (nested(report or {}, ["market_data", "published_strategy_bar_count"], 0) or 0) > 0,
        "data_quality_bars_balanced": bool(nested(report or {}, ["data_quality", "bars", "balanced"])),
        "data_quality_no_imbalances": nested(report or {}, ["data_quality", "imbalances"], None) == [],
        "redis_latest_final_bar_present": latest_bar_body is not None,
        "redis_latest_final_bar_close_ts_ok": bool(latest_bar_body)
        and latest_bar_body.get("close_ts", "") >= args.min_final_close_ts,
        "redis_readiness_reconciliation": latest_readiness_payload.get("phase") == "Reconciliation",
        "redis_health_readonly": latest_health_payload.get("status") == "ReadOnly",
        "command_consumer_disabled": latest_health_payload.get("command_consumer_enabled") is False,
        "order_placement_disabled": latest_health_payload.get("order_placement_enabled") is False,
        "probe_live_trading_disabled": (report or {}).get("live_trading_enabled") is False,
        "probe_order_placement_disabled": (report or {}).get("order_placement_enabled") is False,
        "probe_cancel_disabled": (report or {}).get("cancel_enabled") is False,
        "probe_stop_sltp_bracket_disabled": (report or {}).get("stop_sltp_bracket_enabled") is False,
    }

    commands = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3ga_active_session_fresh_final_evidence.py"]
        ),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }

    ok = (
        all(check.get("ok") for check in checks.values())
        and all(runtime_checks.values())
        and all(command["exit_code"] == 0 for command in commands.values())
    )

    evidence = {
        "evidence_kind": "m4-3g-a-active-session-fresh-final-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "symbol": args.symbol,
        "timeframe": args.timeframe,
        "min_final_close_ts": args.min_final_close_ts,
        "broker_calls_performed": True,
        "websocket_calls_performed": True,
        "redis_calls_performed": True,
        "ssh_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "checks": checks,
        "runtime_checks": runtime_checks,
        "commands": {key: {k: v for k, v in value.items() if k not in {"stdout", "stderr"}} for key, value in commands.items()},
        "probe_command": {k: v for k, v in probe.items() if k not in {"stdout", "stderr"}},
        "probe_summary": {
            "readiness_phase": (report or {}).get("readiness_phase"),
            "readiness_reasons": (report or {}).get("readiness_reasons"),
            "ws_generation": (report or {}).get("ws_generation"),
            "data_quality": (report or {}).get("data_quality"),
            "market_data": {
                "fresh_live_final_bar_seen": nested(report or {}, ["market_data", "fresh_live_final_bar_seen"]),
                "first_fresh_live_final_bar_close_ts": nested(report or {}, ["market_data", "first_fresh_live_final_bar_close_ts"]),
                "last_fresh_live_final_bar_close_ts": nested(report or {}, ["market_data", "last_fresh_live_final_bar_close_ts"]),
                "published_strategy_bar_count": nested(report or {}, ["market_data", "published_strategy_bar_count"]),
                "stale_ws_final_bar_suppressed_count": nested(report or {}, ["market_data", "stale_ws_final_bar_suppressed_count"]),
                "latest_ws_final_bar_close_ts": nested(report or {}, ["market_data", "latest_ws_final_bar_close_ts"]),
            },
        },
        "redis_summary": {
            "latest_health": latest_health,
            "latest_readiness": latest_readiness,
            "latest_final_bar": latest_bar,
        },
        "artifacts": [artifact(DOC), artifact(CLI_SOURCE), artifact(README)],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
