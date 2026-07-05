#!/usr/bin/env python3
"""Generate M4-3a dual-broker shadow parity evidence.

This script is source-only. It does not call broker APIs, Redis, or order
endpoints. It verifies that the broker-neutral parity foundation is present and
that the VPS shadow config remains synthetic/no-live.
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

DOC = Path("docs/m4-3a-dual-broker-shadow-parity.md")
PARITY_SOURCE = Path("crates/broker-core/src/parity.rs")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
VPS_CONFIG = Path("config/finam-gateway-shadow.vps.example.json")

DOC_MARKERS = [
    "M4-3a dual-broker shadow parity foundation",
    "ALOR mature contour / oracle",
    "FINAM new contour / shadow",
    "live_order_authorized = false",
    "Only one broker may be active for live trading at a time",
    "finam-gateway-shadow-loop",
    "IMOEXF",
    "USDRUBF",
    "RI/RTS",
    "Cutover criteria",
]

PARITY_MARKERS = [
    "BrokerTruthParityReport",
    "BrokerBarParityReport",
    "BrokerParityIssueKind",
    "compare_broker_truth_for_instrument",
    "compare_final_bars_for_instrument",
    "M4-3aDualBrokerShadowParity",
    "cutover_safe",
    "bars_synchronized",
    "live_order_authorized: false",
    "TargetPositionQtyMismatch",
    "BarOhlcvMismatch",
]

LIB_MARKERS = [
    "pub mod parity;",
    "compare_broker_truth_for_instrument",
    "compare_final_bars_for_instrument",
    "BrokerTruthParityReport",
]

VPS_CONFIG_EXPECTED = {
    "account_id": "ACC_TEST_0001",
    "symbol": "TICKER@MIC",
    "source": "finam-gateway-shadow-vps",
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
    payload = json.loads(full_path.read_text())
    streams = payload.get("streams", {})
    expected_ok = all(payload.get(key) == value for key, value in VPS_CONFIG_EXPECTED.items())
    stream_namespace_ok = isinstance(streams, dict) and all(
        str(value).startswith("finam_shadow:") for value in streams.values()
    )
    max_iterations_safe = payload.get("max_iterations") == 3
    no_live_literals = all(
        str(forbidden) not in full_path.read_text()
        for forbidden in ["tapi_", "eyJ", "client_code", "refresh_token"]
    )
    result.update(
        {
            "ok": expected_ok and stream_namespace_ok and max_iterations_safe and no_live_literals,
            "synthetic_account": payload.get("account_id"),
            "synthetic_symbol": payload.get("symbol"),
            "source": payload.get("source"),
            "stream_namespace_ok": stream_namespace_ok,
            "max_iterations_safe": max_iterations_safe,
            "no_live_literals": no_live_literals,
            "reason": "ok"
            if expected_ok and stream_namespace_ok and max_iterations_safe and no_live_literals
            else "vps_shadow_config_not_synthetic_or_not_namespaced",
        }
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3a-dual-broker-shadow-parity-evidence.json"),
    )
    parser.add_argument(
        "--skip-cargo",
        action="store_true",
        help="Skip cargo tests; source marker/scanner checks still run.",
    )
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
        "parity_source_markers": marker_check(PARITY_SOURCE, PARITY_MARKERS),
        "lib_exports": marker_check(BROKER_CORE_LIB, LIB_MARKERS),
        "vps_shadow_config": validate_vps_config(),
    }

    commands: dict[str, Any] = {
        "python_compile": run(
            ["python3", "-m", "py_compile", "scripts/m4_3a_dual_broker_shadow_parity_evidence.py"]
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
        commands["broker_core_parity_tests"] = run(
            ["cargo", "test", "-p", "broker-core", "parity::"]
        )

    markers_ok = all(check.get("ok") for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3a-dual-broker-shadow-parity-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_head(),
        "ok": ok,
        "m4_3a_dual_broker_shadow_parity_foundation": ok,
        "source_only": True,
        "broker_calls_performed": False,
        "redis_calls_performed": False,
        "post_delete_calls_performed": False,
        "live_orders_performed": False,
        "command_consumer_to_real_finam_enabled": False,
        "continuous_runtime_live_enabled": False,
        "alor_active_finam_shadow_shape_documented": checks["doc_markers"].get("ok") is True,
        "cutover_automatic": False,
        "live_order_authorized": False,
        "checks": checks,
        "commands": commands,
        "artifacts": [
            artifact(DOC),
            artifact(PARITY_SOURCE),
            artifact(BROKER_CORE_LIB),
            artifact(VPS_CONFIG),
        ],
    }

    output_path = ROOT / args.output
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
