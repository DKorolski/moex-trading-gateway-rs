#!/usr/bin/env python3
"""M4-3j-a local HTTP/debug listener evidence."""

from __future__ import annotations

import hashlib
import http.client
import json
import socket
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports" / "m4" / "m4-3j-a-local-http-debug-listener-evidence.json"


def sha256_file(path: Path) -> str | None:
    if not path.exists():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(cmd: list[str], timeout: int = 120) -> dict[str, Any]:
    proc = subprocess.run(
        cmd,
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    return {
        "cmd": cmd,
        "exit_code": proc.returncode,
        "stdout_sha256": hashlib.sha256(proc.stdout.encode()).hexdigest(),
        "stderr_sha256": hashlib.sha256(proc.stderr.encode()).hexdigest(),
        "stdout_tail": proc.stdout[-4000:],
        "stderr_tail": proc.stderr[-4000:],
    }


def git_sha() -> str:
    proc = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=True,
    )
    return proc.stdout.strip()


def marker_check(path: Path, markers: list[str]) -> dict[str, Any]:
    text = path.read_text() if path.exists() else ""
    missing = [marker for marker in markers if marker not in text]
    return {
        "path": str(path.relative_to(ROOT)),
        "exists": path.exists(),
        "checked": markers,
        "missing": missing,
        "ok": path.exists() and not missing,
    }


def free_local_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_port(port: int, timeout_sec: float = 20.0) -> None:
    deadline = time.time() + timeout_sec
    while time.time() < deadline:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.2)
            if sock.connect_ex(("127.0.0.1", port)) == 0:
                return
        time.sleep(0.1)
    raise TimeoutError(f"local debug listener did not open port {port}")


def http_get(port: int, path: str) -> dict[str, Any]:
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    conn.request("GET", path)
    response = conn.getresponse()
    body = response.read().decode()
    conn.close()
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError:
        parsed = None
    return {
        "path": path,
        "status": response.status,
        "body_sha256": hashlib.sha256(body.encode()).hexdigest(),
        "body": parsed,
    }


