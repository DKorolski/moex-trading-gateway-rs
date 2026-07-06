#!/usr/bin/env python3
"""Generate M4-3i FINAM WS session-aware bar silence watchdog evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = Path("docs/m4-3i-session-aware-bar-silence-watchdog.md")
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
            "cmd": cmd,
            "exit_code": completed.returncode,
            "timeout": False,
            "stdout_sha256": sha256_bytes(completed.stdout.encode()),
            "stderr_sha256": sha256_bytes(completed.stderr.encode()),
            "stdout_tail": completed.stdout[-5000:],
            "stderr_tail": completed.stderr[-3000:],
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout if isinstance(error.stdout, str) else ""
        stderr = error.stderr if isinstance(error.stderr, str) else ""
        return {
            "cmd": cmd,
            "exit_code": 124,
            "timeout": True,
            "stdout_sha256": sha256_bytes(stdout.encode()),
            "stderr_sha256": sha256_bytes(stderr.encode()),
            "stdout_tail": stdout[-5000:],
            "stderr_tail": stderr[-3000:],
            "stdout": stdout,
            "stderr": stderr,
        }


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


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def extract_json_objects(text: str) -> list[dict[str, Any]]:
    decoder = json.JSONDecoder()
    objects: list[dict[str, Any]] = []
    index = 0
    while index < len(text):
        start = text.find("{", index)
        if start < 0:
            break
        try:
            value, end = decoder.raw_decode(text[start:])
        except json.JSONDecodeError:
            index = start + 1
            continue
        if isinstance(value, dict):
            objects.append(value)
        index = start + end
    return objects


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", default=os.environ.get("FINAM_SYMBOL", "IMOEXF@RTSX"))
    parser.add_argument("--timeframe", default="TIME_FRAME_M1")
    parser.add_argument("--max-messages", default="140")
    parser.add_argument("--max-duration-seconds", default="55")
    parser.add_argument("--timeout-seconds", type=int, default=120)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3i-session-aware-bar-silence-watchdog-evidence.json"),
    )
    args = parser.parse_args()

    load_dotenv(ROOT / ".env")

    checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3i",
                "session-aware watchdog",
                "silence_alert",
                "order POST/DELETE",
            ],
        ),
        "readme_markers": marker_check(README, ["M4-3i", "session-aware bar silence watchdog"]),
        "cli_markers": marker_check(
            CLI_SOURCE,
            [
                "FinamWsSessionSilenceWatchdogReport",
                "finam_ws_session_silence_watchdog",
                "session_closed_no_silence_alert",
                "schedule_unknown_blocks_readiness",
                "schedule_fetch_failed_blocks_readiness",
                "MarketDataSessionUnknown",
                "m4_3i_session_aware_bar_silence_watchdog",
                "finam_ws_session_silence_watchdog_alerts_only_inside_open_session",
                "finam_ws_shadow_readiness_blocks_unknown_or_failed_schedule",
                "finam_ws_session_silence_watchdog_unknown_schedule_blocks_readiness",
            ],
        ),
    }

    command = [
        "cargo",
        "run",
        "-q",
        "-p",
        "broker-cli",
        "--",
        "finam-ws-shadow-loop",
        "--symbol",
        args.symbol,
        "--timeframe",
        args.timeframe,
        "--subscribe-bars",
        "--subscribe-quotes",
        "--max-messages",
        args.max_messages,
        "--max-duration-seconds",
        args.max_duration_seconds,
        "--reconnect-delay-seconds",
        "1",
        "--max-iterations",
        "1",
    ]
    loop_run = run(command, timeout=args.timeout_seconds)
    objects = extract_json_objects(loop_run.get("stdout", ""))
    reports = [obj for obj in objects if obj.get("finam_ws_shadow") is True]
    loop_summaries = [obj for obj in objects if obj.get("finam_ws_shadow_loop") == "stopped"]
    report = reports[-1] if reports else {}
    watchdog = report.get("session_silence_watchdog", {})
    market_data = report.get("market_data", {})
    session_state = watchdog.get("session_state")
    open_session_ok = (
        session_state == "Open"
        and watchdog.get("schedule_fetch_ok") is True
        and watchdog.get("silence_alert") is False
        and market_data.get("fresh_live_final_bar_seen") is True
    )
    closed_session_ok = (
        session_state in {"Closed", "Break", "Maintenance"}
        and watchdog.get("schedule_fetch_ok") is True
        and watchdog.get("silence_alert") is False
        and watchdog.get("session_closed_no_silence_alert") is True
    )
    unknown_session_ok = (
        session_state == "Unknown"
        and watchdog.get("silence_alert") is False
        and watchdog.get("alert_reason") in {"ScheduleFetchFailed", "SessionUnknown"}
    )

    runtime_checks = {
        "loop_exit_ok": loop_run["exit_code"] == 0,
        "report_present": bool(reports),
        "loop_summary_present": bool(loop_summaries),
        "watchdog_present": bool(watchdog),
        "watchdog_schema": watchdog.get("schema") == "m4_3i_session_aware_bar_silence_watchdog",
        "watchdog_enabled": watchdog.get("enabled") is True,
        "unknown_schedule_blocker_fields_present": "schedule_unknown_blocks_readiness" in watchdog
        and "schedule_fetch_failed_blocks_readiness" in watchdog
        and "readiness_blocked" in watchdog
        and "readiness_block_reason" in watchdog,
        "session_state_known_or_safe_unknown": open_session_ok or closed_session_ok or unknown_session_ok,
        "open_session_no_silence_if_fresh": open_session_ok or session_state != "Open",
        "closed_session_no_false_alert": closed_session_ok or session_state not in {"Closed", "Break", "Maintenance"},
        "live_orders_disabled": report.get("live_trading_enabled") is False,
        "order_placement_disabled": report.get("order_placement_enabled") is False,
        "cancel_disabled": report.get("cancel_enabled") is False,
        "command_consumer_disabled": report.get("command_consumer_enabled") is False,
    }

    commands = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3i_session_aware_bar_silence_watchdog_evidence.py"]),
        "broker_cli_watchdog_tests": run(["cargo", "test", "-p", "broker-cli", "finam_ws_session_silence_watchdog", "--", "--nocapture"]),
        "broker_cli_readiness_blocker_tests": run(["cargo", "test", "-p", "broker-cli", "finam_ws_shadow_readiness_blocks_unknown_or_failed_schedule", "--", "--nocapture"]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }

    ok = (
        all(check.get("ok") for check in checks.values())
        and all(runtime_checks.values())
        and all(command_result["exit_code"] == 0 for command_result in commands.values())
    )

    evidence = {
        "evidence_kind": "m4-3i-session-aware-bar-silence-watchdog-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "symbol": args.symbol,
        "timeframe": args.timeframe,
        "broker_calls_performed": True,
        "auth_post_performed": True,
        "schedule_get_performed": True,
        "websocket_calls_performed": True,
        "redis_calls_performed": True,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "checks": checks,
        "runtime_checks": runtime_checks,
        "commands": {key: {k: v for k, v in value.items() if k not in {"stdout", "stderr"}} for key, value in commands.items()},
        "loop_command": {k: v for k, v in loop_run.items() if k not in {"stdout", "stderr"}},
        "report_count": len(reports),
        "loop_summary": loop_summaries[-1] if loop_summaries else {},
        "watchdog_summary": watchdog,
        "market_data_summary": market_data,
        "artifacts": [artifact(DOC), artifact(CLI_SOURCE), artifact(README)],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
