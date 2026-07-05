#!/usr/bin/env python3
"""Generate M4-3c5 FINAM WS reconnect/gap-recovery source evidence.

This script is source-only. It does not call FINAM, ALOR, Redis, WebSocket, SSH,
or order endpoints. Runtime reconnect evidence must be collected separately
during an active market phase after the contract is wired into the WS loop.
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

DOC = Path("docs/m4-3c5-finam-ws-reconnect-gap-recovery.md")
CORE_SOURCE = Path("crates/broker-core/src/market_data_recovery.rs")
CORE_LIB = Path("crates/broker-core/src/lib.rs")
README = Path("README.md")
FINAM_WS_SOURCE = Path("crates/broker-finam/src/ws.rs")
FINAM_CLI_SOURCE = Path("crates/broker-cli/src/main.rs")
ALOR_ORACLE_DOC = Path("docs/m4-2a-alor-operational-oracle.md")

DOC_MARKERS = [
    "M4-3c5 FINAM WS reconnect/gap-recovery parity contract",
    "reconnect is not the same thing as data recovery",
    "REST `Bars` replay for the gap window",
    "WebSocket resubscribe for live data",
    "first live final bar",
    "Runtime attachment must continue to be blocked",
    "real FINAM order POST/DELETE",
]

CORE_MARKERS = [
    "pub enum MarketDataRecoveryMode",
    "pub enum MarketDataRecoveryPhase",
    "pub enum MarketDataRecoveryBlocker",
    "pub struct MarketDataRecoveryPlan",
    "pub struct MarketDataRecoveryReport",
    "pub fn plan_market_data_recovery",
    "pub fn evaluate_market_data_recovery",
    "ReplayWindowDoesNotCoverWatermark",
    "ReplayNotContiguousToWatermark",
    "FirstLiveFinalBeforeReplayEnd",
    "GapAbsenceNotProven",
    "recovery_accepts_contiguous_replay_and_fresh_live_final_bar",
    "recovery_blocks_when_replay_does_not_cover_last_watermark",
    "recovery_requires_first_live_final_bar_after_subscription",
]

CORE_LIB_MARKERS = [
    "pub mod market_data_recovery",
    "evaluate_market_data_recovery",
    "plan_market_data_recovery",
    "MarketDataRecoveryReport",
]

README_MARKERS = [
    "M4-3c5",
    "reconnect/gap-recovery contract",
    "REST Bars",
    "first live final bar",
]

FINAM_WS_MARKERS = [
    "build_ws_subscribe_bars_request",
    '"symbol": symbol',
    '"timeframe": timeframe',
]

FINAM_CLI_MARKERS = [
    "run_finam_ws_shadow_loop",
    "reconnect_delay_seconds",
    "publish_degraded_state",
    "ClosedBarFinalizer",
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
    paths = [DOC, CORE_SOURCE, CORE_LIB, README]
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
        default=Path("reports/m4/m4-3c5-finam-ws-reconnect-gap-recovery-source-evidence.json"),
    )
    parser.add_argument("--skip-cargo", action="store_true")
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
        "core_markers": marker_check(CORE_SOURCE, CORE_MARKERS),
        "core_lib_markers": marker_check(CORE_LIB, CORE_LIB_MARKERS),
        "readme_markers": marker_check(README, README_MARKERS),
        "finam_ws_shape_markers": marker_check(FINAM_WS_SOURCE, FINAM_WS_MARKERS),
        "finam_cli_reconnect_markers": marker_check(FINAM_CLI_SOURCE, FINAM_CLI_MARKERS),
        "alor_oracle_doc_present": artifact(ALOR_ORACLE_DOC),
        "forbidden_surface": forbidden_surface_check(),
    }

    commands: dict[str, Any] = {
        "python_compile": run(
            [
                "python3",
                "-m",
                "py_compile",
                "scripts/m4_3c5_finam_ws_reconnect_gap_recovery_evidence.py",
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
        commands["broker_core_market_data_recovery_tests"] = run(
            ["cargo", "test", "-p", "broker-core", "recovery_"]
        )

    markers_ok = all(check.get("ok", check.get("exists", False)) for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3c5-finam-ws-reconnect-gap-recovery-source-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "source_only": True,
        "broker_calls_performed": False,
        "redis_calls_performed": False,
        "websocket_calls_performed": False,
        "ssh_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "finam_ws_reconnect_loop_already_exists": True,
        "finam_gap_replay_wiring_pending": True,
        "alor_parity_oracle_used": True,
        "requires_future_active_session_runtime_evidence": True,
        "checks": checks,
        "commands": commands,
        "artifacts": [
            artifact(DOC),
            artifact(CORE_SOURCE),
            artifact(CORE_LIB),
            artifact(README),
            artifact(FINAM_WS_SOURCE),
            artifact(FINAM_CLI_SOURCE),
            artifact(ALOR_ORACLE_DOC),
        ],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
