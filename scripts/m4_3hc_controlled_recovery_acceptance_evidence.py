#!/usr/bin/env python3
"""Generate M4-3h-c controlled FINAM WS recovery acceptance evidence."""

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
DOC = Path("docs/m4-3h-c-controlled-recovery-acceptance-evidence.md")
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


def parse_ts(value: Any) -> datetime | None:
    if not isinstance(value, str):
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", default=os.environ.get("FINAM_SYMBOL", "IMOEXF@RTSX"))
    parser.add_argument("--timeframe", default="TIME_FRAME_M1")
    parser.add_argument("--initial-final-watermark-lag-bars", default="10")
    parser.add_argument("--recovery-replay-end-lag-bars", default="5")
    parser.add_argument("--max-messages", default="220")
    parser.add_argument("--max-duration-seconds", default="95")
    parser.add_argument("--timeout-seconds", type=int, default=150)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3h-c-controlled-recovery-acceptance-evidence.json"),
    )
    args = parser.parse_args()

    load_dotenv(ROOT / ".env")

    checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3h-c",
                "controlled no-live",
                "recovery.phase = LiveReady",
                "order POST/DELETE",
            ],
        ),
        "readme_markers": marker_check(README, ["M4-3h-c", "controlled recovery acceptance"]),
        "cli_markers": marker_check(
            CLI_SOURCE,
            [
                "initial_final_watermark_lag_bars",
                "recovery_replay_end_lag_bars",
                "finam_ws_aligned_close_ts_before",
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
        "--initial-final-watermark-lag-bars",
        args.initial_final_watermark_lag_bars,
        "--recovery-replay-end-lag-bars",
        args.recovery_replay_end_lag_bars,
    ]
    loop_run = run(command, timeout=args.timeout_seconds)
    objects = extract_json_objects(loop_run.get("stdout", ""))
    reports = [obj for obj in objects if obj.get("finam_ws_shadow") is True]
    loop_summaries = [obj for obj in objects if obj.get("finam_ws_shadow_loop") == "stopped"]
    report = reports[-1] if reports else {}
    recovery = report.get("recovery", {})
    replay = report.get("recovery_replay", {})
    data_quality = report.get("data_quality", {})
    market_data = report.get("market_data", {})
    reason_buckets = data_quality.get("bars", {}).get("reason_buckets", {})
    first_live = parse_ts(recovery.get("first_live_final_bar_close_ts"))
    replay_tail = parse_ts(replay.get("last_close_ts"))
    first_live_at_or_after_replay_tail = bool(
        first_live and replay_tail and first_live >= replay_tail
    )
    first_live_after_replay_tail = bool(first_live and replay_tail and first_live > replay_tail)
    blockers = recovery.get("blockers") or []

    runtime_checks = {
        "loop_exit_ok": loop_run["exit_code"] == 0,
        "report_present": bool(reports),
        "loop_summary_present": bool(loop_summaries),
        "loop_failure_count_zero": (loop_summaries[-1].get("failure_count") if loop_summaries else None) == 0,
        "recovery_phase_live_ready": recovery.get("phase") == "LiveReady",
        "recovery_blockers_empty": blockers == [],
        "recovery_gap_absence_proven": recovery.get("gap_absence_proven") is True,
        "replay_mode_warm": replay.get("mode") == "Warm",
        "replay_fetch_ok": replay.get("fetch_ok") is True,
        "replay_gap_detected_zero": replay.get("gap_detected_count") == 0,
        "replay_gap_absence_proven": replay.get("gap_absence_proven") is True,
        "replay_bars_count_positive": (replay.get("bars_count") or 0) > 0,
        "overlap_dedup_positive": (replay.get("overlap_dedup_bar_count") or 0) > 0,
        "recovery_not_strategy_live_positive": (replay.get("recovery_not_strategy_live_bar_count") or 0) > 0,
        "first_live_final_at_or_after_replay_tail": first_live_at_or_after_replay_tail,
        "first_live_final_after_replay_tail": first_live_after_replay_tail,
        "data_quality_balanced": data_quality.get("bars", {}).get("balanced") is True,
        "replayed_recovery_bar_count_positive": (reason_buckets.get("ReplayedRecoveryBar") or 0) > 0,
        "recovery_not_strategy_live_count_positive": (reason_buckets.get("RecoveryNotStrategyLive") or 0) > 0,
        "overlap_dedup_count_positive": (reason_buckets.get("OverlapDeduped") or 0) > 0,
        "fresh_live_final_seen": market_data.get("fresh_live_final_bar_seen") is True,
        "replay_not_published_to_redis": replay.get("published_to_redis") is False,
        "replay_not_published_as_strategy_live": replay.get("published_as_strategy_live") is False,
        "recovery_bars_not_strategy_live": recovery.get("recovery_bars_publishable_as_strategy_live") is False,
        "live_orders_disabled": report.get("live_trading_enabled") is False,
        "order_placement_disabled": report.get("order_placement_enabled") is False,
        "cancel_disabled": report.get("cancel_enabled") is False,
        "command_consumer_disabled": report.get("command_consumer_enabled") is False,
    }

    commands = {
        "python_compile": run(["python3", "-m", "py_compile", str(Path("scripts/m4_3hc_controlled_recovery_acceptance_evidence.py"))]),
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
        "evidence_kind": "m4-3h-c-controlled-recovery-acceptance-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "symbol": args.symbol,
        "timeframe": args.timeframe,
        "initial_final_watermark_lag_bars": args.initial_final_watermark_lag_bars,
        "recovery_replay_end_lag_bars": args.recovery_replay_end_lag_bars,
        "broker_calls_performed": True,
        "auth_post_performed": True,
        "get_bars_performed": True,
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
        "controlled_recovery_summary": {
            "recovery": recovery,
            "recovery_replay": replay,
            "first_live_final_bar_close_ts": recovery.get("first_live_final_bar_close_ts"),
            "replay_last_close_ts": replay.get("last_close_ts"),
            "first_live_final_at_or_after_replay_tail": first_live_at_or_after_replay_tail,
            "first_live_final_after_replay_tail": first_live_after_replay_tail,
            "reason_buckets": reason_buckets,
            "data_quality": data_quality,
            "market_data": market_data,
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
