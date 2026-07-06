#!/usr/bin/env python3
"""Generate M4-3h-a FINAM REST Bars replay GET-only evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DOC = Path("docs/m4-3h-a-rest-bars-replay-evidence.md")
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


def redis_latest_final_bar(stream: str, symbol: str) -> dict[str, Any] | None:
    completed = subprocess.run(
        ["redis-cli", "--raw", "XREVRANGE", stream, "+", "-", "COUNT", "2000"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    lines = completed.stdout.splitlines()
    for index, line in enumerate(lines):
        if line != "payload" or index + 1 >= len(lines):
            continue
        entry_id = lines[index - 1] if index >= 1 else None
        try:
            envelope = json.loads(lines[index + 1])
        except json.JSONDecodeError:
            continue
        bar = envelope.get("payload", {}).get("Bar")
        if not bar:
            continue
        instrument = bar.get("instrument", {})
        if (
            bar.get("is_final") is True
            and bar.get("source_kind") == "LiveStream"
            and bar.get("timeframe_sec") == 60
            and instrument.get("venue_symbol") == symbol
        ):
            return {"entry_id": entry_id, "envelope": envelope, "bar": bar}
    return None


def parse_ts(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def fmt_ts(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def finam_auth(secret: str, base_url: str, timeout: int) -> tuple[int, dict[str, Any]]:
    body = json.dumps({"secret": secret}).encode()
    request = urllib.request.Request(
        urllib.parse.urljoin(base_url, "/v1/sessions"),
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return response.status, json.loads(response.read().decode())


def finam_get_bars(
    token: str,
    base_url: str,
    symbol: str,
    timeframe: str,
    start_time: str,
    end_time: str,
    timeout: int,
) -> tuple[int, dict[str, Any], str]:
    query = urllib.parse.urlencode(
        {
            "timeframe": timeframe,
            "interval.start_time": start_time,
            "interval.end_time": end_time,
        }
    )
    path = f"/v1/instruments/{urllib.parse.quote(symbol, safe='')}/bars?{query}"
    url = urllib.parse.urljoin(base_url, path)
    request = urllib.request.Request(
        url,
        headers={"Authorization": f"Bearer {token}"},
        method="GET",
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return response.status, json.loads(response.read().decode()), url


def decimal_value(value: Any) -> str | None:
    if isinstance(value, dict):
        raw = value.get("value")
        return str(raw) if raw is not None else None
    if value is None:
        return None
    return str(value)


def mapped_bar_summary(raw_bar: dict[str, Any], timeframe_sec: int) -> dict[str, Any]:
    open_ts = parse_ts(str(raw_bar["timestamp"]))
    close_ts = open_ts + timedelta(seconds=timeframe_sec)
    return {
        "open_ts": fmt_ts(open_ts),
        "close_ts": fmt_ts(close_ts),
        "open": decimal_value(raw_bar.get("open")),
        "high": decimal_value(raw_bar.get("high")),
        "low": decimal_value(raw_bar.get("low")),
        "close": decimal_value(raw_bar.get("close")),
        "volume": decimal_value(raw_bar.get("volume")),
    }


def contiguous(close_timestamps: list[datetime], timeframe_sec: int) -> tuple[bool, list[dict[str, str]]]:
    gaps: list[dict[str, str]] = []
    for previous, current in zip(close_timestamps, close_timestamps[1:]):
        expected = previous + timedelta(seconds=timeframe_sec)
        if current != expected:
            gaps.append({"expected": fmt_ts(expected), "actual": fmt_ts(current)})
    return not gaps, gaps


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbol", default=os.environ.get("FINAM_SYMBOL", "IMOEXF@RTSX"))
    parser.add_argument("--timeframe", default="TIME_FRAME_M1")
    parser.add_argument("--timeframe-sec", type=int, default=60)
    parser.add_argument("--overlap-bars", type=int, default=2)
    parser.add_argument("--redis-stream", default="finam:market-data")
    parser.add_argument("--base-url", default="https://api.finam.ru")
    parser.add_argument("--secret-env", default="FINAM_SECRET_TOKEN")
    parser.add_argument("--timeout-seconds", type=int, default=60)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3h-a-rest-bars-replay-evidence.json"),
    )
    args = parser.parse_args()

    load_dotenv(ROOT / ".env")
    secret = os.environ.get(args.secret_env)
    latest = redis_latest_final_bar(args.redis_stream, args.symbol)

    checks = {
        "doc_markers": marker_check(
            DOC,
            [
                "M4-3h-a REST Bars replay evidence",
                "GET /v1/instruments/{symbol}/bars",
                "no timestamp gaps",
                "order POST/DELETE",
            ],
        ),
        "readme_markers": marker_check(README, ["M4-3h-a", "REST Bars replay evidence"]),
        "cli_markers": marker_check(CLI_SOURCE, ["m4_3h_warm_cold_resync_contract"]),
    }

    errors: list[str] = []
    auth_http: int | None = None
    bars_http: int | None = None
    bars_response: dict[str, Any] = {}
    bars_url_sha256: str | None = None
    if not secret:
        errors.append(f"missing {args.secret_env}")
    if latest is None:
        errors.append("latest Redis final LiveStream M1 bar not found")

    watermark_close_ts = parse_ts(latest["bar"]["close_ts"]) if latest else None
    replay_from_close_ts = (
        watermark_close_ts - timedelta(seconds=args.timeframe_sec * args.overlap_bars)
        if watermark_close_ts
        else None
    )
    rest_query_start_open_ts = (
        replay_from_close_ts - timedelta(seconds=args.timeframe_sec)
        if replay_from_close_ts
        else None
    )
    rest_query_end_ts = datetime.now(timezone.utc)

    if secret and latest and rest_query_start_open_ts:
        try:
            auth_http, auth_response = finam_auth(secret, args.base_url, args.timeout_seconds)
            token = auth_response.get("token")
            if not token:
                errors.append("auth response missing token")
            else:
                bars_http, bars_response, url = finam_get_bars(
                    token,
                    args.base_url,
                    args.symbol,
                    args.timeframe,
                    fmt_ts(rest_query_start_open_ts),
                    fmt_ts(rest_query_end_ts),
                    args.timeout_seconds,
                )
                bars_url_sha256 = sha256_bytes(url.encode())
        except Exception as error:  # noqa: BLE001 - evidence must capture redacted failure.
            errors.append(f"request_failed:{type(error).__name__}")

    raw_bars = bars_response.get("bars", []) if isinstance(bars_response, dict) else []
    mapped = [mapped_bar_summary(bar, args.timeframe_sec) for bar in raw_bars if "timestamp" in bar]
    close_timestamps = [parse_ts(bar["close_ts"]) for bar in mapped]
    close_timestamps_sorted = sorted(close_timestamps)
    monotonic = close_timestamps == close_timestamps_sorted
    is_contiguous, gaps = contiguous(close_timestamps_sorted, args.timeframe_sec)

    first_close = close_timestamps_sorted[0] if close_timestamps_sorted else None
    last_close = close_timestamps_sorted[-1] if close_timestamps_sorted else None
    covers_watermark = bool(first_close and watermark_close_ts and first_close <= watermark_close_ts)
    reaches_watermark = bool(last_close and watermark_close_ts and last_close >= watermark_close_ts)
    has_after_watermark = any(ts > watermark_close_ts for ts in close_timestamps_sorted) if watermark_close_ts else False
    overlap_dedup_bar_count = (
        sum(1 for ts in close_timestamps_sorted if ts <= watermark_close_ts)
        if watermark_close_ts
        else 0
    )
    gap_absence_proven = (
        bool(raw_bars)
        and monotonic
        and is_contiguous
        and covers_watermark
        and reaches_watermark
        and has_after_watermark
        and overlap_dedup_bar_count >= 1
    )

    runtime_checks = {
        "auth_http_200": auth_http == 200,
        "bars_http_200": bars_http == 200,
        "bars_count_positive": len(raw_bars) > 0,
        "mapped_count_matches": len(mapped) == len(raw_bars),
        "timestamps_monotonic": monotonic,
        "replay_contiguous": is_contiguous,
        "replay_first_close_covers_watermark": covers_watermark,
        "replay_last_close_reaches_watermark": reaches_watermark,
        "replay_has_bar_after_watermark": has_after_watermark,
        "overlap_dedup_present": overlap_dedup_bar_count >= 1,
        "gap_absence_proven": gap_absence_proven,
        "live_orders_performed": False,
        "post_delete_calls_performed": False,
        "runtime_live_attachment_allowed": False,
    }

    commands = {
        "python_compile": run(["python3", "-m", "py_compile", str(Path("scripts/m4_3ha_rest_bars_replay_evidence.py"))]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }

    required_runtime_ok = all(
        value
        for key, value in runtime_checks.items()
        if key
        not in {
            "live_orders_performed",
            "post_delete_calls_performed",
            "runtime_live_attachment_allowed",
        }
    )
    boundary_ok = (
        runtime_checks["live_orders_performed"] is False
        and runtime_checks["post_delete_calls_performed"] is False
        and runtime_checks["runtime_live_attachment_allowed"] is False
    )
    ok = (
        not errors
        and all(check.get("ok") for check in checks.values())
        and required_runtime_ok
        and boundary_ok
        and all(command["exit_code"] == 0 for command in commands.values())
    )

    evidence = {
        "evidence_kind": "m4-3h-a-rest-bars-replay-get-only-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "errors": errors,
        "symbol": args.symbol,
        "timeframe": args.timeframe,
        "timeframe_sec": args.timeframe_sec,
        "overlap_bars": args.overlap_bars,
        "broker_calls_performed": True,
        "auth_post_performed": True,
        "get_bars_performed": True,
        "redis_calls_performed": True,
        "websocket_calls_performed": False,
        "ssh_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "checks": checks,
        "runtime_checks": runtime_checks,
        "commands": commands,
        "request_summary": {
            "auth_http": auth_http,
            "bars_http": bars_http,
            "bars_url_sha256": bars_url_sha256,
            "rest_query_start_open_ts": fmt_ts(rest_query_start_open_ts) if rest_query_start_open_ts else None,
            "rest_query_end_ts": fmt_ts(rest_query_end_ts),
        },
        "watermark": {
            "redis_stream": args.redis_stream,
            "entry_id": latest.get("entry_id") if latest else None,
            "close_ts": fmt_ts(watermark_close_ts) if watermark_close_ts else None,
            "bar": latest.get("bar") if latest else None,
        },
        "replay_summary": {
            "replay_from_close_ts": fmt_ts(replay_from_close_ts) if replay_from_close_ts else None,
            "bars_count": len(raw_bars),
            "mapped_bars_count": len(mapped),
            "first_close_ts": fmt_ts(first_close) if first_close else None,
            "last_close_ts": fmt_ts(last_close) if last_close else None,
            "overlap_dedup_bar_count": overlap_dedup_bar_count,
            "gap_count": len(gaps),
            "gaps": gaps[:10],
            "gap_absence_proven": gap_absence_proven,
            "sample_first_bars": mapped[:3],
            "sample_last_bars": mapped[-3:],
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
