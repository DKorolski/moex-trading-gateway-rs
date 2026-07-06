#!/usr/bin/env python3
"""M4-3k ALOR ↔ FINAM observability parity evidence."""

from __future__ import annotations

import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "reports" / "m4" / "m4-3k-observability-parity-evidence.json"


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


def main() -> int:
    source_checks = {
        "gateway_markers": marker_check(
            ROOT / "crates" / "finam-gateway" / "src" / "lib.rs",
            [
                "BrokerNeutralObservabilityParityReport",
                "BrokerNeutralObservabilityCapability",
                "alor_debug_cws_observability_source_shape",
                "finam_observability_source_shape_from_debug_surface",
                "build_broker_neutral_observability_parity_report",
                "m4_3k_alor_finam_observability_parity_passes_on_neutral_surface",
                "m4_3k_alor_finam_observability_parity_detects_missing_session_watchdog",
                "m4_3ka_observability_parity_rejects_non_live_ready_http_200",
                "m4_3ka_observability_parity_rejects_live_ready_http_503",
            ],
        ),
        "doc_markers": marker_check(
            ROOT / "docs" / "m4-3k-alor-finam-observability-parity.md",
            [
                "M4-3k",
                "ALOR",
                "FINAM",
                "DebugTransportRoute",
                "SessionWatchdog",
                "M4-3l dry runtime attach",
            ],
        ),
        "strictness_doc_markers": marker_check(
            ROOT / "docs" / "m4-3k-a-readiness-http-semantics-strictness.md",
            [
                "M4-3k-a",
                "ReadinessPhase::LiveReady -> HTTP 200",
                "any other readiness phase -> HTTP 503",
                "Reconciliation + HTTP 200",
                "LiveReady      + HTTP 503",
            ],
        ),
    }
    commands = {
        "python_compile": run(["python3", "-m", "py_compile", "scripts/m4_3k_observability_parity_evidence.py"]),
        "targeted_parity_tests": run(["cargo", "test", "-p", "finam-gateway", "m4_3k", "--", "--nocapture"]),
        "forbidden_surface_scan": run(["bash", "scripts/forbidden_surface_scan.sh"]),
        "forbidden_surface_negative_harness": run(["bash", "scripts/forbidden_surface_negative_harness.sh"]),
        "order_endpoint_scanner_transition_spec": run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"]),
    }
    report = {
        "evidence_kind": "m4-3k-observability-parity-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_sha(),
        "artifacts": [
            {"path": "crates/finam-gateway/src/lib.rs", "sha256": sha256_file(ROOT / "crates" / "finam-gateway" / "src" / "lib.rs")},
            {"path": "docs/m4-3k-alor-finam-observability-parity.md", "sha256": sha256_file(ROOT / "docs" / "m4-3k-alor-finam-observability-parity.md")},
            {"path": "docs/m4-3k-a-readiness-http-semantics-strictness.md", "sha256": sha256_file(ROOT / "docs" / "m4-3k-a-readiness-http-semantics-strictness.md")},
        ],
        "source_checks": source_checks,
        "commands": commands,
        "parity_schema": "m4_3k_alor_finam_observability_parity",
        "m4_3ka_readiness_http_semantics_strict": True,
        "critical_capabilities": [
            "LivenessRoute",
            "ReadinessRoute",
            "DebugTransportRoute",
            "ReadinessHttpStatusRule",
            "TransportConnected",
            "WsGeneration",
            "SubscriptionCounts",
            "DataQualityLedger",
            "RecoveryState",
            "SessionWatchdog",
            "RedactedDebug",
            "RuntimeLiveDisabledFlag",
            "CommandConsumerToRealBrokerDisabledFlag",
            "OrderPostDeleteDisabledFlag",
        ],
        "runtime_live_attachment_allowed": False,
        "command_consumer_to_real_finam_enabled": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
    }
    report["ok"] = (
        all(check["ok"] for check in source_checks.values())
        and all(command["exit_code"] == 0 for command in commands.values())
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
