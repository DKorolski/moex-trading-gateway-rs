#!/usr/bin/env python3
"""M4-3n ALOR native 10m vs diagnostic ALOR 1m-to-10m stand evidence.

The script is read-only. It reads a production ALOR native 10m Redis stream and
a separate diagnostic stand ALOR 1m Redis stream, assembles the stand M1 bars to
M10, and compares overlapping buckets. It does not write to Redis and does not
call broker order endpoints.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports" / "m4" / "m4-3n-alor-native-vs-assembled-stand-evidence.json"
M43M_PATH = ROOT / "scripts" / "m4_3m_active_session_bar_parity_evidence.py"


def load_m43m() -> Any:
    spec = importlib.util.spec_from_file_location("m4_3m_active_session_bar_parity_evidence", M43M_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load M4-3m evidence module")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


M43M = load_m43m()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


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
        "stdout_tail": completed.stdout[-2000:],
        "stderr_tail": completed.stderr[-2000:],
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


def redis_xlen(redis_cli_prefix: str, redis_url: str, stream: str) -> dict[str, Any]:
    cmd = M43M.redis_cli_base(redis_cli_prefix, json_mode=False)
    if redis_url:
        cmd.extend(["-u", redis_url])
    cmd.extend(["XLEN", stream])
    completed = subprocess.run(cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    result: dict[str, Any] = {
        "stream": stream,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_bytes(completed.stdout.encode()),
        "stderr_sha256": sha256_bytes(completed.stderr.encode()),
        "len": None,
        "error_kind": None,
    }
    if completed.returncode != 0:
        result["error_kind"] = "RedisCommandFailed"
        result["stderr_tail"] = completed.stderr[-500:]
        return result
    try:
        result["len"] = int(completed.stdout.strip() or "0")
    except ValueError:
        result["error_kind"] = "RedisXlenDecodeFailed"
    return result


def read_alor_bars(redis_cli_prefix: str, redis_url: str, stream: str, count: int) -> tuple[list[Any], dict[str, Any]]:
    exists = M43M.stream_exists(redis_url, redis_cli_prefix, stream)
    read = M43M.redis_xrevrange(redis_url, redis_cli_prefix, stream, count) if exists else None
    bars = []
    if read:
        for _, payload in read["entries"]:
            try:
                bar = M43M.parse_alor_v1_bar(payload, stream)
                if bar:
                    bars.append(bar)
            except Exception:
                continue
    return bars, {
        "name": stream,
        "exists": exists,
        "read_entry_count": None if read is None else read["entry_count"],
        "decoded_bar_count": len(bars),
        "latest_bar": M43M.summarize_latest_bar(bars),
    }


def self_test_report() -> dict[str, Any]:
    return M43M.self_test_report()


def collect_runtime(args: argparse.Namespace) -> dict[str, Any]:
    native_bars, native_summary = read_alor_bars(
        args.native_redis_cli_prefix,
        args.native_redis_url,
        args.native_stream,
        args.count,
    )
    stand_m1, stand_summary = read_alor_bars(
        args.stand_redis_cli_prefix,
        args.stand_redis_url,
        args.stand_m1_stream,
        args.count,
    )
    stand_derived, aggregation_metrics = M43M.aggregate_m1_to_m10(
        stand_m1,
        args.symbol,
        output_source_kind="AlorStandDerivedM1ToM10",
        accepted_source_kinds={"live", "history"},
    )
    native_summary["strategy_bar_provenance"] = M43M.strategy_bar_provenance(
        "AlorNativeBarsGetAndSubscribeTf600",
        source_timeframe_sec=600,
        target_timeframe_sec=600,
        aggregation_complete=True,
        gap_absence_proven=True,
    )
    stand_provenance = M43M.strategy_bar_provenance(
        "AlorStandDerivedM1ToM10",
        source_timeframe_sec=60,
        target_timeframe_sec=600,
        aggregation_complete=bool(stand_derived),
        gap_absence_proven=aggregation_metrics["gap_bucket_count"] == 0,
    )
    comparison = (
        M43M.compare_bar_sets(
            native_bars,
            stand_derived,
            args.symbol,
            left_label="alor_native",
            right_label="alor_stand_derived",
            missing_left_issue="MissingAlorNativeBar",
            missing_right_issue="MissingAlorStandDerivedBar",
            require_overlap=True,
        )
        if native_bars and stand_derived
        else None
    )
    orders_len = redis_xlen(args.stand_redis_cli_prefix, args.stand_redis_url, args.stand_command_stream)
    acks_len = redis_xlen(args.stand_redis_cli_prefix, args.stand_redis_url, args.stand_ack_stream)
    pending_reasons = []
    if not native_summary["exists"]:
        pending_reasons.append("MissingProductionAlorNative10mStream")
    if not stand_summary["exists"]:
        pending_reasons.append("MissingStandAlorM1Stream")
    if native_summary["exists"] and not native_bars:
        pending_reasons.append("NoProductionAlorNative10mBarsDecoded")
    if stand_summary["exists"] and not stand_derived:
        pending_reasons.append("NoCompleteStandAlorDerivedM10Bucket")
    if comparison and comparison["status"] == "NoOverlap":
        pending_reasons.append("NoNativeVsStandDerivedM10Overlap")
    if orders_len.get("len") not in (0, None):
        pending_reasons.append("StandCommandStreamNotEmpty")
    if acks_len.get("len") not in (0, None):
        pending_reasons.append("StandAckStreamNotEmpty")

    runtime_closed = (
        comparison is not None
        and comparison["bars_synchronized"]
        and orders_len.get("len") == 0
        and acks_len.get("len") in (0, None)
    )
    return {
        "runtime_status": "Closed" if runtime_closed else "Pending",
        "pending_reasons": pending_reasons,
        "native_stream": native_summary,
        "stand_m1_stream": {
            **stand_summary,
            "aggregation_metrics": aggregation_metrics,
            "latest_derived_m10_bar": M43M.summarize_latest_bar(stand_derived),
            "strategy_bar_provenance": stand_provenance,
        },
        "comparison": comparison,
        "stand_command_safety": {
            "command_stream": orders_len,
            "ack_stream": acks_len,
            "strategy_runtime_attached": False,
        },
    }


def generate(args: argparse.Namespace) -> dict[str, Any]:
    doc = Path("docs/m4-3n-alor-native-vs-assembled-10m-stand.md")
    script = Path("scripts/m4_3n_alor_native_vs_assembled_stand_evidence.py")
    source_checks = {
        "doc_markers": marker_check(
            doc,
            [
                "M4-3n ALOR native 10m vs ALOR assembled 1m-to-10m stand evidence",
                "trading-hybrid-1m-stand",
                "stand_command_safety.orders_len = 0",
                "no order endpoints",
            ],
        ),
        "script_markers": marker_check(
            script,
            [
                "AlorStandDerivedM1ToM10",
                "StandCommandStreamNotEmpty",
                "strategy_bar_provenance",
                "tolerance_policy",
                "compare_bar_sets",
                "live_orders_performed",
            ],
        ),
        "m4_3m_reuse_markers": marker_check(
            Path("scripts/m4_3m_active_session_bar_parity_evidence.py"),
            [
                "parse_alor_v1_bar",
                "aggregate_m1_to_m10",
                "compare_bar_sets",
                "TOLERANCE_POLICY",
                "compact_diff_summary",
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
        "evidence_kind": "m4-3n-alor-native-10m-vs-alor-stand-derived-m10-evidence-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_checks": source_checks,
        "commands": commands,
        "runtime": runtime,
        "production_source_mode": "AlorNativeBarsGetAndSubscribeTf600",
        "stand_source_mode": "AlorDiagnosticStandM1ToM10",
        "tolerance_policy": M43M.TOLERANCE_POLICY,
        "raw_redis_payload_exported": False,
        "production_redis_write_allowed": False,
        "stand_strategy_runtime_enabled": False,
        "runtime_live_attachment_allowed": False,
        "external_order_endpoint_allowed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "cutover_ready": False,
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True, default=str) + "\n")
    return report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-redis-cli-prefix", default="redis-cli")
    parser.add_argument("--stand-redis-cli-prefix", default="redis-cli")
    parser.add_argument("--native-redis-url", default="redis://127.0.0.1/")
    parser.add_argument("--stand-redis-url", default="redis://127.0.0.1/")
    parser.add_argument("--native-stream", default="md.bars.ALOR_PORTFOLIO.10m")
    parser.add_argument("--stand-m1-stream", default="md.bars.ALOR_PORTFOLIO.1m")
    parser.add_argument("--stand-command-stream", default="cmd.orders.ALOR_PORTFOLIO")
    parser.add_argument("--stand-ack-stream", default="cmd.acks.ALOR_PORTFOLIO")
    parser.add_argument("--symbol", default="IMOEXF")
    parser.add_argument("--count", type=int, default=3000)
    return parser.parse_args()


def main() -> int:
    report = generate(parse_args())
    print(json.dumps(report, indent=2, sort_keys=True, default=str))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
