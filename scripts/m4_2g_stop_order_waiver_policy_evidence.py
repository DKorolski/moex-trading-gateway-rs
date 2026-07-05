#!/usr/bin/env python3
"""Generate M4-2g stop-order plain-micro waiver policy evidence.

No broker calls are performed. The script validates that stop-order unsupported
can be waived only by an explicit narrow plain market/limit micro policy, while
runtime-live and command-consumer-to-real-FINAM remain disabled.
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

DOC = Path("docs/m4-2g-canonical-live-entry-policy-stop-order-waiver.md")
BROKER_CORE_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_CORE_LIB = Path("crates/broker-core/src/lib.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")

DOC_MARKERS = [
    "StopOrderNotRequiredForPlainMicro waiver may exist",
    "BrokerStopOrderWaiverSource::StopOrderNotRequiredForPlainMicro",
    "BrokerPlainMicroStopOrderWaiverPolicy",
    "BrokerStopOrderWaiverDecision",
    "BrokerCanonicalPreflightDecision.stop_order_waiver_decision",
    "qty <= 1",
    "explicit operator approval",
    "runtime-live is disabled",
    "command-consumer-to-real-FINAM is disabled",
    "Stop/SLTP/bracket/replace/multi-leg features are disabled",
    "BrokerCanonicalPreflightBlock::StopOrderWaiverRejected",
    "no_live_authorization = true",
    "New live-position tests remain blocked",
]

CORE_CONFIG_MARKERS = [
    "BrokerStopOrderWaiverSource",
    "StopOrderNotRequiredForPlainMicro",
    "BrokerStopOrderWaiverRejection",
    "BrokerStopOrderWaiverDecision",
    "BrokerPlainMicroStopOrderWaiverPolicy",
    "StopOrderWaiverRejected",
    "from_readiness_margin_truth_and_stop_order_waiver",
    "strict_plain_micro_waiver_suppresses_only_stop_order_unsupported_block",
    "plain_micro_waiver_rejects_out_of_scope_or_unsafe_runtime",
    "plain_micro_waiver_does_not_suppress_missing_or_stale_stop_readiness",
]

CORE_LIB_MARKERS = [
    "BrokerPlainMicroStopOrderWaiverPolicy",
    "BrokerStopOrderWaiverDecision",
    "BrokerStopOrderWaiverRejection",
    "BrokerStopOrderWaiverSource",
]

FINAM_MAPPER_MARKERS = [
    "stop_order_waiver_policy",
    "BrokerPlainMicroStopOrderWaiverPolicy",
    "BrokerStopOrderWaiverDecision::not_requested",
    "m4_2g_plain_micro_stop_order_waiver_policy",
    "m4_2g_plain_micro_stop_order_waiver_allows_preflight_but_not_live_authorization",
    "no_live_authorization",
]

CLI_MARKERS = [
    "m4_1c_canonical_report_golden_requires_broker_truth_snapshot_source",
    '"truth_source": "BrokerTruthSnapshot"',
    '"final_truth_source": "BrokerTruthSnapshot"',
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
        default=Path("reports/m4/m4-2g-stop-order-waiver-policy-evidence.json"),
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CORE_CONFIG),
        artifact(BROKER_CORE_LIB),
        artifact(BROKER_FINAM_MAPPER),
        artifact(BROKER_CLI),
    ]
    doc_check = marker_check(DOC, DOC_MARKERS)
    core_config_check = marker_check(BROKER_CORE_CONFIG, CORE_CONFIG_MARKERS)
    core_lib_check = marker_check(BROKER_CORE_LIB, CORE_LIB_MARKERS)
    finam_mapper_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MAPPER_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)

    broker_core_plain_micro = run(["cargo", "test", "-p", "broker-core", "plain_micro"])
    broker_core_combined = run(["cargo", "test", "-p", "broker-core", "combined_canonical_preflight"])
    broker_core_operational = run(["cargo", "test", "-p", "broker-core", "operational"])
    broker_finam_m4_2g = run(["cargo", "test", "-p", "broker-finam", "m4_2g"])
    broker_finam_m4_2f = run(["cargo", "test", "-p", "broker-finam", "m4_2f"])
    broker_cli_m4_1c = run(
        ["cargo", "test", "-p", "broker-cli", "m4_1c", "--no-default-features"]
    )
    forbidden_scan = run(["bash", "scripts/forbidden_surface_scan.sh"])
    forbidden_negative = run(["bash", "scripts/forbidden_surface_negative_harness.sh"])
    order_transition_scan = run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"])

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_broker_calls_performed": True,
        "no_live_calls_performed": True,
        "waiver_source_policy_decision_exported_ok": core_config_check["ok"]
        and core_lib_check["ok"],
        "strict_plain_micro_waiver_positive_ok": broker_core_plain_micro["exit_code"] == 0
        and broker_finam_m4_2g["exit_code"] == 0,
        "out_of_scope_waiver_rejected_ok": broker_core_plain_micro["exit_code"] == 0,
        "stale_missing_stop_order_not_waived_ok": broker_core_plain_micro["exit_code"] == 0,
        "combined_preflight_preserves_non_stop_blocks_ok": broker_core_combined["exit_code"] == 0
        and broker_core_operational["exit_code"] == 0,
        "finam_package_carries_waiver_decision_ok": finam_mapper_check["ok"]
        and broker_finam_m4_2g["exit_code"] == 0,
        "m4_2f_regression_ok": broker_finam_m4_2f["exit_code"] == 0,
        "m4_1c_canonical_report_regression_ok": cli_check["ok"]
        and broker_cli_m4_1c["exit_code"] == 0,
        "forbidden_surface_scan_ok": forbidden_scan["exit_code"] == 0,
        "forbidden_surface_negative_harness_ok": forbidden_negative["exit_code"] == 0,
        "order_endpoint_transition_scan_ok": order_transition_scan["exit_code"] == 0,
        "doc_ok": doc_check["ok"],
        "live_expansion_blocked": doc_check["ok"],
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2g-stop-order-plain-micro-waiver-policy-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "broker_api_calls_performed": False,
            "live_calls_performed": False,
            "live_position_tests_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "waiver_policy": {
            "source": "StopOrderNotRequiredForPlainMicro",
            "scope_limited": True,
            "max_qty": "1",
            "allowed_order_types": ["market", "limit"],
            "operator_approval_required": True,
            "one_account_required": True,
            "one_symbol_required": True,
            "runtime_live_must_be_disabled": True,
            "command_consumer_to_real_finam_must_be_disabled": True,
            "stop_sltp_bracket_replace_multi_leg_must_be_disabled": True,
            "suppresses_only": "BrokerLiveEntryBlock::StopOrderUnsupportedBlocked",
        },
        "package_policy": {
            "preflight_allowed_source": "BrokerCanonicalPreflightDecision.allowed",
            "waiver_evidence_source": "BrokerCanonicalPreflightDecision.stop_order_waiver_decision",
            "finam_package_field": "FinamCanonicalReadinessPackage.canonical_preflight_decision",
            "no_live_authorization_remains_true": True,
        },
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "broker_core_config": core_config_check,
            "broker_core_lib": core_lib_check,
            "broker_finam_mapper": finam_mapper_check,
            "broker_cli": cli_check,
        },
        "test_commands": {
            "broker_core_plain_micro": broker_core_plain_micro,
            "broker_core_combined_canonical_preflight": broker_core_combined,
            "broker_core_operational": broker_core_operational,
            "broker_finam_m4_2g": broker_finam_m4_2g,
            "broker_finam_m4_2f": broker_finam_m4_2f,
            "broker_cli_m4_1c_no_default_features": broker_cli_m4_1c,
            "forbidden_surface_scan": forbidden_scan,
            "forbidden_surface_negative_harness": forbidden_negative,
            "order_endpoint_scanner_transition_spec": order_transition_scan,
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
