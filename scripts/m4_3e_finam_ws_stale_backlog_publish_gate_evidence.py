#!/usr/bin/env python3
"""Generate M4-3e FINAM WS stale backlog publish-gate source evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]

DOC = Path("docs/m4-3e-finam-ws-stale-backlog-publish-gate.md")
CLI_SOURCE = Path("crates/broker-cli/src/main.rs")
README = Path("README.md")

DOC_MARKERS = [
    "M4-3e FINAM WS stale backlog publish gate",
    "must not be published as strategy-ready market-data bars",
    "stale_ws_final_bar_suppressed_count",
    "published_strategy_bar_count",
    "no live orders",
]

CLI_MARKERS = [
    "stale_ws_final_bars_published_for_strategy",
    "published_strategy_bar_count",
    "stale_ws_final_bar_suppressed_count",
    "let publish_strategy_bar = record_canonical_ws_bar_metrics",
    "if publish_strategy_bar",
    "finam_ws_shadow_strategy_publish_policy_suppresses_stale_backlog_bars",
]

README_MARKERS = [
    "M4-3e",
    "stale WS final-bar backlog",
    "only fresh final live bars are published",
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
    paths = [DOC, CLI_SOURCE, README]
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
        default=Path("reports/m4/m4-3e-finam-ws-stale-backlog-publish-gate-source-evidence.json"),
    )
    parser.add_argument("--skip-cargo", action="store_true")
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
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
                "scripts/m4_3e_finam_ws_stale_backlog_publish_gate_evidence.py",
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
        commands["broker_cli_finam_ws_shadow_tests"] = run(
            ["cargo", "test", "-p", "broker-cli", "finam_ws_shadow"]
        )

    markers_ok = all(check.get("ok") for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3e-finam-ws-stale-backlog-publish-gate-source-v1",
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
        "stale_ws_backlog_published_for_strategy": False,
        "active_session_evidence_required": True,
        "checks": checks,
        "commands": commands,
        "artifacts": [artifact(DOC), artifact(CLI_SOURCE), artifact(README)],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