def probe_local_listener() -> dict[str, Any]:
    port = free_local_port()
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "broker-cli",
        "--",
        "finam-local-debug-http",
        "--bind",
        f"127.0.0.1:{port}",
        "--max-requests",
        "4",
        "--source",
        "finam-local-debug-evidence",
    ]
    proc = subprocess.Popen(
        cmd,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        wait_for_port(port)
        responses = [
            http_get(port, "/liveness"),
            http_get(port, "/readiness"),
            http_get(port, "/debug/transport"),
        ]
        stdout, stderr = proc.communicate(timeout=20)
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.communicate(timeout=5)

    return {
        "cmd": cmd,
        "port": port,
        "exit_code": proc.returncode,
        "stdout_sha256": hashlib.sha256(stdout.encode()).hexdigest(),
        "stderr_sha256": hashlib.sha256(stderr.encode()).hexdigest(),
        "stdout_tail": stdout[-4000:],
        "stderr_tail": stderr[-4000:],
        "responses": responses,
    }


def main() -> int:
    source_checks = {
        "cli_markers": marker_check(
            ROOT / "crates" / "broker-cli" / "src" / "main.rs",
            [
                "finam-local-debug-http",
                "run_finam_local_debug_http",
                "build_finam_local_debug_http_report",
                "m4_3ja_local_http_debug_bind_policy_rejects_public_addresses",
                "m4_3ja_local_http_debug_report_is_redacted_and_no_live_by_default",
            ],
        ),
        "gateway_markers": marker_check(
            ROOT / "crates" / "finam-gateway" / "src" / "lib.rs",
            [
                "BrokerNeutralDebugTransportSnapshot",
                "data_quality_ledger",
                "session_watchdog",
                "recovery",
            ],
        ),
        "doc_markers": marker_check(
            ROOT / "docs" / "m4-3j-a-local-http-debug-listener.md",
            [
                "M4-3j-a",
                "GET /liveness",
                "GET /readiness",
                "GET /debug/transport",
                "127.0.0.1",
                "no FINAM order POST/DELETE",
            ],
        ),
    }
    commands = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3ja_local_http_debug_listener_evidence.py"]),
        "targeted_cli_tests": run(["cargo", "test", "-p", "broker-cli", "m4_3ja", "--", "--nocapture"]),
        "targeted_gateway_tests": run(["cargo", "test", "-p", "finam-gateway", "m4_3j_broker_neutral_http_debug_surface", "--", "--nocapture"]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }
    local_probe = probe_local_listener()
    responses = {row["path"]: row for row in local_probe["responses"]}
    debug_body = responses.get("/debug/transport", {}).get("body") or {}
    debug_transports = debug_body.get("transports") or []
    ws_debug = next(
        (
            row
            for row in debug_transports
            if row.get("transport_kind") == "WebSocketMarketData"
        ),
        {},
    )
    runtime_checks = {
        "listener_exit_ok": local_probe["exit_code"] == 0,
        "liveness_200": responses.get("/liveness", {}).get("status") == 200,
        "readiness_503_by_default": responses.get("/readiness", {}).get("status") == 503,
        "debug_transport_503_by_default": responses.get("/debug/transport", {}).get("status") == 503,
        "debug_transport_redacted": debug_body.get("redacted") is True,
        "debug_no_raw_secrets": debug_body.get("raw_secrets_exported") is False,
        "debug_no_raw_tokens": debug_body.get("raw_tokens_exported") is False,
        "debug_no_raw_account_ids": debug_body.get("raw_account_ids_exported") is False,
        "debug_runtime_live_disabled": debug_body.get("runtime_live_attachment_allowed") is False,
        "debug_command_consumer_disabled": debug_body.get("command_consumer_to_real_broker_enabled") is False,
        "debug_order_post_delete_disabled": debug_body.get("order_post_delete_allowed") is False,
        "debug_has_ws_generation": bool(ws_debug.get("connection_generation")),
        "debug_has_subscription_counts": all(
            key in ws_debug
            for key in [
                "desired_subscriptions_count",
                "active_subscriptions_count",
                "pending_subscriptions_count",
            ]
        ),
        "debug_has_data_quality": "data_quality_ledger" in ws_debug,
        "debug_has_recovery": "recovery" in ws_debug,
        "debug_has_session_watchdog": "session_watchdog" in ws_debug,
    }
    report = {
        "evidence_kind": "m4-3j-a-local-http-debug-listener-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_sha(),
        "artifacts": [
            {
                "path": "crates/broker-cli/src/main.rs",
                "sha256": sha256_file(ROOT / "crates" / "broker-cli" / "src" / "main.rs"),
            },
            {
                "path": "crates/finam-gateway/src/lib.rs",
                "sha256": sha256_file(ROOT / "crates" / "finam-gateway" / "src" / "lib.rs"),
            },
            {
                "path": "docs/m4-3j-a-local-http-debug-listener.md",
                "sha256": sha256_file(ROOT / "docs" / "m4-3j-a-local-http-debug-listener.md"),
            },
        ],
        "source_checks": source_checks,
        "commands": commands,
        "local_probe": local_probe,
        "runtime_checks": runtime_checks,
        "actual_http_listener_enabled": True,
        "bind_scope": "127.0.0.1",
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
    }
    report["ok"] = (
        all(check["ok"] for check in source_checks.values())
        and all(command["exit_code"] == 0 for command in commands.values())
        and all(runtime_checks.values())
        and report["actual_http_listener_enabled"]
        and not report["runtime_live_attachment_allowed"]
        and not report["command_consumer_to_real_finam_enabled"]
        and not report["post_delete_calls_performed"]
        and not report["live_orders_performed"]
    )

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
