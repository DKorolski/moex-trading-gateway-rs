#!/usr/bin/env python3
"""M4-3m active-session ALOR native 10m vs FINAM derived 10m evidence.

Default mode performs a bounded Redis read if Redis is available and emits a
reviewable report. It does not call broker APIs, WebSocket endpoints, SSH, or
order endpoints, and it does not write to Redis.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from dataclasses import dataclass
from datetime import datetime, timezone
from decimal import Decimal
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports" / "m4" / "m4-3m-active-session-bar-parity-evidence.json"


@dataclass(frozen=True)
class Bar:
    symbol: str
    timeframe_sec: int
    open_ts: int
    close_ts: int
    open: Decimal
    high: Decimal
    low: Decimal
    close: Decimal
    volume: Decimal
    is_final: bool
    source_kind: str


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


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
        "stdout_tail": completed.stdout[-3000:],
        "stderr_tail": completed.stderr[-3000:],
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


def iso_to_epoch(value: str) -> int:
    normalized = value.replace("Z", "+00:00")
    return int(datetime.fromisoformat(normalized).timestamp())


def bucket_open_ts(open_ts: int, target_timeframe_sec: int) -> int:
    return open_ts - (open_ts % target_timeframe_sec)


def parse_redis_json_entries(stdout: str) -> list[tuple[str, str]]:
    if not stdout.strip():
        return []
    raw = json.loads(stdout)
    entries: list[tuple[str, str]] = []
    for entry_id, fields in raw:
        field_map = dict(zip(fields[0::2], fields[1::2]))
        payload = field_map.get("payload")
        if isinstance(payload, str):
            entries.append((entry_id, payload))
    return entries


def redis_xrevrange(redis_url: str, stream: str, count: int) -> dict[str, Any]:
    cmd = ["redis-cli", "--json"]
    if redis_url:
        cmd.extend(["-u", redis_url])
    cmd.extend(["XREVRANGE", stream, "+", "-", "COUNT", str(count)])
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    result: dict[str, Any] = {
        "stream": stream,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_bytes(completed.stdout.encode()),
        "stderr_sha256": sha256_bytes(completed.stderr.encode()),
        "entry_count": 0,
        "entries": [],
        "error_kind": None,
    }
    if completed.returncode != 0:
        result["error_kind"] = "RedisCommandFailed"
        result["stderr_tail"] = completed.stderr[-1000:]
        return result
    try:
        entries = parse_redis_json_entries(completed.stdout)
    except Exception as exc:  # noqa: BLE001 - evidence should classify decode failures
        result["error_kind"] = f"RedisJsonDecodeFailed:{type(exc).__name__}"
        return result
    result["entry_count"] = len(entries)
    result["entries"] = entries
    return result


def stream_exists(redis_url: str, stream: str) -> bool:
    cmd = ["redis-cli", "--raw"]
    if redis_url:
        cmd.extend(["-u", redis_url])
    cmd.extend(["EXISTS", stream])
    completed = subprocess.run(cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return completed.returncode == 0 and completed.stdout.strip() == "1"


def parse_alor_v1_bar(payload_text: str, stream: str) -> Bar | None:
    envelope = json.loads(payload_text)
    if envelope.get("msg_type") != "bar":
        return None
    payload = envelope.get("payload") or {}
    symbol = str(payload.get("symbol") or "")
    if not symbol:
        return None
    timeframe_sec = 600 if stream.endswith(".10m") else 60
    close_ts = int(payload["close_time_utc"])
    return Bar(
        symbol=symbol,
        timeframe_sec=timeframe_sec,
        open_ts=close_ts - timeframe_sec,
        close_ts=close_ts,
        open=Decimal(str(payload["o"])),
        high=Decimal(str(payload["h"])),
        low=Decimal(str(payload["l"])),
        close=Decimal(str(payload["c"])),
        volume=Decimal(str(payload["v"])),
        is_final=True,
        source_kind=str(payload.get("origin") or "unknown"),
    )


def parse_finam_v2_market_bar(payload_text: str) -> Bar | None:
    envelope = json.loads(payload_text)
    if envelope.get("msg_type") != "MarketData":
        return None
    payload = envelope.get("payload") or {}
    bar_payload = payload.get("Bar")
    if not isinstance(bar_payload, dict):
        return None
    instrument = bar_payload.get("instrument") or {}
    return Bar(
        symbol=str(instrument.get("symbol") or ""),
        timeframe_sec=int(bar_payload["timeframe_sec"]),
        open_ts=iso_to_epoch(str(bar_payload["open_ts"])),
        close_ts=iso_to_epoch(str(bar_payload["close_ts"])),
        open=Decimal(str(bar_payload["open"])),
        high=Decimal(str(bar_payload["high"])),
        low=Decimal(str(bar_payload["low"])),
        close=Decimal(str(bar_payload["close"])),
        volume=Decimal(str(bar_payload["volume"])),
        is_final=bool(bar_payload["is_final"]),
        source_kind=str(bar_payload.get("source_kind") or "unknown"),
    )


def aggregate_m1_to_m10(bars: list[Bar], symbol: str) -> tuple[list[Bar], dict[str, int]]:
    final_m1 = sorted(
        [
            bar
            for bar in bars
            if bar.symbol == symbol
            and bar.timeframe_sec == 60
            and bar.is_final
            and bar.source_kind == "LiveStream"
        ],
        key=lambda bar: bar.open_ts,
    )
    metrics = {
        "source_final_m1_count": len(final_m1),
        "derived_m10_count": 0,
        "dropped_incomplete_bucket_count": 0,
        "gap_bucket_count": 0,
    }
    buckets: dict[int, list[Bar]] = {}
    for bar in final_m1:
        buckets.setdefault(bucket_open_ts(bar.open_ts, 600), []).append(bar)

    derived: list[Bar] = []
    for open_ts, bucket in sorted(buckets.items()):
        bucket = sorted(bucket, key=lambda bar: bar.open_ts)
        expected_opens = list(range(open_ts, open_ts + 600, 60))
        actual_opens = [bar.open_ts for bar in bucket]
        if len(bucket) != 10:
            metrics["dropped_incomplete_bucket_count"] += 1
            continue
        if actual_opens != expected_opens:
            metrics["gap_bucket_count"] += 1
            continue
        derived.append(
            Bar(
                symbol=symbol,
                timeframe_sec=600,
                open_ts=open_ts,
                close_ts=open_ts + 600,
                open=bucket[0].open,
                high=max(bar.high for bar in bucket),
                low=min(bar.low for bar in bucket),
                close=bucket[-1].close,
                volume=sum((bar.volume for bar in bucket), Decimal("0")),
                is_final=True,
                source_kind="FinamDerivedM1ToM10",
            )
        )
    metrics["derived_m10_count"] = len(derived)
    return derived, metrics


def compare_bar_pair(alor: Bar, finam: Bar) -> list[str]:
    issues: list[str] = []
    if alor.symbol != finam.symbol:
        issues.append("InstrumentMismatch")
    if alor.timeframe_sec != finam.timeframe_sec:
        issues.append("TimeframeMismatch")
    if not alor.is_final or not finam.is_final:
        issues.append("FinalityMismatch")
    if alor.open_ts != finam.open_ts or alor.close_ts != finam.close_ts:
        issues.append("TimestampMismatch")
    if (
        alor.open != finam.open
        or alor.high != finam.high
        or alor.low != finam.low
        or alor.close != finam.close
        or alor.volume != finam.volume
    ):
        issues.append("OhlcvMismatch")
    return issues


def summarize_bar(bar: Bar) -> dict[str, Any]:
    return {
        "symbol": bar.symbol,
        "timeframe_sec": bar.timeframe_sec,
        "open_ts": datetime.fromtimestamp(bar.open_ts, tz=timezone.utc).isoformat(),
        "close_ts": datetime.fromtimestamp(bar.close_ts, tz=timezone.utc).isoformat(),
        "source_kind": bar.source_kind,
        "is_final": bar.is_final,
    }


def compare_alor_finam(alor_bars: list[Bar], finam_derived: list[Bar], symbol: str) -> dict[str, Any]:
    alor_by_open = {bar.open_ts: bar for bar in alor_bars if bar.symbol == symbol and bar.timeframe_sec == 600}
    finam_by_open = {bar.open_ts: bar for bar in finam_derived}
    all_opens = sorted(set(alor_by_open) | set(finam_by_open))
    comparisons = []
    blocking_issue_count = 0
    for open_ts in all_opens:
        alor = alor_by_open.get(open_ts)
        finam = finam_by_open.get(open_ts)
        if alor is None:
            issues = ["MissingAlorBar"]
            blocking_issue_count += 1
            comparisons.append({"open_ts": open_ts, "issues": issues, "finam": summarize_bar(finam)})
            continue
        if finam is None:
            issues = ["MissingFinamDerivedBar"]
            blocking_issue_count += 1
            comparisons.append({"open_ts": open_ts, "issues": issues, "alor": summarize_bar(alor)})
            continue
        issues = compare_bar_pair(alor, finam)
        blocking_issue_count += len([issue for issue in issues if issue != "SourceKindDiagnostic"])
        comparisons.append(
            {
                "open_ts": open_ts,
                "issues": issues,
                "alor": summarize_bar(alor),
                "finam": summarize_bar(finam),
            }
        )
    return {
        "matched_bucket_count": sum(1 for item in comparisons if "alor" in item and "finam" in item),
        "comparison_count": len(comparisons),
        "blocking_issue_count": blocking_issue_count,
        "bars_synchronized": bool(comparisons) and blocking_issue_count == 0,
        "comparisons": comparisons[-20:],
    }


def self_test_report() -> dict[str, Any]:
    base = 1783317600  # 2026-07-06T09:00:00Z
    m1 = [
        Bar("IMOEXF", 60, base + i * 60, base + (i + 1) * 60, Decimal("100") + i, Decimal("102") + i, Decimal("99") - i, Decimal("101") + i, Decimal("10") + i, True, "LiveStream")
        for i in range(10)
    ]
    derived, metrics = aggregate_m1_to_m10(m1, "IMOEXF")
    alor = [
        Bar(
            "IMOEXF",
            600,
            base,
            base + 600,
            derived[0].open,
            derived[0].high,
            derived[0].low,
            derived[0].close,
            derived[0].volume,
            True,
            "live",
        )
    ]
    comparison = compare_alor_finam(alor, derived, "IMOEXF")
    return {
        "ok": len(derived) == 1 and metrics["derived_m10_count"] == 1 and comparison["bars_synchronized"],
        "metrics": metrics,
        "comparison": comparison,
    }


def collect_runtime(args: argparse.Namespace) -> dict[str, Any]:
    alor_exists = stream_exists(args.redis_url, args.alor_stream)
    finam_exists = stream_exists(args.redis_url, args.finam_stream)
    alor_read = redis_xrevrange(args.redis_url, args.alor_stream, args.count) if alor_exists else None
    finam_read = redis_xrevrange(args.redis_url, args.finam_stream, args.count) if finam_exists else None

    alor_bars: list[Bar] = []
    if alor_read:
        for _, payload in alor_read["entries"]:
            try:
                bar = parse_alor_v1_bar(payload, args.alor_stream)
                if bar:
                    alor_bars.append(bar)
            except Exception:
                continue

    finam_m1: list[Bar] = []
    if finam_read:
        for _, payload in finam_read["entries"]:
            try:
                bar = parse_finam_v2_market_bar(payload)
                if bar:
                    finam_m1.append(bar)
            except Exception:
                continue

    finam_derived, aggregation_metrics = aggregate_m1_to_m10(finam_m1, args.symbol)
    comparison = compare_alor_finam(alor_bars, finam_derived, args.symbol) if alor_bars else None
    pending_reasons = []
    if not alor_exists:
        pending_reasons.append("MissingAlorOracleStream")
    if not finam_exists:
        pending_reasons.append("MissingFinamShadowStream")
    if finam_exists and not finam_derived:
        pending_reasons.append("NoCompleteFinamDerivedM10Bucket")
    if alor_exists and not alor_bars:
        pending_reasons.append("NoAlorNativeM10BarsDecoded")

    runtime_status = "Closed" if comparison and comparison["bars_synchronized"] else "Pending"
    return {
        "runtime_status": runtime_status,
        "pending_reasons": pending_reasons,
        "redis_available": alor_exists or finam_exists,
        "alor_stream": {
            "name": args.alor_stream,
            "exists": alor_exists,
            "read_entry_count": None if alor_read is None else alor_read["entry_count"],
            "decoded_bar_count": len(alor_bars),
            "latest_bar": summarize_bar(alor_bars[-1]) if alor_bars else None,
        },
        "finam_stream": {
            "name": args.finam_stream,
            "exists": finam_exists,
            "read_entry_count": None if finam_read is None else finam_read["entry_count"],
            "decoded_m1_bar_count": len(finam_m1),
            "aggregation_metrics": aggregation_metrics,
            "latest_derived_m10_bar": summarize_bar(finam_derived[-1]) if finam_derived else None,
        },
        "comparison": comparison,
    }


def generate(args: argparse.Namespace) -> dict[str, Any]:
    doc = Path("docs/m4-3m-active-session-alor-finam-10m-parity.md")
    script = Path("scripts/m4_3m_active_session_bar_parity_evidence.py")
    parity_source = Path("crates/broker-core/src/parity.rs")
    source_checks = {
        "doc_markers": marker_check(
            doc,
            [
                "M4-3m active-session ALOR native 10m vs FINAM derived 10m parity",
                "MissingAlorOracleStream",
                "FINAM M1 aggregation is final-only",
                "no order endpoints",
            ],
        ),
        "script_markers": marker_check(
            script,
            [
                "parse_alor_v1_bar",
                "parse_finam_v2_market_bar",
                "aggregate_m1_to_m10",
                "compare_alor_finam",
                "MissingAlorOracleStream",
                "post_delete_calls_performed",
            ],
        ),
        "broker_core_parity_markers": marker_check(
            parity_source,
            [
                "compare_final_bars_for_instrument",
                "BrokerBarParityReport",
                "BarOhlcvMismatch",
            ],
        ),
    }
    commands = {
        "python_compile": run(["python3", "-m", "py_compile", str(script)]),
        "self_test": self_test_report(),
    }
    runtime = collect_runtime(args)
    ok = all(check.get("ok") for check in source_checks.values()) and commands["python_compile"]["exit_code"] == 0 and commands["self_test"]["ok"]
    report = {
        "evidence_kind": "m4-3m-active-session-alor-native-10m-vs-finam-derived-m10-evidence-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "commands": commands,
        "runtime": runtime,
        "alor_oracle_source_mode": "AlorNativeBarsGetAndSubscribeTf600",
        "finam_source_mode": "FinamDerivedM1ToM10",
        "provenance_required": True,
        "raw_redis_payload_exported": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "external_order_endpoint_allowed": False,
        "real_finam_order_endpoint_used": False,
        "command_consumer_to_real_finam_enabled": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "cutover_ready": False,
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True, default=str) + "\n")
    return report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--redis-url", default="redis://127.0.0.1/", help="Redis URL for read-only stream reads")
    parser.add_argument(
        "--alor-stream",
        default="md.bars.ALOR_PORTFOLIO.10m",
        help="ALOR native 10m oracle stream",
    )
    parser.add_argument("--finam-stream", default="finam_ws_shadow:market_data", help="FINAM WS shadow market-data stream")
    parser.add_argument("--symbol", default="IMOEXF", help="Target symbol")
    parser.add_argument("--count", type=int, default=500, help="Bounded XREVRANGE count per stream")
    return parser.parse_args()


def main() -> int:
    report = generate(parse_args())
    print(json.dumps(report, indent=2, sort_keys=True, default=str))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
