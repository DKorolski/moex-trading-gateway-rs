#!/usr/bin/env python3
"""Generate M4-2c Gateway Config & Operational Parity evidence.

This script performs no broker calls. It validates that the M4-2c specification
is present, that broker-core exposes the canonical operational config/readiness
model, that P0 live blockers are documented, and that the scoped broker-core and
broker-finam tests pass.
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

DOC = Path("docs/m4-2c-gateway-config-operational-parity.md")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
BROKER_CORE_OPERATIONAL_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_CORE_OPERATIONAL_SNAPSHOT = Path("crates/broker-core/src/operational_snapshot.rs")

CANONICAL_TYPES = [
    "BrokerTruthSnapshot",
    "BrokerOrderSnapshot",
    "BrokerPositionSnapshot",
    "BrokerCashSnapshot",
    "BrokerInstrumentSpec",
    "BrokerTradeSnapshot",
    "BrokerReadinessSnapshot",
    "BrokerOperationalConfig",
    "BrokerCapabilityMatrix",
]

DERIVED_METHODS = [
    "target_position_qty",
    "target_is_flat",
    "target_active_orders",
    "account_active_orders",
    "unknown_orders",
    "cash_by_currency",
    "margin_sufficiency_for_order",
    "broker_truth_is_fresh",
    "live_entry_allowed",
]

REQUIRED_DOC_MARKERS = [
    "Config parity table",
    "Operational parity matrix",
    "P0 — blocks further live-position tests",
    "P1 — blocks continuous runtime",
    "P2 — technical debt",
    "target lifecycle truth is instrument-scoped",
    "account-wide truth is a safety guard",
    "zero-quantity position rows are diagnostic",
    "unknown order status blocks readiness",
    "stale broker truth blocks live entry",
    "This stage must not",
    "send live orders",
    "connect command-consumer to real FINAM",
]

ALOR_CONFIG_MARKERS = [
    "ALOR_WS_IDLE_TIMEOUT_SEC",
    "ALOR_WS_PING_INTERVAL_SEC",
    "ALOR_WS_PING_TIMEOUT_SEC",
    "ALOR_SUBSCRIBE_ACK_TIMEOUT_MS",
    "ALOR_SUBSCRIBE_ACK_RETRIES",
    "ALOR_CONTROL_PATH_STALE_AFTER_SEC",
    "ALOR_ACTION_SCOPE_ENABLE_MARKET",
    "ALOR_ACTION_SCOPE_CREATE_LIMIT",
    "ALOR_ACTION_SCOPE_DELETE_LIMIT",
    "ALOR_ACTION_SCOPE_REPLACE_LIMIT",
    "ALOR_ACTION_SCOPE_EXIT",
]

FINAM_CONFIG_MARKERS = [
    "FinamConfig",
    "GatewayFeatureSet",
    "BrokerTruthGatewayConfig",
    "CancelBrokerTruthFreshnessPolicy",
    "OrderPreflightPolicy",
    "FinamApiCapabilities",
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


def read_text(path: Path) -> str:
    return (ROOT / path).read_text()


def marker_check(text: str, markers: list[str]) -> dict[str, Any]:
    missing = [marker for marker in markers if marker not in text]
    return {"ok": not missing, "missing": missing, "checked": markers}


def count_p0_gaps(doc_text: str) -> int:
    p0_section = doc_text.split("### P0 — blocks further live-position tests", 1)[-1]
    p0_section = p0_section.split("### P1 — blocks continuous runtime", 1)[0]
    return sum(1 for line in p0_section.splitlines() if line.strip().startswith(tuple(f"{idx}." for idx in range(1, 21))))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2c-gateway-config-operational-parity-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CORE_LIB),
        artifact(BROKER_CORE_OPERATIONAL_CONFIG),
        artifact(BROKER_CORE_OPERATIONAL_SNAPSHOT),
    ]
    doc_text = read_text(DOC)
    lib_text = read_text(BROKER_CORE_LIB)
    operational_config_text = read_text(BROKER_CORE_OPERATIONAL_CONFIG)
    operational_snapshot_text = read_text(BROKER_CORE_OPERATIONAL_SNAPSHOT)
    broker_core_text = "\n".join([lib_text, operational_config_text, operational_snapshot_text])

    doc_markers = marker_check(doc_text, REQUIRED_DOC_MARKERS)
    alor_markers = marker_check(doc_text, ALOR_CONFIG_MARKERS)
    finam_markers = marker_check(doc_text, FINAM_CONFIG_MARKERS)
    canonical_types = marker_check(broker_core_text, CANONICAL_TYPES)
    derived_methods = marker_check(broker_core_text, DERIVED_METHODS)
    p0_gaps_count = count_p0_gaps(doc_text)

    broker_core_tests = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_finam_tests = run(["cargo", "test", "-p", "broker-finam", "m4_2b"])

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_live_calls_performed": True,
        "alor_config_inventory_ok": alor_markers["ok"],
        "finam_config_inventory_ok": finam_markers["ok"],
        "canonical_config_model_ok": canonical_types["ok"] and derived_methods["ok"],
        "operational_parity_matrix_ok": doc_markers["ok"],
        "p0_gaps_count": p0_gaps_count,
        "p0_gaps_block_live": p0_gaps_count >= 10 and "blocks further live-position tests" in doc_text,
        "broker_core_tests_ok": broker_core_tests["exit_code"] == 0,
        "broker_finam_tests_ok": broker_finam_tests["exit_code"] == 0,
        "broker_truth_snapshot_canonical_usage_ok": "BrokerTruthSnapshot" in doc_text
        and "local counters directly" in doc_text,
        "live_expansion_blocked": "Further live-position tests remain blocked" in doc_text
        and "runtime live" in doc_text
        and "command-consumer to real FINAM" in doc_text,
    }
    evidence_ready = all(
        value if isinstance(value, bool) else value > 0 for value in checks.values()
    )
    head = git_head()
    payload = {
        "evidence_kind": "m4-2c-gateway-config-operational-parity-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_markers,
            "alor_config_inventory": alor_markers,
            "finam_config_inventory": finam_markers,
            "canonical_types": canonical_types,
            "derived_methods": derived_methods,
        },
        "test_commands": {
            "broker_core_operational": broker_core_tests,
            "broker_finam_m4_2b": broker_finam_tests,
        },
        "checks": checks,
        "evidence_ready_for_review": evidence_ready,
    }

    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(checks | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
