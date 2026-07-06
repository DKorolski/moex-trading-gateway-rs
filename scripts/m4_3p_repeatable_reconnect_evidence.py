#!/usr/bin/env python3
"""Generate M4-3p repeatable FINAM WS reconnect evidence.

The script reads local stdout logs produced by `broker-cli finam-ws-shadow-loop`
and emits a redacted evidence JSON. It does not read `.env`, does not call FINAM,
does not call Redis, and does not include raw market-data payloads or raw logs in
the output.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = Path("reports/m4/m4-3p-repeatable-reconnect-evidence.json")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return sha256_bytes(text.encode())


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


def artifact(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {"present": False}
    full_path = path if path.is_absolute() else ROOT / path
    result: dict[str, Any] = {"path": str(path), "present": full_path.exists()}
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


def load_log(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {"present": False, "objects": []}
    full_path = path if path.is_absolute() else ROOT / path
    if not full_path.exists():
        return {"present": False, "path": str(path), "objects": []}
    text = full_path.read_text(errors="replace")
    return {
        "present": True,
        "path": str(path),
        "sha256": sha256_text(text),
        "bytes": len(text.encode()),
        "objects": extract_json_objects(text),
    }


def list_contains(value: Any, expected: str) -> bool:
    return isinstance(value, list) and expected in value


def safe_get(obj: dict[str, Any], path: list[str], default: Any = None) -> Any:
    current: Any = obj
    for item in path:
        if not isinstance(current, dict) or item not in current:
            return default
        current = current[item]
    return current


def report_boundary(report: dict[str, Any]) -> dict[str, Any]:
    return {
        "live_trading_enabled": report.get("live_trading_enabled"),
        "command_consumer_enabled": report.get("command_consumer_enabled"),
        "order_placement_enabled": report.get("order_placement_enabled"),
        "cancel_enabled": report.get("cancel_enabled"),
        "stop_sltp_bracket_enabled": report.get("stop_sltp_bracket_enabled"),
        "rest_bars_used_for_strategy": report.get("rest_bars_used_for_strategy"),
        "rest_market_data_used_for_strategy": report.get("rest_market_data_used_for_strategy"),
    }


def summarize_reconnect(log: dict[str, Any]) -> dict[str, Any]:
    objects = log["objects"]
    failures = [obj for obj in objects if obj.get("finam_ws_shadow") is False]
    reports = [obj for obj in objects if obj.get("finam_ws_shadow") is True]
    recovery_reports = [
        report
        for report in reports
        if report.get("stop_reason") not in {"ctrl_c", "sigterm"}
    ]
    report = recovery_reports[-1] if recovery_reports else (reports[-1] if reports else {})
    recovery = report.get("recovery", {}) if isinstance(report.get("recovery"), dict) else {}
    replay = report.get("recovery_replay", {}) if isinstance(report.get("recovery_replay"), dict) else {}
    market_data = report.get("market_data", {}) if isinstance(report.get("market_data"), dict) else {}
    lifecycle = (
        report.get("market_data_lifecycle", {})
        if isinstance(report.get("market_data_lifecycle"), dict)
        else {}
    )
    ws_generation = report.get("ws_generation", {}) if isinstance(report.get("ws_generation"), dict) else {}
    confirmations = ws_generation.get("confirmations") if isinstance(ws_generation.get("confirmations"), list) else []
    bars_confirmation = next(
        (
            item
            for item in confirmations
            if isinstance(item, dict) and item.get("subscription_type") == "BARS"
        ),
        {},
    )
    degraded_failures = [
        obj
        for obj in failures
        if obj.get("readiness_phase") == "Degraded"
        and list_contains(obj.get("readiness_reasons"), "MarketDataNotLive")
    ]
    blockers = recovery.get("blockers") if isinstance(recovery.get("blockers"), list) else None
    boundary = report_boundary(report) if report else {}
    no_live_boundary_closed = bool(
        report
        and boundary.get("live_trading_enabled") is False
        and boundary.get("command_consumer_enabled") is False
        and boundary.get("order_placement_enabled") is False
        and boundary.get("cancel_enabled") is False
        and boundary.get("stop_sltp_bracket_enabled") is False
    )

    return {
        "log_present": log["present"],
        "log_artifact": {key: log.get(key) for key in ["path", "sha256", "bytes"] if key in log},
        "failure_iteration_count": len(failures),
        "degraded_market_data_not_live_count": len(degraded_failures),
        "success_report_count": len(reports),
        "recovery_success_report_count": len(recovery_reports),
        "ignored_shutdown_success_report_count": len(reports) - len(recovery_reports),
        "latest_success": {
            "present": bool(report),
            "iteration": report.get("iteration"),
            "stop_reason": report.get("stop_reason"),
            "readiness_phase": report.get("readiness_phase"),
            "readiness_reasons": report.get("readiness_reasons"),
            "market_data_lifecycle_phase": lifecycle.get("phase"),
            "strategy_market_data_source": report.get("strategy_market_data_source"),
            "ws_generation_id": ws_generation.get("ws_generation_id"),
            "bars_subscription_status": bars_confirmation.get("status"),
            "bars_subscription_confirmation_source": bars_confirmation.get("confirmation_source"),
            "recovery_phase": recovery.get("phase"),
            "recovery_blockers": blockers,
            "recovery_gap_absence_proven": recovery.get("gap_absence_proven"),
            "recovery_first_live_final_bar_close_ts": recovery.get("first_live_final_bar_close_ts"),
            "recovery_replay_last_close_ts": replay.get("last_close_ts"),
            "recovery_replay_fetch_ok": replay.get("fetch_ok"),
            "recovery_replay_gap_absence_proven": replay.get("gap_absence_proven"),
            "recovery_replay_gap_detected_count": replay.get("gap_detected_count"),
            "first_fresh_live_final_bar_close_ts": market_data.get("first_fresh_live_final_bar_close_ts"),
            "last_fresh_live_final_bar_close_ts": market_data.get("last_fresh_live_final_bar_close_ts"),
            "final_bar_gap_detected_count": market_data.get("final_bar_gap_detected_count"),
            "boundary": boundary,
        },
        "checks": {
            "readiness_degraded_during_break": len(degraded_failures) > 0,
            "success_report_present": bool(report),
            "readiness_returned_safe_no_live": report.get("readiness_phase") == "Reconciliation"
            and list_contains(report.get("readiness_reasons"), "OperatorLiveArmMissing"),
            "market_data_lifecycle_live_ready": lifecycle.get("phase") == "LiveReady",
            "bars_subscription_data_confirmed": bars_confirmation.get("status") == "DataConfirmed",
            "recovery_live_ready": recovery.get("phase") == "LiveReady",
            "recovery_blockers_empty": blockers == [],
            "recovery_gap_absence_proven": recovery.get("gap_absence_proven") is True,
            "replay_gap_absence_proven": replay.get("gap_absence_proven") is True,
            "replay_gap_detected_zero": replay.get("gap_detected_count") == 0,
            "final_bar_gap_detected_zero": market_data.get("final_bar_gap_detected_count") == 0,
            "no_live_boundary_closed": no_live_boundary_closed,
        },
    }


def summarize_shutdown(log: dict[str, Any], expected_reason: str) -> dict[str, Any]:
    objects = log["objects"]
    reports = [obj for obj in objects if obj.get("finam_ws_shadow") is True]
    summaries = [
        obj
        for obj in objects
        if obj.get("finam_ws_shadow_loop") == "stopped"
        and obj.get("stop_reason") == expected_reason
    ]
    report = reports[-1] if reports else {}
    summary = summaries[-1] if summaries else {}
    return {
        "expected_stop_reason": expected_reason,
        "log_present": log["present"],
        "log_artifact": {key: log.get(key) for key in ["path", "sha256", "bytes"] if key in log},
        "ws_report_count": len(reports),
        "stopped_summary_count": len(summaries),
        "latest_ws_report": {
            "present": bool(report),
            "stop_reason": report.get("stop_reason"),
            "readiness_phase": report.get("readiness_phase"),
            "readiness_reasons": report.get("readiness_reasons"),
            "boundary": report_boundary(report) if report else {},
        },
        "latest_stopped_summary": {
            "present": bool(summary),
            "stop_reason": summary.get("stop_reason"),
            "live_trading_enabled": summary.get("live_trading_enabled"),
            "iterations": summary.get("iterations"),
            "success_count": summary.get("success_count"),
            "failure_count": summary.get("failure_count"),
        },
        "checks": {
            "stopped_summary_present": bool(summary),
            "ws_iteration_stop_reason_matches": report.get("stop_reason") == expected_reason,
            "summary_stop_reason_matches": summary.get("stop_reason") == expected_reason,
            "summary_live_trading_disabled": summary.get("live_trading_enabled") is False,
        },
    }


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if not full_path.exists():
        result.update({"ok": False, "missing": markers})
        return result
    text = full_path.read_text()
    missing = [marker for marker in markers if marker not in text]
    result.update({"ok": not missing, "missing": missing, "checked": markers})
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reconnect-log", type=Path, required=True)
    parser.add_argument("--sigterm-log", type=Path, required=True)
    parser.add_argument("--ctrlc-log", type=Path, required=True)
    parser.add_argument("--break-window-msk", required=True)
    parser.add_argument("--break-window-utc", required=True)
    parser.add_argument("--readiness-dump", type=Path)
    parser.add_argument("--health-dump", type=Path)
    parser.add_argument("--market-data-dump", type=Path)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    args = parser.parse_args()

    head = git_head()
    reconnect_log = load_log(args.reconnect_log)
    sigterm_log = load_log(args.sigterm_log)
    ctrlc_log = load_log(args.ctrlc_log)

    reconnect = summarize_reconnect(reconnect_log)
    sigterm = summarize_shutdown(sigterm_log, "sigterm")
    ctrlc = summarize_shutdown(ctrlc_log, "ctrl_c")
    all_checks: dict[str, bool] = {}
    for prefix, section in [
        ("reconnect", reconnect),
        ("sigterm", sigterm),
        ("ctrlc", ctrlc),
    ]:
        checks = section.get("checks", {})
        if isinstance(checks, dict):
            for key, value in checks.items():
                all_checks[f"{prefix}_{key}"] = value is True

    evidence = {
        "schema": "m4_3p_repeatable_reconnect_evidence",
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "break_window_msk": args.break_window_msk,
        "break_window_utc": args.break_window_utc,
        "raw_logs_included": False,
        "raw_broker_payloads_included": False,
        "secrets_or_jwts_included": False,
        "redis_dumps": {
            "readiness": artifact(args.readiness_dump),
            "health": artifact(args.health_dump),
            "market_data": artifact(args.market_data_dump),
        },
        "source_markers": {
            "runbook": marker_check(
                Path("docs/m4-3p-repeatable-finam-ws-reconnect-evidence.md"),
                ["M4-3p", "SIGTERM", "Ctrl-C", "recovery.phase = LiveReady"],
            ),
            "cli": marker_check(
                Path("crates/broker-cli/src/main.rs"),
                [
                    "shutdown_signal",
                    "SignalKind::terminate",
                    "finam_ws_recovery_confirming_live_final_close_ts",
                    "is_shutdown_stop_reason",
                ],
            ),
        },
        "reconnect": reconnect,
        "sigterm_shutdown": sigterm,
        "ctrlc_shutdown": ctrlc,
        "all_checks": all_checks,
        "accepted": all(all_checks.values()),
        "safety_boundary": {
            "runtime_live_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "order_placement_enabled": False,
            "cancel_enabled": False,
            "stop_sltp_bracket_enabled": False,
            "live_orders_performed": False,
        },
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(args.output)
    print(sha256_bytes(args.output.read_bytes()))
    return 0 if evidence["accepted"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
