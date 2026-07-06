#!/usr/bin/env python3
"""Generate M4-3h-b FINAM WS replay-wiring loop evidence."""

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
DOC = Path("docs/m4-3h-b-replay-wiring-loop-evidence.md")
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
    parser.add_argument("--max-messages", default="120")
    parser.add_argument("--max-duration-seconds", default="35")
    parser.add_argument("--max-iterations", default="2")
    parser.add_argument("--timeout-seconds", type=int, default=140)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3h-b-replay-wiring-loop-evidence.json"),
    )
    args = parser.parse_args()

    load_dotenv(ROOT / ".env")

    checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3h-b replay wiring",
                "recovery_replay.fetch_ok",
                "RecoveryNotStrategyLive",
                "order POST/DELETE",
            ],
        ),
        "readme_markers": marker_check(README, ["M4-3h-b", "replay wiring"]),
        "cli_markers": marker_check(
            CLI_SOURCE,
            [
                "run_finam_ws_recovery_replay",
                "record_finam_ws_recovery_replay_metrics",
                "finam_ws_recovery_replay_json",
                "RecoveryNotStrategyLive",
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
        args.max_iterations,
    ]
    loop_run = run(command, timeout=args.timeout_seconds)
    objects = extract_json_objects(loop_run.get("stdout", ""))
    reports = [obj for obj in objects if obj.get("finam_ws_shadow") is True]
    loop_summaries = [obj for obj in objects if obj.get("finam_ws_shadow_loop") == "stopped"]
    latest = reports[-1] if reports else {}
    replay = latest.get("recovery_replay", {})
    recovery = latest.get("recovery", {})
    data_quality = latest.get("data_quality", {})
    latest_reason_buckets = data_quality.get("bars", {}).get("reason_buckets", {})
    aggregate_reason_buckets: dict[str, int] = {}
    for report in reports:
        buckets = report.get("data_quality", {}).get("bars", {}).get("reason_buckets", {})
        for key, value in buckets.items():
            if isinstance(value, int):
                aggregate_reason_buckets[key] = aggregate_reason_buckets.get(key, 0) + value

    any_replay = any(report.get("recovery_replay", {}).get("attempted") is True for report in reports)
    any_replay_fetch_ok = any(report.get("recovery_replay", {}).get("fetch_ok") is True for report in reports)
    any_replay_bars = any((report.get("recovery_replay", {}).get("bars_count") or 0) > 0 for report in reports)
    any_gap_absence_proven = any(
        report.get("recovery_replay", {}).get("gap_absence_proven") is True for report in reports
    )
    all_replay_unpublished = all(
        report.get("recovery_replay", {}).get("published_to_redis") is False
        and report.get("recovery_replay", {}).get("published_as_strategy_live") is False
        for report in reports
    )
    all_recovery_wired = all(
        report.get("recovery", {}).get("rest_replay_wiring_enabled") is True for report in reports
    )
    all_recovery_not_strategy_live = all(
        report.get("recovery", {}).get("recovery_bars_publishable_as_strategy_live") is False
        for report in reports
    )
    all_data_quality_balanced = all(
        report.get("data_quality", {}).get("bars", {}).get("balanced") is True for report in reports
    )
    all_live_disabled = all(
        report.get("live_trading_enabled") is False
        and report.get("order_placement_enabled") is False
        and report.get("cancel_enabled") is False
        and report.get("command_consumer_enabled") is False
        for report in reports
    )
    latest_loop_summary = loop_summaries[-1] if loop_summaries else {}

    runtime_checks = {
        "loop_exit_ok": loop_run["exit_code"] == 0,
        "reports_present": len(reports) >= 1,
        "loop_summary_present": bool(loop_summaries),
        "loop_failure_count_zero": latest_loop_summary.get("failure_count") in (0, None),
        "replay_attempted": any_replay,
        "replay_fetch_ok": any_replay_fetch_ok,
        "replay_bars_count_positive": any_replay_bars,
        "replay_gap_absence_proven_in_at_least_one_iteration": any_gap_absence_proven,
        "replay_not_published_to_redis": all_replay_unpublished,
        "replay_not_published_as_strategy_live": all_replay_unpublished,
        "recovery_wiring_enabled": all_recovery_wired,
        "recovery_bars_not_strategy_live": all_recovery_not_strategy_live,
        "data_quality_balanced": all_data_quality_balanced,
        "replayed_recovery_bar_count_positive": (aggregate_reason_buckets.get("ReplayedRecoveryBar") or 0) > 0,
        "recovery_not_strategy_live_count_positive": (aggregate_reason_buckets.get("RecoveryNotStrategyLive") or 0) > 0,
        "overlap_dedup_count_positive": (aggregate_reason_buckets.get("OverlapDeduped") or 0) > 0,
        "live_orders_disabled": all_live_disabled,
        "order_placement_disabled": all_live_disabled,
        "cancel_disabled": all_live_disabled,
        "command_consumer_disabled": all_live_disabled,
    }

    commands = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3hb_replay_wiring_loop_evidence.py"]),
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
        "evidence_kind": "m4-3h-b-replay-wiring-loop-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "symbol": args.symbol,
        "timeframe": args.timeframe,
        "broker_calls_performed": True,
        "auth_post_performed": True,
        "get_bars_performed": True,
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
        "loop_command": {k: v for k, v in loop_run.items() if k not in {"stdout", "stderr"}},
        "report_count": len(reports),
        "loop_summary": latest_loop_summary,
        "aggregate_reason_buckets": aggregate_reason_buckets,
        "per_report_summary": [
            {
                "mode": report.get("mode"),
                "iteration": report.get("iteration"),
                "readiness_phase": report.get("readiness_phase"),
                "recovery_phase": report.get("recovery", {}).get("phase"),
                "recovery_blockers": report.get("recovery", {}).get("blockers"),
                "recovery_replay": report.get("recovery_replay", {}),
                "reason_buckets": report.get("data_quality", {}).get("bars", {}).get("reason_buckets", {}),
                "data_quality_balanced": report.get("data_quality", {}).get("bars", {}).get("balanced"),
                "live_trading_enabled": report.get("live_trading_enabled"),
                "order_placement_enabled": report.get("order_placement_enabled"),
                "cancel_enabled": report.get("cancel_enabled"),
                "command_consumer_enabled": report.get("command_consumer_enabled"),
            }
            for report in reports
        ],
        "latest_report_summary": {
            "mode": latest.get("mode"),
            "iteration": latest.get("iteration"),
            "readiness_phase": latest.get("readiness_phase"),
            "readiness_reasons": latest.get("readiness_reasons"),
            "recovery": recovery,
            "recovery_replay": replay,
            "data_quality": data_quality,
            "reason_buckets": latest_reason_buckets,
            "market_data": latest.get("market_data"),
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
