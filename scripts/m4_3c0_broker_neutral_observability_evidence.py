#!/usr/bin/env python3
"""Generate M4-3c0 broker-neutral observability contract evidence.

This script is source-only. It does not call broker APIs, Redis, WebSocket, or
order endpoints. It verifies that ALOR and FINAM can map their different raw
streams into the same canonical observability channel set while keeping
runtime-live and real order paths disabled.
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

DOC = Path("docs/m4-3c0-broker-neutral-observability-contract.md")
OBSERVABILITY_SOURCE = Path("crates/broker-core/src/observability.rs")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
README = Path("README.md")

DOC_MARKERS = [
    "M4-3c0 broker-neutral observability contract",
    "ALOR raw gateway/runtime streams",
    "FINAM shadow REST/WS streams",
    "GatewayHealth",
    "GatewayReadiness",
    "BrokerTruth",
    "MarketData",
    "CommandAckLifecycle",
    "RuntimeState",
    "OpsConsumerGroups",
    "runtime state is not gateway state",
    "IMOEXF hybrid runtime implication",
    "Continuous runtime live remains blocked",
]

SOURCE_MARKERS = [
    "BrokerObservabilityChannelKind",
    "BrokerObservabilityOwner",
    "BrokerObservabilityContract",
    "BrokerObservabilityReadinessReport",
    "BrokerObservabilityBlocker",
    "BrokerConsumerGroupSnapshot",
    "validate_for_shadow_runtime",
    "validate_for_continuous_live_runtime",
    "live_order_authorized: false",
    "CommandConsumerToRealBrokerEnabled",
    "ContinuousRuntimeLiveEnabled",
    "TenMinuteBarParityMissing",
    "alor_and_finam_raw_outputs_map_to_same_canonical_observability_kinds",
    "runtime_state_is_strategy_owned_not_gateway_owned",
    "live_runtime_stays_blocked_until_parity_evidence_is_proven",
    "consumer_group_snapshot_requires_clean_pending_and_lag",
]

LIB_MARKERS = [
    "pub mod observability;",
    "BrokerObservabilityChannelKind",
    "BrokerObservabilityContract",
    "BrokerConsumerGroupSnapshot",
]

README_MARKERS = [
    "M4-3c0",
    "broker-neutral observability contract",
    "foundation for an IMOEXF hybrid shadow/dry runtime attachment",
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-3c0-broker-neutral-observability-evidence.json"),
    )
    parser.add_argument(
        "--skip-cargo",
        action="store_true",
        help="Skip cargo tests; source marker/scanner checks still run.",
    )
    args = parser.parse_args()

    checks = {
        "doc_markers": marker_check(DOC, DOC_MARKERS),
        "observability_source_markers": marker_check(OBSERVABILITY_SOURCE, SOURCE_MARKERS),
        "lib_exports": marker_check(BROKER_CORE_LIB, LIB_MARKERS),
        "readme_markers": marker_check(README, README_MARKERS),
    }

    commands: dict[str, Any] = {
        "python_compile": run(
            [
                "python3",
                "-m",
                "py_compile",
                "scripts/m4_3c0_broker_neutral_observability_evidence.py",
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
        commands["broker_core_observability_tests"] = run(
            ["cargo", "test", "-p", "broker-core", "observability::"]
        )

    markers_ok = all(check.get("ok") for check in checks.values())
    commands_ok = all(command.get("exit_code") == 0 for command in commands.values())
    ok = markers_ok and commands_ok

    evidence = {
        "evidence_kind": "m4-3c0-broker-neutral-observability-contract-v1",
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
        "live_order_authorized": False,
        "imoexf_hybrid_next_allowed_step": "shadow_or_dry_runtime_attachment_after_reviewed_10m_parity_and_consumer_group_evidence",
        "checks": checks,
        "commands": commands,
        "artifacts": [
            artifact(DOC),
            artifact(OBSERVABILITY_SOURCE),
            artifact(BROKER_CORE_LIB),
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
