#!/usr/bin/env python3
"""Generate M4-2i account-trades windowed orphan-resolution evidence.

The source checks do not perform broker calls. If --typed-readonly-report is
provided, the script validates a previously generated redacted GET-only FINAM
typed-readonly report with an explicit account-trades window. No POST/DELETE
order endpoint is allowed by this evidence step.
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

DOC = Path("docs/m4-2i-account-trades-windowed-orphan-resolution.md")
BROKER_CLI = Path("crates/broker-cli/src/main.rs")
BROKER_CORE_SNAPSHOT = Path("crates/broker-core/src/operational_snapshot.rs")
BROKER_FINAM_MAPPER = Path("crates/broker-finam/src/mapper.rs")

DOC_MARKERS = [
    "M4-2i account-trades windowed orphan-resolution package",
    "trades_window_start_ts",
    "trades_window_end_ts",
    "canonical_preflight_blocks",
    "orphan_order_reasons_by_kind",
    "order_post_delete_calls_performed = false",
    "Live expansion remains blocked after M4-2i",
]

CLI_MARKERS = [
    "canonical_readiness_package_typed",
    "account_transactions_typed",
    "canonical_preflight_blocks",
    "orphan_order_reasons_by_kind",
    "trades_window_start_ts",
    "trades_window_end_ts",
    "trades_window_explicit",
    "transactions_probe_ok",
    "readonly_broker_calls_performed",
    "order_post_delete_calls_performed",
    "live_order_calls_performed",
]

SNAPSHOT_MARKERS = [
    "FilledQuantityWithoutMatchingTrade",
    "account_orphan_orders",
    "orphan_reasons_for_order",
    "MatchingTradeQuantityLessThanFilledQuantity",
]

FINAM_MAPPER_MARKERS = [
    "build_finam_canonical_readiness_package",
    "map_finam_broker_truth_snapshot_with_readonly_artifacts",
    "trades: Option<&'a dto::AccountTradesResponse>",
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
    summary_ok = isinstance(summary, dict) and all(
        [
            summary.get("truth_source") == "BrokerTruthSnapshot",
            summary.get("readiness_source") == "BrokerReadinessSnapshot",
            summary.get("package_source") == "FinamCanonicalReadinessPackage",
            summary.get("no_live_authorization") is True,
            summary.get("trades_window_explicit") is True,
            isinstance(summary.get("trades_window_start_ts"), str),
            isinstance(summary.get("trades_window_end_ts"), str),
            isinstance(summary.get("canonical_preflight_blocks"), list),
            isinstance(summary.get("orphan_order_reasons_by_kind"), dict),
            isinstance(summary.get("filled_orders_count"), int),
            isinstance(summary.get("account_orphan_orders_count"), int),
            isinstance(summary.get("canonical_preflight_blocks_count"), int),
            summary.get("readonly_broker_calls_performed") is True,
            summary.get("order_post_delete_calls_performed") is False,
            summary.get("live_order_calls_performed") is False,
        ]
    )

    if isinstance(summary, dict):
        trades_ok = summary.get("trades_probe_ok") is True
        orphan_count = summary.get("account_orphan_orders_count")
        preflight_allowed = summary.get("canonical_preflight_allowed")
        conservative_if_unresolved = trades_ok or preflight_allowed is False or orphan_count == 0
    else:
        conservative_if_unresolved = False

    record_ok = all(
        [
            record.get("ok") is True,
            record.get("live_trading_enabled") is False,
            record.get("order_endpoints_used") is False,
            record.get("typed_probe") is True,
            summary_ok,
            conservative_if_unresolved,
        ]
    )
    result.update(
        {
            "ok": record_ok,
            "record_count": len(records),
            "canonical_record_count": len(matching_records),
            "summary": summary if isinstance(summary, dict) else None,
            "reason": "ok" if record_ok else "windowed_canonical_record_incomplete",
        }
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-2i-account-trades-windowed-orphan-resolution-evidence.json"),
    )
    parser.add_argument(
        "--typed-readonly-report",
        type=Path,
        help="Previously generated redacted finam-typed-readonly-check report.",
    )
    parser.add_argument(
        "--require-typed-readonly-report",
        action="store_true",
        help="Fail evidence if the real read-only canonical package report is absent.",
    )
    args = parser.parse_args()

    artifacts = [
        artifact(DOC),
        artifact(BROKER_CLI),
        artifact(BROKER_CORE_SNAPSHOT),
        artifact(BROKER_FINAM_MAPPER),
    ]
    doc_check = marker_check(DOC, DOC_MARKERS)
    cli_check = marker_check(BROKER_CLI, CLI_MARKERS)
    snapshot_check = marker_check(BROKER_CORE_SNAPSHOT, SNAPSHOT_MARKERS)
    finam_mapper_check = marker_check(BROKER_FINAM_MAPPER, FINAM_MAPPER_MARKERS)
    typed_readonly_report = validate_typed_readonly_report(
        args.typed_readonly_report,
        args.require_typed_readonly_report,
    )

    broker_core_orphan = run(["cargo", "test", "-p", "broker-core", "orphan"])
    broker_core_operational_snapshot = run(
        ["cargo", "test", "-p", "broker-core", "operational_snapshot"]
    )
    broker_finam_m4_2d = run(["cargo", "test", "-p", "broker-finam", "m4_2d"])
    broker_finam_m4_2h = run(["cargo", "test", "-p", "broker-finam", "m4_2h"])
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
        "cli_windowed_summary_ok": cli_check["ok"],
        "core_orphan_policy_ok": snapshot_check["ok"],
        "finam_canonical_mapper_ok": finam_mapper_check["ok"],
        "typed_readonly_report_ok": typed_readonly_report["ok"],
        "broker_core_orphan_regression_ok": broker_core_orphan["exit_code"] == 0,
        "broker_core_operational_snapshot_regression_ok": broker_core_operational_snapshot[
            "exit_code"
        ]
        == 0,
        "finam_mapper_regression_ok": broker_finam_m4_2d["exit_code"] == 0
        and broker_finam_m4_2h["exit_code"] == 0,
        "canonical_cli_regression_ok": broker_cli_m4_1c["exit_code"] == 0,
        "forbidden_surface_scan_ok": forbidden_scan["exit_code"] == 0,
        "forbidden_surface_negative_harness_ok": forbidden_negative["exit_code"] == 0,
        "order_endpoint_transition_scan_ok": order_transition_scan["exit_code"] == 0,
    }
    evidence_ready = all(checks.values())
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-2i-account-trades-windowed-orphan-resolution-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "trading_boundary": {
            "broker_api_calls_performed_by_this_script": False,
            "validated_report_is_get_only": typed_readonly_report.get("ok", False),
            "live_order_calls_allowed": False,
            "runtime_live_attachment_allowed": False,
            "command_consumer_to_real_finam_allowed": False,
            "stop_sltp_bracket_replace_multi_leg_allowed": False,
        },
        "orphan_resolution_policy": {
            "source": "BrokerTruthSnapshot",
            "account_trades_window_required_for_real_report": args.require_typed_readonly_report,
            "filled_order_without_matching_trade_remains_orphan": True,
            "unresolved_orphans_keep_preflight_blocked": True,
            "old_terminal_order_horizon_waiver_added": False,
        },
        "real_readonly_windowed_package": typed_readonly_report,
        "artifacts": artifacts,
        "marker_checks": {
            "doc": doc_check,
            "broker_cli": cli_check,
            "broker_core_snapshot": snapshot_check,
            "broker_finam_mapper": finam_mapper_check,
        },
        "test_commands": {
            "broker_core_orphan": broker_core_orphan,
            "broker_core_operational_snapshot": broker_core_operational_snapshot,
            "broker_finam_m4_2d": broker_finam_m4_2d,
            "broker_finam_m4_2h": broker_finam_m4_2h,
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
