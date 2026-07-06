#!/usr/bin/env python3
"""M4-3j-b synthetic readiness guard + real WS-bound debug report evidence."""

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
REPORT = ROOT / "reports" / "m4" / "m4-3j-b-synthetic-readiness-guard-evidence.json"


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
    proc = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=True)
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
    return {
        "path": path,
        "status": response.status,
        "body_sha256": hashlib.sha256(body.encode()).hexdigest(),
        "body": json.loads(body),
    }


def probe_default_listener() -> dict[str, Any]:
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
        "finam-local-debug-m4-3jb",
    ]
    proc = subprocess.Popen(cmd, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
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
            stdout, stderr = proc.communicate(timeout=5)
    return {
        "cmd": cmd,
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
                "m4_3jb_synthetic_live_ready_is_test_only_and_marked_not_for_systemd",
                "m4_3jb_normal_listener_rejects_legacy_live_ready_cli_flag",
                "build_finam_ws_bound_debug_http_report",
                "broker_neutral_debug_surface",
                "synthetic_readiness",
                "not_for_systemd_readiness",
            ],
        ),
        "gateway_markers": marker_check(
            ROOT / "crates" / "finam-gateway" / "src" / "lib.rs",
            [
                "synthetic_readiness",
                "not_for_systemd_readiness",
                "BrokerNeutralReadinessHttpResponse",
                "BrokerNeutralDebugTransportResponse",
            ],
        ),
        "doc_markers": marker_check(
            ROOT / "docs" / "m4-3j-b-synthetic-readiness-guard.md",
            [
                "M4-3j-b",
                "synthetic_readiness = true",
                "not_for_systemd_readiness = true",
                "broker_neutral_debug_surface",
            ],
        ),
    }
    commands = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3jb_synthetic_readiness_guard_evidence.py"]),
        "targeted_m4_3jb_tests": run(["cargo", "test", "-p", "broker-cli", "m4_3jb", "--", "--nocapture"]),
        "targeted_m4_3ja_tests": run(["cargo", "test", "-p", "broker-cli", "m4_3ja", "--", "--nocapture"]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }
    legacy_flag = run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "broker-cli",
            "--",
            "finam-local-debug-http",
            "--bind",
            "127.0.0.1:18081",
            "--live-ready",
        ],
        timeout=30,
    )
    local_probe = probe_default_listener()
    responses = {row["path"]: row for row in local_probe["responses"]}
    readiness = responses.get("/readiness", {}).get("body") or {}
    debug = responses.get("/debug/transport", {}).get("body") or {}
    runtime_checks = {
        "legacy_live_ready_flag_rejected": legacy_flag["exit_code"] != 0,
        "listener_exit_ok": local_probe["exit_code"] == 0,
        "liveness_200": responses.get("/liveness", {}).get("status") == 200,
        "readiness_503_by_default": responses.get("/readiness", {}).get("status") == 503,
        "debug_transport_503_by_default": responses.get("/debug/transport", {}).get("status") == 503,
        "readiness_not_synthetic": readiness.get("synthetic_readiness") is False,
        "readiness_for_systemd_ok": readiness.get("not_for_systemd_readiness") is False,
        "debug_not_synthetic": debug.get("synthetic_readiness") is False,
        "debug_for_systemd_ok": debug.get("not_for_systemd_readiness") is False,
        "debug_redacted": debug.get("redacted") is True,
        "debug_runtime_live_disabled": debug.get("runtime_live_attachment_allowed") is False,
        "debug_order_post_delete_disabled": debug.get("order_post_delete_allowed") is False,
        "debug_command_consumer_disabled": debug.get("command_consumer_to_real_broker_enabled") is False,
    }
    report = {
        "evidence_kind": "m4-3j-b-synthetic-readiness-guard-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_sha(),
        "artifacts": [
            {"path": "crates/broker-cli/src/main.rs", "sha256": sha256_file(ROOT / "crates" / "broker-cli" / "src" / "main.rs")},
            {"path": "crates/finam-gateway/src/lib.rs", "sha256": sha256_file(ROOT / "crates" / "finam-gateway" / "src" / "lib.rs")},
            {"path": "docs/m4-3j-b-synthetic-readiness-guard.md", "sha256": sha256_file(ROOT / "docs" / "m4-3j-b-synthetic-readiness-guard.md")},
        ],
        "source_checks": source_checks,
        "commands": commands,
        "legacy_flag_probe": legacy_flag,
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
