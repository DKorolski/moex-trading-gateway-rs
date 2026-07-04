#!/usr/bin/env python3
"""Build redacted M3j-16 LimitCancel evidence from a dry-run report."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_output(args: list[str]) -> str | None:
    try:
        return subprocess.check_output(["git", *args], text=True).strip()
    except Exception:
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run-report", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-archive", type=Path)
    args = parser.parse_args()

    dry_run = json.loads(args.dry_run_report.read_text())
    report = dry_run.get("report", {})
    scope = dry_run.get("operator_scope", {})
    truth = dry_run.get("pre_boundary_broker_truth", {})
    execution = dry_run.get("execution_redacted", {})
    bindings = dry_run.get("redacted_bindings", {})
    quote = dry_run.get("reference_quote_redacted", {})

    evidence = {
        "m3j_step": "M3j-16",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git_output(["rev-parse", "HEAD"]),
        "source_commit_short_sha": git_output(["rev-parse", "--short", "HEAD"]),
        "dry_run_report_name": args.dry_run_report.name,
        "dry_run_report_sha256": sha256_file(args.dry_run_report),
        "source_archive_name": args.source_archive.name if args.source_archive else None,
        "source_archive_sha256": sha256_file(args.source_archive) if args.source_archive else None,
        "limit_cancel_one_shot_dry_run_ok": report.get("decision") == "DryRunNoSend",
        "m3j16a_pre_actual_gate_ok": (
            report.get("decision") == "ActualSendAllowed"
            and execution.get("pre_actual_gate_only") is True
            and report.get("boundary_invocation_performed") is False
            and report.get("place_attempted") is False
            and report.get("cancel_attempted") is False
        ),
        "dry_run_no_send": report.get("dry_run_no_send") is True,
        "actual_send_allowed": report.get("actual_send_allowed") is True,
        "boundary_invocation_performed": report.get("boundary_invocation_performed") is True,
        "place_attempted": report.get("place_attempted") is True,
        "cancel_attempted": report.get("cancel_attempted") is True,
        "real_finam_order_endpoint_used": report.get("real_finam_order_endpoint_used") is True,
        "feature_gate_required": True,
        "actual_send_flag_required": True,
        "feature_enabled": execution.get("compile_feature_enabled") is True,
        "actual_send_flag_present": execution.get("actual_send_flag_present") is True,
        "pre_actual_gate_only": execution.get("pre_actual_gate_only") is True,
        "symbol_exact_match_or_hash": bindings.get("symbol_exact_match_or_hash") is True,
        "account_operator_binding_ok": bool(bindings.get("operator_approval_digest")),
        "operator_approval_digest_present": bool(bindings.get("operator_approval_digest")),
        "reference_quote_bound_to_fresh_artifact": quote.get("reference_quote_bound_to_fresh_artifact") is True,
        "reference_quote_fresh": quote.get("reference_quote_fresh") is True,
        "reference_quote_age_ms": quote.get("quote_age_ms"),
        "reference_quote_artifact_digest_present": bool(bindings.get("reference_quote_artifact_digest")),
        "price_guard_ok": report.get("price_guard_ok") is True,
        "limit_price_below_reference": scope.get("limit_price_below_reference") is True,
        "qty": scope.get("qty"),
        "max_orders": scope.get("max_orders"),
        "active_orders_count": truth.get("active_orders_count"),
        "unknown_active_orders_count": truth.get("unknown_active_orders_count"),
        "orphan_active_orders_count": truth.get("orphan_active_orders_count"),
        "terminal_or_ignored_orders_count": truth.get("terminal_or_ignored_orders_count"),
        "orders_total": truth.get("orders_total"),
        "positions_count": truth.get("positions_count"),
        "broker_truth_clean": truth.get("broker_truth_clean") is True,
        "kill_switch_ok": report.get("risk_controls_ok") is True,
        "audit_before_boundary_required": report.get("audit_and_reconciliation_ok") is True,
        "post_run_reconciliation_required": report.get("audit_and_reconciliation_ok") is True,
        "redaction_ok": report.get("redaction_ok") is True,
        "raw_secret_exported": False,
        "raw_account_exported": False,
        "raw_symbol_exported": False,
        "raw_broker_payload_exported": False,
        "execution_redacted_summary": {
            "actual_send_flag_present": execution.get("actual_send_flag_present"),
            "compile_feature_enabled": execution.get("compile_feature_enabled"),
            "place_attempted": execution.get("place_attempted"),
            "cancel_attempted": execution.get("cancel_attempted"),
            "broker_order_id_present": execution.get("broker_order_id_present"),
        },
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    print(json.dumps(evidence, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
