#!/usr/bin/env python3
"""Generate M4-3c3 FINAM WS closed-bar finalizer evidence.

This script is source-only. It does not call FINAM, Redis, WebSocket, or order
endpoints. It verifies that raw FINAM WS forming bars are counted as diagnostics
but are not published as strategy bars before they become canonical closed bars.
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

DOC = Path("docs/m4-3c3-finam-ws-closed-bar-finalizer.md")
CORE_SOURCE = Path("crates/broker-core/src/bar_finalizer.rs")
CORE_EVENT = Path("crates/broker-core/src/event.rs")
CORE_LIB = Path("crates/broker-core/src/lib.rs")
CLI_SOURCE = Path("crates/broker-cli/src/main.rs")
README = Path("README.md")

DOC_MARKERS = [
    "M4-3c3 FINAM WS closed-bar finalizer",
    "raw forming bar N        -> buffer only, no strategy publish",
    "raw forming bar N+1      -> emit buffered bar N as final, buffer N+1",
    "late final bar N-1       -> suppress without deleting current forming bar N",
    "strategy_bars_are_final_only = true",
    "raw_forming_bars_published_for_strategy = false",
    "no broker order endpoints are enabled or called",
]

CORE_MARKERS = [
    "pub struct ClosedBarFinalizer",
    "pub enum ClosedBarFinalizerActionKind",
    "BufferedForming",
    "EmittedClosedFromNextBar",
    "SuppressedDuplicateFinal",
    "DroppedNonMonotonicForming",
    "closed_bar_finalizer_keeps_current_forming_after_late_duplicate_final",
]

CORE_EVENT_MARKERS = [
    "pub enum MarketDataSourceKind",
    "Hash, Serialize, Deserialize",
]

CORE_LIB_MARKERS = [
    "pub mod bar_finalizer",
    "ClosedBarFinalizer",
    "ClosedBarFinalizerActionKind",
    "ClosedBarStreamKey",
]

CLI_MARKERS = [
    "ClosedBarFinalizer::default",
    "record_inbound_ws_bar_metrics",
    "record_canonical_ws_bar_metrics",
    "record_closed_bar_finalizer_action",
    "closed_bar_finalizer.observe_bar",
    "closed_bar_finalizer_enabled",
    "strategy_bars_are_final_only",
    "raw_forming_bars_published_for_strategy",
    "forming_bar_suppressed_count",
    "duplicate_final_suppressed_count",
]

README_MARKERS = [
    "M4-3c3",
    "FINAM WS closed-bar finalizer",
    "market-data stream receives only canonical finalized bars",
]

FORBIDDEN_SURFACE = [
    "real_order_endpoint_enabled = true",
    "command_consumer_to_real_finam_enabled = true",
    "continuous_runtime_live_enabled = true",
]


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


def forbidden_surface_check() -> dict[str, Any]:
    paths = [DOC, CORE_SOURCE, CORE_EVENT, CORE_LIB, CLI_SOURCE, README]
    findings = []
    for path in paths:
        full_path = ROOT / path
        if not full_path.exists():
            continue
        text = full_path.read_text()
        for token in FORBIDDEN_SURFACE:
            if token in text:
                findings.append({"path": str(path), "token": token})
    return {
        "ok": not findings,
        "checked_paths": [str(path) for path in paths],
        "forbidden_tokens": FORBIDDEN_SURFACE,
        "findings": findings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3c3-finam-ws-closed-bar-finalizer-evidence.json"),
    )
    parser.add_argument("--skip-cargo", action="store_true")
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
        "core_markers": marker_check(CORE_SOURCE, CORE_MARKERS),
        "core_event_markers": marker_check(CORE_EVENT, CORE_EVENT_MARKERS),
        "core_lib_markers": marker_check(CORE_LIB, CORE_LIB_MARKERS),
        "cli_markers": marker_check(CLI_SOURCE, CLI_MARKERS),
        "readme_markers": marker_check(README, README_MARKERS),
        "forbidden_surface": forbidden_surface_check(),
    }

    commands: dict[str, Any] = {
        "python_compile": run(
            [
                "python3",
                "-m",
                "py_compile",
                "scripts/m4_3c3_finam_ws_closed_bar_finalizer_evidence.py",
            ]
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
        commands["broker_core_closed_bar_finalizer_tests"] = run(
            ["cargo", "test", "-p", "broker-core", "closed_bar_finalizer"]
        )
        commands["broker_cli_ws_lifecycle_tests"] = run(
            [
                "cargo",
                "test",
                "-p",
                "broker-cli",
                "finam_ws_shadow_bars_are_required_for_strategy_parity_readiness",
            ]
        )

    markers_ok = all(check.get("ok") for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3c3-finam-ws-closed-bar-finalizer-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_only": True,
        "broker_calls_performed": False,
        "redis_calls_performed": False,
        "websocket_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "strategy_market_data_source": "FinamWebSocketBarsLiveStream",
        "closed_bar_finalizer_enabled": True,
        "strategy_bars_are_final_only": True,
        "raw_forming_bars_published_for_strategy": False,
        "first_live_final_bar_required": True,
        "forming_live_bars_satisfy_readiness": False,
        "checks": checks,
        "commands": commands,
        "artifacts": [
            artifact(DOC),
            artifact(CORE_SOURCE),
            artifact(CORE_EVENT),
            artifact(CORE_LIB),
            artifact(CLI_SOURCE),
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
