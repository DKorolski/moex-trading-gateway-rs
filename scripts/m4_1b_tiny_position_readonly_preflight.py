#!/usr/bin/env python3
"""Generate M4-1b tiny-position real read-only preflight evidence.

This script performs no broker calls. It aggregates already-created auth,
typed-readonly and guarded no-send preflight reports.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DOC = Path("docs/m4-1b-tiny-position-readonly-preflight-evidence.md")

REQUIRED_DOC_MARKERS = [
    "does not send broker orders",
    "does not open a position",
    "finam-auth-check",
    "finam-typed-readonly-check",
    "finam-limit-cancel-one-shot --pre-actual-gate-only",
    "active/unknown/orphan orders = `0`",
    "positions = `0`",
    "M4-1c tiny position lifecycle actual pre-authorization",
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path), "exists": path.exists()}
    if path.exists():
        data = path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def load(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def git_head() -> str:
    return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()


def typed_records(payload: dict[str, Any]) -> list[dict[str, Any]]:
    records = payload.get("records", [])
    return [record for record in records if isinstance(record, dict)]


def probe(records: list[dict[str, Any]], name: str) -> dict[str, Any] | None:
    return next((record for record in records if record.get("probe") == name), None)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", type=Path, required=True)
    parser.add_argument("--m4-1a-evidence", type=Path, required=True)
    parser.add_argument("--auth-report", type=Path, required=True)
    parser.add_argument("--typed-readonly-report", type=Path, required=True)
    parser.add_argument("--no-send-preflight-report", type=Path, required=True)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m4/m4-1b-tiny-position-readonly-preflight-evidence.json"),
    )
    args = parser.parse_args()

    doc = DOC.read_text() if DOC.exists() else ""
    doc_markers = {marker: marker in doc for marker in REQUIRED_DOC_MARKERS}
    m4_1a = load(args.m4_1a_evidence)
    auth = load(args.auth_report)
    typed = load(args.typed_readonly_report)
    preflight = load(args.no_send_preflight_report)
    records = typed_records(typed)

    failed_probes = [
        record
        for record in records
        if record.get("probe") is not None and record.get("ok") is not True
    ]
    account = probe(records, "account_typed") or {}
    orders = probe(records, "account_orders_typed") or {}
    trades = probe(records, "account_trades_typed") or {}
    asset_params = probe(records, "asset_params_typed") or {}
    quote = probe(records, "last_quote_typed") or {}
    token_details = probe(records, "token_details_typed") or {}

    auth_ok = (
        auth.get("auth", {}).get("auth_http") == 200
        and auth.get("details", {}).get("details_http") == 200
        and auth.get("raw_secret_exported") is False
        and auth.get("raw_jwt_exported") is False
    )
    typed_ok = (
        typed.get("fixture_kind") == "finam-typed-readonly-redacted-v1"
        and bool(records)
        and not failed_probes
        and token_details.get("ok") is True
        and account.get("ok") is True
        and orders.get("ok") is True
        and trades.get("ok") is True
        and asset_params.get("ok") is True
        and quote.get("ok") is True
    )
    account_summary = account.get("summary", {})
    orders_summary = orders.get("summary", {})
    quote_summary = quote.get("summary", {})
    asset_params_summary = asset_params.get("summary", {})
    typed_broker_truth_ok = (
        account_summary.get("positions_count") == 0
        and orders_summary.get("active_orders_count") == 0
        and orders_summary.get("blocking_unknown_status_present") is False
    )
    typed_quote_ok = (
        quote_summary.get("bid_present") is True
        or quote_summary.get("ask_present") is True
        or quote_summary.get("last_present") is True
    ) and quote_summary.get("source_ts_present") is True
    asset_params_ok = (
        asset_params.get("ok") is True
        and (
            asset_params_summary.get("is_tradable") is True
            or asset_params_summary.get("tradeable") is True
            or asset_params_summary.get("long_initial_margin_present") is True
            or asset_params_summary.get("short_initial_margin_present") is True
        )
    )

    report = preflight.get("report", {})
    truth = preflight.get("pre_boundary_broker_truth", {})
    execution = preflight.get("execution_redacted", {})
    preflight_ok = (
        report.get("actual_send_allowed") is True
        and report.get("boundary_invocation_performed") is False
        and execution.get("place_attempted") is False
        and execution.get("cancel_attempted") is False
        and truth.get("active_orders_count") == 0
        and truth.get("unknown_active_orders_count") == 0
        and truth.get("orphan_active_orders_count") == 0
        and truth.get("positions_count") == 0
        and truth.get("broker_truth_clean") is True
        and report.get("runtime_live_attachment_allowed") is False
        and report.get("command_consumer_to_real_finam_allowed") is False
        and report.get("stop_sltp_bracket_replace_multileg_allowed") is False
    )
    m4_1a_ok = (
        m4_1a.get("evidence_ready_for_review") is True
        and m4_1a.get("review_policy", {}).get("m4_1a_authorizes_live_entry") is False
    )
    no_live_expansion = {
        "m4_1b_performs_broker_calls": False,
        "order_post_delete_called": False,
        "position_opened": False,
        "live_entry_authorized": False,
        "market_order_authorized": False,
        "continuous_runtime_live_enabled": False,
        "command_consumer_to_real_finam_enabled": False,
        "stop_sltp_bracket_replace_multileg_enabled": False,
    }
    evidence_ready = all(
        [
            args.source_archive.exists(),
            all(doc_markers.values()),
            auth_ok,
            typed_ok,
            typed_broker_truth_ok,
            typed_quote_ok,
            asset_params_ok,
            preflight_ok,
            m4_1a_ok,
            all(value is False for value in no_live_expansion.values()),
        ]
    )
    head = git_head()
    payload: dict[str, Any] = {
        "evidence_kind": "m4-1b-tiny-position-readonly-preflight-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "source_archive": artifact(args.source_archive),
        "artifact_manifest": {
            "doc": artifact(DOC),
            "m4_1a_evidence": artifact(args.m4_1a_evidence),
            "auth_report": artifact(args.auth_report),
            "typed_readonly_report": artifact(args.typed_readonly_report),
            "no_send_preflight_report": artifact(args.no_send_preflight_report),
        },
        "doc_markers": doc_markers,
        "auth_summary": {
            "auth_http": auth.get("auth", {}).get("auth_http"),
            "details_http": auth.get("details", {}).get("details_http"),
            "raw_secret_exported": auth.get("raw_secret_exported"),
            "raw_jwt_exported": auth.get("raw_jwt_exported"),
        },
        "typed_readonly_summary": {
            "records_count": len(records),
            "failed_probe_count": len(failed_probes),
            "account": account_summary,
            "orders": orders_summary,
            "trades": trades.get("summary", {}),
            "asset_params": asset_params_summary,
            "quote": quote_summary,
        },
        "no_send_preflight_summary": {
            "report": report,
            "broker_truth": truth,
            "execution_redacted": execution,
            "reference_quote": preflight.get("reference_quote_redacted", {}),
            "operator_scope": preflight.get("operator_scope", {}),
        },
        "no_live_expansion": no_live_expansion,
        "checks": {
            "doc_ok": all(doc_markers.values()),
            "auth_ok": auth_ok,
            "typed_ok": typed_ok,
            "typed_broker_truth_ok": typed_broker_truth_ok,
            "typed_quote_ok": typed_quote_ok,
            "asset_params_ok": asset_params_ok,
            "preflight_ok": preflight_ok,
            "m4_1a_ok": m4_1a_ok,
            "no_live_expansion": all(value is False for value in no_live_expansion.values()),
        },
        "next_stage_policy": {
            "recommended_next_stage": "M4-1c tiny position lifecycle actual pre-authorization",
            "m4_1c_requires_fresh_explicit_operator_approval": True,
            "m4_1b_authorizes_live_entry": False,
        },
        "evidence_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(json.dumps(payload["checks"] | {"evidence_ready_for_review": evidence_ready}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
