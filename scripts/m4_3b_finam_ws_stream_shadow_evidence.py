#!/usr/bin/env python3
"""Generate M4-3b FINAM WebSocket stream shadow evidence.

This script is source-only. It does not call FINAM, Redis, WebSocket, or order
endpoints. It verifies that the WS shadow implementation exists, remains
market-data-only, and keeps the live/order surface disabled.
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

DOC = Path("docs/m4-3b-finam-websocket-stream-shadow.md")
WS_SOURCE = Path("crates/broker-finam/src/ws.rs")
CLI_SOURCE = Path("crates/broker-cli/src/main.rs")
BROKER_FINAM_LIB = Path("crates/broker-finam/src/lib.rs")
VPS_CONFIG = Path("config/finam-ws-shadow.vps.example.json")
ROOT_CARGO = Path("Cargo.toml")
README = Path("README.md")

DOC_MARKERS = [
    "M4-3b FINAM WebSocket stream shadow",
    "streaming market-data shadow / no-live / no order endpoints",
    "FINAM WebSocket shadow streams",
    "FINAM REST read-only truth snapshots",
    "send FINAM `POST /orders`",
    "send FINAM `DELETE /orders/{id}`",
    "QUOTES",
    "BARS",
    "MarketDataSourceKind::LiveStream",
    "bar.close_ts <= received_ts",
    "finam_ws_shadow:market_data",
    "command_acks_disabled",
]

WS_SOURCE_MARKERS = [
    "FinamWsEnvelope",
    "build_ws_subscribe_bars_request",
    "build_ws_subscribe_quotes_request",
    "\"action\": \"SUBSCRIBE\"",
    "\"type\": \"BARS\"",
    "\"type\": \"QUOTES\"",
    "map_ws_market_data_events",
    "MarketDataSourceKind::LiveStream",
    "mapped.is_final = mapped.close_ts <= received_ts",
    "websocket_subscribe_requests_keep_expected_wire_shape",
    "websocket_quote_envelope_maps_to_live_stream_market_data",
    "websocket_bar_envelope_marks_only_closed_bars_final",
]

CLI_SOURCE_MARKERS = [
    "finam-ws-shadow-once",
    "finam-ws-shadow-loop",
    "connect_async",
    "build_ws_subscribe_quotes_request",
    "build_ws_subscribe_bars_request",
    "publish_market_data_event",
    "gateway_config.features.command_consumer_enabled = false",
    "gateway_config.features.order_placement_enabled = false",
    "gateway_config.features.cancel_enabled = false",
    "gateway_config.features.stop_sltp_bracket_enabled = false",
    "ReadinessReason::OperatorLiveArmMissing",
    "MarketDataNotLive",
    "finam_ws_shadow_config_keeps_live_order_features_disabled",
]

LIB_MARKERS = [
    "pub mod ws;",
    "pub use ws::*;",
]

CARGO_MARKERS = [
    "tokio-tungstenite",
    "connect",
    "rustls-tls-webpki-roots",
    "futures-util",
]

README_MARKERS = [
    "finam-ws-shadow-once",
    "finam-ws-shadow-loop",
    "finam_ws_shadow:*",
    "M4-3b FINAM WebSocket stream shadow",
]

VPS_CONFIG_EXPECTED = {
    "account_id": "ACC_TEST_0001",
    "symbol": "TICKER@MIC",
    "source": "finam-ws-shadow-vps",
    "timeframe": "TIME_FRAME_M1",
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    full_path = ROOT / path
    result: dict[str, Any] = {"path": str(path), "exists": full_path.exists()}
    if full_path.exists():
        data = full_path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


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


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()


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


def validate_vps_config() -> dict[str, Any]:
    full_path = ROOT / VPS_CONFIG
    result: dict[str, Any] = {"path": str(VPS_CONFIG), "exists": full_path.exists()}
    if not full_path.exists():
        result.update({"ok": False, "reason": "missing"})
        return result
    text = full_path.read_text()
    payload = json.loads(text)
    streams = payload.get("streams", {})
    expected_ok = all(payload.get(key) == value for key, value in VPS_CONFIG_EXPECTED.items())
    stream_namespace_ok = isinstance(streams, dict) and all(
        str(value).startswith("finam_ws_shadow:") for value in streams.values()
    )
    disabled_ack_stream_named = streams.get("command_ack") == "finam_ws_shadow:command_acks_disabled"
    max_iterations_safe = payload.get("max_iterations") == 3
    no_live_literals = all(
        forbidden not in text
        for forbidden in [
            "tapi_",
            "eyJ",
            "client_code",
            "refresh_token",
        ]
    )
    ok = (
        expected_ok
        and stream_namespace_ok
        and disabled_ack_stream_named
        and max_iterations_safe
        and no_live_literals
    )
    result.update(
        {
            "ok": ok,
            "synthetic_account": payload.get("account_id"),
            "synthetic_symbol": payload.get("symbol"),
            "source": payload.get("source"),
            "stream_namespace_ok": stream_namespace_ok,
            "disabled_ack_stream_named": disabled_ack_stream_named,
            "max_iterations_safe": max_iterations_safe,
            "no_live_literals": no_live_literals,
            "reason": "ok" if ok else "vps_ws_shadow_config_not_synthetic_or_not_namespaced",
        }
    )
    return result


def no_order_endpoint_surface() -> dict[str, Any]:
    paths = [WS_SOURCE, CLI_SOURCE, DOC, README, VPS_CONFIG]
    forbidden = [
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
    findings: list[dict[str, str]] = []
    for path in paths:
        full_path = ROOT / path
        if not full_path.exists():
            continue
        text = full_path.read_text()
        for token in forbidden:
            if token in text:
                findings.append({"path": str(path), "token": token})
    return {
        "ok": not findings,
        "checked_paths": [str(path) for path in paths],
        "forbidden_tokens": forbidden,
        "findings": findings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3b-finam-ws-stream-shadow-evidence.json"),
    )
    parser.add_argument(
        "--skip-cargo",
        action="store_true",
        help="Skip cargo tests; source marker/scanner checks still run.",
    )
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
        "ws_source_markers": marker_check(WS_SOURCE, WS_SOURCE_MARKERS),
        "cli_source_markers": marker_check(CLI_SOURCE, CLI_SOURCE_MARKERS),
        "broker_finam_lib_exports": marker_check(BROKER_FINAM_LIB, LIB_MARKERS),
        "cargo_ws_dependencies": marker_check(ROOT_CARGO, CARGO_MARKERS),
        "readme_markers": marker_check(README, README_MARKERS),
        "vps_ws_shadow_config": validate_vps_config(),
        "no_order_endpoint_surface": no_order_endpoint_surface(),
    }

    commands: dict[str, Any] = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3b_finam_ws_stream_shadow_evidence.py"]
        ),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(
            ["bash", "scripts/forbidden_surface_negative_harness.sh"]
        ),
        "order_endpoint_scanner_transition_spec": run(
            ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]
        ),
    }
    if not args.skip_cargo:
        commands["broker_finam_ws_tests"] = run(["cargo", "test", "-p", "broker-finam", "ws::"])
        commands["broker_cli_ws_config_test"] = run(
            ["cargo", "test", "-p", "broker-cli", "finam_ws_shadow_config"]
        )

    markers_ok = all(check.get("ok") for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3b-finam-ws-stream-shadow-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "m4_3b_finam_websocket_stream_shadow": ok,
        "source_only": True,
        "finam_rest_calls_performed": False,
        "finam_websocket_calls_performed": False,
        "redis_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "order_placement_enabled": False,
        "cancel_enabled": False,
        "stop_sltp_bracket_enabled": False,
        "market_data_source_kind": "LiveStream",
        "ws_shadow_stream_namespace": "finam_ws_shadow:*",
        "rest_shadow_stream_namespace": "finam_shadow:*",
        "alor_live_oracle_remains_separate": True,
        "strategy_runtime_attached": False,
        "cutover_automatic": False,
        "live_order_authorized": False,
        "checks": checks,
        "commands": commands,
        "artifacts": [
            artifact(DOC),
            artifact(WS_SOURCE),
            artifact(CLI_SOURCE),
            artifact(BROKER_FINAM_LIB),
            artifact(VPS_CONFIG),
            artifact(ROOT_CARGO),
            artifact(README),
        ],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
