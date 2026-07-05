#!/usr/bin/env python3
"""Generate M4-2j live-position pre-authorization no-send evidence.

The source checks do not perform broker calls. If --typed-readonly-report is
provided, the script validates a previously generated redacted GET-only FINAM
typed-readonly report with explicit no-send plain-micro stop-order waiver
approval. No POST/DELETE order endpoint is allowed by this evidence step.
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

DOC = Path("docs/m4-2j-live-position-preauthorization-no-send.md")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")
BROKER_CORE_CONFIG = Path("crates/broker-core/src/operational_config.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")

DOC_MARKERS = [
    "M4-2j live-position pre-authorization gate / no-send",
    "plain_micro_stop_waiver_operator_approval_present",
    "StopOrderNotRequiredForPlainMicro",
    "actual_send_allowed = false",
    "order_post_delete_calls_performed = false",
    "Live expansion remains blocked after M4-2j",
]

CLI_MARKERS = [
    "plain_micro_stop_waiver_operator_approved_no_send",
    "typed_readonly_plain_micro_stop_waiver_policy",
    "pre_waiver_canonical_preflight_blocks",
    "plain_micro_stop_waiver_operator_approval_present",
    "plain_micro_stop_waiver_source",
    "m4_2j_pre_authorization_gate",
    "m4_2j_no_send_pre_authorization_ready",
    "pre_authorization_evidence_only",
    "actual_send_allowed",
    "NoSendPreAuthorizationReady",
]

CORE_CONFIG_MARKERS = [
    "StopOrderNotRequiredForPlainMicro",
    "BrokerPlainMicroStopOrderWaiverPolicy",
    "StopOrderWaiverRejected",
    "strict_plain_micro_waiver_suppresses_only_stop_order_unsupported_block",
]

FINAM_MAPPER_MARKERS = [
    "stop_order_waiver_policy",
    "m4_2g_plain_micro_stop_order_waiver_allows_preflight_but_not_live_authorization",
    "no_live_authorization",
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


def validate_typed_readonly_report(path: Path | None, required: bool) -> dict[str, Any]:
    if path is None:
        return {
            "provided": False,
            "required": required,
            "ok": not required,
            "reason": "not_provided",
        }

    full_path = ROOT / path if not path.is_absolute() else path
    result: dict[str, Any] = {
        "provided": True,
        "required": required,
        "path": str(path),
        "exists": full_path.exists(),
    }
    if not full_path.exists():
        result.update({"ok": False, "reason": "missing_file"})
        return result

    data = full_path.read_bytes()
    result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    try:
        payload = json.loads(data)
    except json.JSONDecodeError as error:
        result.update({"ok": False, "reason": "invalid_json", "error": str(error)})
        return result

    records = payload.get("records")
    if not isinstance(records, list):
        result.update({"ok": False, "reason": "records_not_list"})
        return result
    matching_records = [
        record
        for record in records
        if isinstance(record, dict)
        and record.get("probe") == "canonical_readiness_package_typed"
    ]
    if not matching_records:
        result.update({"ok": False, "reason": "canonical_record_missing"})
        return result

    record = matching_records[-1]
    summary = record.get("summary")
    expected_pre_blocks = ["Readiness(StopOrderUnsupportedBlocked)"]
    summary_ok = isinstance(summary, dict) and all(
        [
            summary.get("truth_source") == "BrokerTruthSnapshot",
            summary.get("account_orphan_orders_count") == 0,
            summary.get("pre_waiver_canonical_preflight_blocks") == expected_pre_blocks,
            summary.get("plain_micro_stop_waiver_requested") is True,
            summary.get("plain_micro_stop_waiver_operator_approval_present") is True,
            summary.get("plain_micro_stop_waiver_source")
            == "StopOrderNotRequiredForPlainMicro",
            summary.get("stop_order_waiver_applied") is True,
            summary.get("m4_2j_pre_authorization_gate") is True,
            summary.get("m4_2j_no_send_pre_authorization_ready") is True,
            summary.get("pre_authorization_evidence_only") is True,
            summary.get("final_decision") == "NoSendPreAuthorizationReady",
            summary.get("actual_send_allowed") is False,
            summary.get("no_live_authorization") is True,
            summary.get("order_post_delete_calls_performed") is False,
            summary.get("live_order_calls_performed") is False,
            summary.get("trades_window_explicit") is True,
        ]
    )
    record_ok = all(
        [
            record.get("ok") is True,
            record.get("live_trading_enabled") is False,
            record.get("order_endpoints_used") is False,
            record.get("typed_probe") is True,
            summary_ok,
        ]
    )
    result.update(
        {
            "ok": record_ok,
            "record_count": len(records),
            "canonical_record_count": len(matching_records),
            "summary": summary if isinstance(summary, dict) else None,
            "reason": "ok" if record_ok else "m4_2j_no_send_preauth_record_incomplete",
        }
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2j-live-position-preauth-no-send-evidence.json"),
    )
    parser.add_argument(
        "--typed-readonly-report",
        type=Path,
        help="Previously generated redacted finam-typed-readonly-check preauth report.",
    )
    parser.add_argument(
        "--require-typed-readonly-report",
        action="store_true",
        help="Fail evidence if the no-send preauthorization report is absent.",
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CLI),
        artifact(BROKER_CORE_CONFIG),
        artifact(BROKER_FINAM_MAPPER),
    ]
    doc_check = marker_check(DOC, DOC_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)
    core_config_check = marker_check(BROKER_CORE_CONFIG, CORE_CONFIG_MARKERS)
    finam_mapper_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MAPPER_MARKERS)
    typed_readonly_report = validate_typed_readonly_report(
        args.typed_readonly_report,
        args.require_typed_readonly_report,
    )

    broker_core_plain_micro = run(["cargo", "test", "-p", "broker-core", "plain_micro"])
    broker_finam_m4_2g = run(["cargo", "test", "-p", "broker-finam", "m4_2g"])
    broker_cli_m4_1c = run(
        ["cargo", "test", "-p", "broker-cli", "m4_1c", "--no-default-features"]
    )
    forbidden_scan = run(["bash", "scripts/forbidden_surface_scan.sh"])
    forbidden_negative = run(["bash", "scripts/forbidden_surface_negative_harness.sh"])
    order_transition_scan = run(["bash", "scripts/order_endpoint_scanner_transition_spec.sh"])

    checks = {
        "artifacts_present": all(item["exists"] for item in artifacts),
        "no_live_order_calls_performed_by_script": True,
        "doc_ok": doc_check["ok"],
        "cli_no_send_preauth_summary_ok": cli_check["ok"],
        "core_waiver_policy_ok": core_config_check["ok"],
        "finam_package_waiver_support_ok": finam_mapper_check["ok"],
        "typed_readonly_report_ok": typed_readonly_report["ok"],
        "broker_core_plain_micro_regression_ok": broker_core_plain_micro["exit_code"] == 0,
        "broker_finam_m4_2g_regression_ok": broker_finam_m4_2g["exit_code"] == 0,
        "canonical_cli_regression_ok": broker_cli_m4_1c["exit_code"] == 0,
        "forbidden_surface_scan_ok": forbidden_scan["exit_code"] == 0,
        "forbidden_surface_negative_harness_ok": forbidden_negative["exit_code"] == 0,
        "order_endpoint_transition_scan_ok": order_transition_scan["exit_code"] == 0,
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2j-live-position-preauthorization-no-send-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "broker_api_calls_performed_by_this_script": False,
            "validated_report_is_get_only": typed_readonly_report.get("ok", False),
            "actual_order_send_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "preauthorization_policy": {
            "source": "StopOrderNotRequiredForPlainMicro",
            "requires_explicit_operator_no_send_approval": True,
            "final_actual_send_allowed": False,
            "separate_actual_package_required": True,
        },
        "real_readonly_no_send_preauth_package": typed_readonly_report,
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "broker_cli": cli_check,
            "broker_core_config": core_config_check,
            "broker_finam_mapper": finam_mapper_check,
        },
        "test_commands": {
            "broker_core_plain_micro": broker_core_plain_micro,
            "broker_finam_m4_2g": broker_finam_m4_2g,
            "broker_cli_m4_1c": broker_cli_m4_1c,
            "forbidden_surface_scan": forbidden_scan,
            "forbidden_surface_negative_harness": forbidden_negative,
            "order_endpoint_transition_scan": order_transition_scan,
        },
        "checks": checks,
        "evidence_ready": evidence_ready,
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"output": str(args.output), "evidence_ready": evidence_ready}))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
