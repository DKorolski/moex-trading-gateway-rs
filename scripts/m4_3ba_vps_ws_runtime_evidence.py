#!/usr/bin/env python3
"""Collect M4-3b-a VPS FINAM WebSocket runtime evidence.

The collector is intentionally bounded and redacted. It reads systemd state,
Redis stream metadata, and one `finam-ws-shadow-once` report from the VPS. It
does not call FINAM order endpoints and does not store tokens, account ids, raw
market-data values, or raw WebSocket payloads.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

DEFAULT_OUTPUT = Path("reports/m4/m4-3b-a-vps-ws-runtime-evidence.json")
DEFAULT_SERVICE = "moex-finam-ws-shadow.service"
DEFAULT_REST_SERVICE = "moex-finam-shadow.service"
DEFAULT_REMOTE_ROOT = "/opt/moex-trading-project"
DEFAULT_REMOTE_ENV = "/opt/moex-trading-project-shadow.env"
DEFAULT_REMOTE_CONFIG = "/opt/moex-trading-project-runtime/finam-ws-shadow.vps.runtime.json"
DEFAULT_REDIS_CONTAINER = "trading-hybrid-redis-1"

WS_STREAMS = {
    "health": "finam_ws_shadow:health",
    "readiness": "finam_ws_shadow:readiness",
    "market_data": "finam_ws_shadow:market_data",
    "command_ack_disabled": "finam_ws_shadow:command_acks_disabled",
}

XINFO_ALLOWED_KEYS = {
    "length",
    "radix-tree-keys",
    "radix-tree-nodes",
    "last-generated-id",
    "max-deleted-entry-id",
    "entries-added",
    "recorded-first-entry-id",
    "groups",
}

FORBIDDEN_SURFACE = [
    "Method::POST",
    "Method::DELETE",
    ".post(",
    ".delete(",
    "POST /v1/accounts",
    "DELETE /v1/accounts",
    "real_order_endpoint_enabled = true",
    "command_consumer_to_real_finam_enabled = true",
    "continuous_runtime_live_enabled = true",
]

SECRET_MARKERS = [
    "eyJ",
    "tapi_",
    "refresh_token",
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return sha256_bytes(text.encode())


def run(cmd: list[str], *, input_text: str | None = None, timeout: int = 120) -> dict[str, Any]:
    completed = subprocess.run(
        cmd,
        cwd=ROOT,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=timeout,
    )
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "stdout_sha256": sha256_text(completed.stdout),
        "stderr_sha256": sha256_text(completed.stderr),
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def run_simple(cmd: list[str]) -> str:
    return subprocess.check_output(cmd, cwd=ROOT, text=True).strip()


def ssh_base(args: argparse.Namespace) -> list[str]:
    command = ["ssh", "-o", "BatchMode=yes", "-o", "ServerAliveInterval=10"]
    if args.ssh_key:
        command.extend(["-i", str(Path(args.ssh_key).expanduser())])
    command.append(args.ssh_host)
    return command


def ssh(args: argparse.Namespace, remote_command: str, *, timeout: int = 120) -> str:
    attempts = max(1, int(getattr(args, "ssh_retries", 3)))
    last_completed: subprocess.CompletedProcess[str] | None = None
    for attempt in range(1, attempts + 1):
        completed = subprocess.run(
            ssh_base(args) + ["bash", "-lc", remote_command],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout,
        )
        if completed.returncode == 0:
            return completed.stdout.strip()
        last_completed = completed
        if attempt < attempts:
            time.sleep(min(2 * attempt, 5))
    assert last_completed is not None
    raise RuntimeError(
        f"ssh command failed after {attempts} attempts: {remote_command}\n"
        f"stdout={last_completed.stdout[-1000:]}\nstderr={last_completed.stderr[-1000:]}"
    )


def safe_host_identity(host: str) -> dict[str, Any]:
    return {
        "redacted": True,
        "sha256": sha256_text(host),
        "len": len(host),
    }


def parse_systemctl_show(output: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in output.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        result[key] = value
    return result


def sanitized_status_lines(output: str) -> list[str]:
    lines: list[str] = []
    for raw_line in output.splitlines()[:20]:
        line = raw_line.strip()
        if not line:
            continue
        if any(marker in line for marker in SECRET_MARKERS):
            lines.append("<redacted-secret-like-line>")
            continue
        lines.append(line)
    return lines


def redis_cli(args: argparse.Namespace, *redis_args: str) -> str:
    quoted = " ".join(shlex.quote(part) for part in redis_args)
    container = shlex.quote(args.redis_container)
    return ssh(args, f"docker exec {container} redis-cli --raw {quoted}", timeout=60)


def parse_int(value: str) -> int | None:
    try:
        return int(value.strip())
    except ValueError:
        return None


def parse_xinfo(output: str) -> dict[str, str]:
    lines = [line for line in output.splitlines() if line != ""]
    result: dict[str, str] = {}
    index = 0
    while index < len(lines):
        key = lines[index]
        value = lines[index + 1] if index + 1 < len(lines) else ""
        if key in XINFO_ALLOWED_KEYS:
            result[key] = value
        index += 2
    return result


def latest_stream_payload(args: argparse.Namespace, stream: str) -> dict[str, Any]:
    output = redis_cli(args, "XREVRANGE", stream, "+", "-", "COUNT", "1")
    lines = output.splitlines()
    if not lines:
        return {"present": False}
    entry_id = lines[0]
    fields: dict[str, str] = {}
    for index in range(1, len(lines) - 1, 2):
        fields[lines[index]] = lines[index + 1]
    payload = fields.get("payload")
    if not payload:
        return {"present": True, "entry_id": entry_id, "payload_present": False}
    payload_json = json.loads(payload)
    return {
        "present": True,
        "entry_id": entry_id,
        "payload_present": True,
        "payload_sha256": sha256_text(payload),
        "payload_shape": json_shape(payload_json),
        "payload_summary": market_data_payload_summary(payload_json)
        if payload_json.get("msg_type") == "MarketData"
        else typed_envelope_summary(payload_json),
    }


def json_shape(value: Any, depth: int = 0) -> dict[str, Any]:
    if isinstance(value, dict):
        keys = sorted(str(key) for key in value.keys())
        if depth >= 4:
            return {"kind": "object", "keys": keys, "truncated": True}
        return {
            "kind": "object",
            "keys": keys,
            "fields": [
                {"key": str(key), "shape": json_shape(item, depth + 1)}
                for key, item in sorted(value.items(), key=lambda item: str(item[0]))
            ],
        }
    if isinstance(value, list):
        return {
            "kind": "array",
            "len": len(value),
            "item_kinds": sorted({json_kind(item) for item in value}),
            "first_item_shape": json_shape(value[0], depth + 1) if value else None,
        }
    return {"kind": json_kind(value)}


def json_kind(value: Any) -> str:
    if isinstance(value, dict):
        return "object"
    if isinstance(value, list):
        return "array"
    if isinstance(value, str):
        return "string"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, (int, float)):
        return "number"
    if value is None:
        return "null"
    return type(value).__name__


def typed_envelope_summary(payload_json: dict[str, Any]) -> dict[str, Any]:
    payload = payload_json.get("payload", {})
    return {
        "schema_version": payload_json.get("schema_version"),
        "msg_type": payload_json.get("msg_type"),
        "source_present": bool(payload_json.get("source")),
        "payload_keys": sorted(payload.keys()) if isinstance(payload, dict) else [],
    }


def market_data_payload_summary(payload_json: dict[str, Any]) -> dict[str, Any]:
    payload = payload_json.get("payload", {})
    if not isinstance(payload, dict) or not payload:
        return typed_envelope_summary(payload_json)
    variant = next(iter(payload.keys()))
    event = payload.get(variant, {})
    source_kind = event.get("source_kind") if isinstance(event, dict) else None
    instrument = event.get("instrument", {}) if isinstance(event, dict) else {}
    return {
        "schema_version": payload_json.get("schema_version"),
        "msg_type": payload_json.get("msg_type"),
        "variant": variant,
        "source_kind": source_kind,
        "instrument_present": isinstance(instrument, dict) and bool(instrument),
        "venue_symbol_present": bool(instrument.get("venue_symbol")) if isinstance(instrument, dict) else False,
        "price_fields_present": sorted(
            key for key in ["bid", "ask", "last", "open", "high", "low", "close", "volume"] if key in event
        )
        if isinstance(event, dict)
        else [],
        "source_ts_present": bool(event.get("source_ts")) if isinstance(event, dict) else False,
        "received_ts_present": bool(event.get("received_ts")) if isinstance(event, dict) else False,
        "is_final": event.get("is_final") if isinstance(event, dict) else None,
    }


def collect_service_snapshot(args: argparse.Namespace, service: str) -> dict[str, Any]:
    active = ssh(args, f"systemctl is-active {shlex.quote(service)} || true")
    enabled = ssh(args, f"systemctl is-enabled {shlex.quote(service)} || true")
    show = parse_systemctl_show(
        ssh(
            args,
            "systemctl show "
            f"{shlex.quote(service)} "
            "-p ActiveState -p SubState -p NRestarts -p MemoryCurrent "
            "-p MainPID -p ExecMainStatus --no-pager",
        )
    )
    status = ssh(args, f"systemctl status {shlex.quote(service)} --no-pager -n 5 || true")
    return {
        "service": service,
        "active": active,
        "enabled": enabled,
        "active_state": show.get("ActiveState"),
        "sub_state": show.get("SubState"),
        "n_restarts": parse_int(show.get("NRestarts", "")),
        "memory_current_bytes": parse_int(show.get("MemoryCurrent", "")),
        "main_pid_present": parse_int(show.get("MainPID", "0")) not in (None, 0),
        "exec_main_status": parse_int(show.get("ExecMainStatus", "")),
        "status_lines_redacted": sanitized_status_lines(status),
    }


def collect_redis_snapshot(args: argparse.Namespace) -> dict[str, Any]:
    streams: dict[str, Any] = {}
    for name, stream in WS_STREAMS.items():
        exists = parse_int(redis_cli(args, "EXISTS", stream)) or 0
        length = parse_int(redis_cli(args, "XLEN", stream)) or 0
        xinfo = parse_xinfo(redis_cli(args, "XINFO", "STREAM", stream)) if exists else {}
        latest = latest_stream_payload(args, stream) if exists and length > 0 else {"present": False}
        streams[name] = {
            "stream": stream,
            "exists": bool(exists),
            "length": length,
            "xinfo": xinfo,
            "latest": latest,
        }
    memory_output = redis_cli(args, "INFO", "memory")
    memory: dict[str, str] = {}
    for line in memory_output.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key in {"used_memory_human", "used_memory_peak_human", "mem_fragmentation_ratio"}:
            memory[key] = value.strip()
    return {"container": args.redis_container, "streams": streams, "memory": memory}


def collect_resources(args: argparse.Namespace) -> dict[str, Any]:
    free = ssh(args, "free -m | sed -n '2p'")
    disk = ssh(args, "df -h / | tail -1")
    return {
        "free_m_line": free,
        "root_disk_line": disk,
    }


def collect_one_shot(args: argparse.Namespace) -> dict[str, Any]:
    remote_root = shlex.quote(args.remote_root)
    remote_env = shlex.quote(args.remote_env)
    remote_config = shlex.quote(args.remote_config)
    command = (
        f"set -euo pipefail; set -a; . {remote_env}; set +a; "
        f"{remote_root}/target/debug/broker-cli finam-ws-shadow-once "
        f"--config {remote_config} "
        f"--max-duration-seconds {int(args.max_duration_seconds)} "
        f"--max-messages {int(args.max_messages)}"
    )
    output = ssh(args, command, timeout=max(90, int(args.max_duration_seconds) + 60))
    return json.loads(output)


def local_forbidden_surface_scan() -> dict[str, Any]:
    paths = [
        ROOT / "crates/broker-finam/src/ws.rs",
        ROOT / "crates/broker-cli/src/main.rs",
        ROOT / "docs/m4-3b-finam-websocket-stream-shadow.md",
        ROOT / "docs/m4-3b-a-vps-ws-runtime-evidence.md",
        ROOT / "config/finam-ws-shadow.vps.example.json",
        ROOT / "README.md",
    ]
    findings: list[dict[str, str]] = []
    for path in paths:
        if not path.exists():
            continue
        text = path.read_text()
        for token in FORBIDDEN_SURFACE:
            if token in text:
                findings.append({"path": str(path.relative_to(ROOT)), "token": token})
    return {
        "ok": not findings,
        "checked_paths": [str(path.relative_to(ROOT)) for path in paths if path.exists()],
        "forbidden_tokens": FORBIDDEN_SURFACE,
        "findings": findings,
    }


def validate_report(report: dict[str, Any]) -> dict[str, Any]:
    ws = report["vps"]["ws_service"]
    redis = report["vps"]["redis"]
    streams = redis["streams"]
    one_shot = report["vps"]["one_shot_report"]
    one_shot_metrics = one_shot.get("metrics", {})

    checks = {
        "ws_service_active": ws.get("active") == "active",
        "ws_service_disabled_on_boot": ws.get("enabled") == "disabled",
        "ws_service_no_restarts": ws.get("n_restarts") == 0,
        "rest_shadow_active": report["vps"]["rest_shadow_service"].get("active") == "active",
        "redis_market_data_stream_positive": streams["market_data"].get("length", 0) > 0,
        "redis_command_ack_disabled_absent": streams["command_ack_disabled"].get("exists") is False,
        "one_shot_no_live": one_shot.get("live_trading_enabled") is False,
        "one_shot_no_command_consumer": one_shot.get("command_consumer_enabled") is False,
        "one_shot_no_order_placement": one_shot.get("order_placement_enabled") is False,
        "one_shot_no_cancel": one_shot.get("cancel_enabled") is False,
        "one_shot_live_stream_source": one_shot.get("market_data", {}).get("source_kind") == "LiveStream",
        "one_shot_decode_clean": one_shot_metrics.get("decode_error_count") == 0,
        "one_shot_mapper_clean": one_shot_metrics.get("mapper_error_count") == 0,
        "one_shot_market_data_present": one_shot.get("market_data", {}).get("published_market_data_count", 0) > 0,
        "latest_market_data_live_stream": streams["market_data"]
        .get("latest", {})
        .get("payload_summary", {})
        .get("source_kind")
        == "LiveStream",
        "local_forbidden_surface_scan": report["local_checks"]["no_order_endpoint_surface"]["ok"] is True,
    }
    return {"ok": all(checks.values()), "checks": checks}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ssh-host", required=True)
    parser.add_argument("--ssh-key")
    parser.add_argument("--remote-root", default=DEFAULT_REMOTE_ROOT)
    parser.add_argument("--remote-env", default=DEFAULT_REMOTE_ENV)
    parser.add_argument("--remote-config", default=DEFAULT_REMOTE_CONFIG)
    parser.add_argument("--service", default=DEFAULT_SERVICE)
    parser.add_argument("--rest-service", default=DEFAULT_REST_SERVICE)
    parser.add_argument("--redis-container", default=DEFAULT_REDIS_CONTAINER)
    parser.add_argument("--ssh-retries", type=int, default=3)
    parser.add_argument("--max-duration-seconds", type=int, default=30)
    parser.add_argument("--max-messages", type=int, default=20)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--skip-cargo", action="store_true")
    args = parser.parse_args()

    head = run_simple(["git", "rev-parse", "HEAD"])
    short_head = run_simple(["git", "rev-parse", "--short", "HEAD"])
    archive_path = ROOT / "reports" / "handoff" / f"moex-trading-project-{short_head}.zip"

    local_commands: dict[str, Any] = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3ba_vps_ws_runtime_evidence.py"]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(
            ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]
        ),
    }
    if not args.skip_cargo:
        local_commands["broker_finam_ws_tests"] = run(["cargo", "test", "-p", "broker-finam", "ws::"])
        local_commands["broker_cli_ws_config_test"] = run(
            ["cargo", "test", "-p", "broker-cli", "finam_ws_shadow_config"]
        )

    report: dict[str, Any] = {
        "evidence_kind": "m4-3b-a-vps-ws-runtime-evidence-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": short_head,
        "source_archive_name": archive_path.name if archive_path.exists() else None,
        "source_archive_sha256": sha256_bytes(archive_path.read_bytes()) if archive_path.exists() else None,
        "vps_host": safe_host_identity(args.ssh_host),
        "runtime_evidence": True,
        "finam_websocket_calls_performed": True,
        "finam_rest_calls_performed": False,
        "redis_calls_performed": True,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "order_placement_enabled": False,
        "cancel_enabled": False,
        "stop_sltp_bracket_enabled": False,
        "strategy_runtime_attached": False,
        "cutover_automatic": False,
        "live_order_authorized": False,
        "vps": {
            "ws_service": collect_service_snapshot(args, args.service),
            "rest_shadow_service": collect_service_snapshot(args, args.rest_service),
            "redis": collect_redis_snapshot(args),
            "one_shot_report": collect_one_shot(args),
            "resources": collect_resources(args),
        },
        "local_checks": {
            "no_order_endpoint_surface": local_forbidden_surface_scan(),
            "commands": local_commands,
        },
    }
    validation = validate_report(report)
    report["ok"] = validation["ok"] and all(
        command.get("exit_code") == 0 for command in local_commands.values()
    )
    report["validation"] = validation

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
