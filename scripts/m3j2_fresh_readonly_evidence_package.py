#!/usr/bin/env python3
"""Generate M3j-2 fresh read-only FINAM evidence package metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from zipfile import ZipFile


GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
DOC = Path("docs/m3j2-fresh-readonly-evidence-package.md")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3j", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3j2_fresh_readonly_evidence_package.py"],
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_text(command: list[str], cwd: Path) -> dict[str, Any]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    return {
        "command": command,
        "exit_code": completed.returncode,
        "status": "Ok" if completed.returncode == 0 else "Failed",
        "stdout_tail": completed.stdout[-5000:],
        "stderr_tail": completed.stderr[-5000:],
    }


def clean_handoff_summary(path: Path, source_commit: str) -> dict[str, Any]:
    forbidden_markers = (
        ".env",
        ".git/",
        "target/",
        "tmp/",
        "reports/",
        "__MACOSX/",
        ".DS_Store",
    )
    forbidden_entries: list[str] = []
    handoff_marker_candidates: list[str] = []
    handoff_commit = None
    with ZipFile(path) as archive:
        names = archive.namelist()
        handoff_marker_candidates = [
            name for name in names if name.endswith("handoff-commit.txt")
        ]
        if handoff_marker_candidates:
            handoff_commit = (
                archive.read(handoff_marker_candidates[0]).decode("utf-8").strip()
            )
        for name in names:
            if name.endswith(".log") or any(marker in name for marker in forbidden_markers):
                forbidden_entries.append(name)
    return {
        "archive_name": path.name,
        "archive_sha256": sha256_file(path),
        "handoff_commit_marker_present": bool(handoff_marker_candidates),
        "handoff_commit_marker": handoff_marker_candidates[0]
        if handoff_marker_candidates
        else None,
        "handoff_commit_matches_source": bool(
            handoff_commit and source_commit in handoff_commit
        ),
        "forbidden_entry_count": len(forbidden_entries),
        "forbidden_entries": forbidden_entries[:20],
        "clean": bool(handoff_marker_candidates)
        and bool(handoff_commit and source_commit in handoff_commit)
        and not forbidden_entries,
    }


def contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def runtime_broker_truth_summary(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {"provided": False, "ok": False}
    payload = load_json(path)
    operator_report = payload.get("operator_report", {})
    evidence_matrix = operator_report.get("evidence_matrix", [])
    route_templates = {
        row.get("route_template")
        for row in evidence_matrix
        if isinstance(row, dict) and row.get("route_template")
    }
    statuses = [
        row.get("http_status")
        for row in evidence_matrix
        if isinstance(row, dict) and row.get("http_status") is not None
    ]
    required_sources = {"GetOrder", "OrdersSnapshot", "TradesSnapshot", "PositionSnapshot"}
    observed_sources = {
        row.get("source")
        for row in evidence_matrix
        if isinstance(row, dict) and row.get("source")
    }
    ok = (
        payload.get("fixture_kind") == "finam-real-readonly-contract-probe-evidence-v1"
        and payload.get("live_trading_enabled") is False
        and payload.get("order_endpoints_used") is False
        and payload.get("scope", {}).get("real_order_post_delete_enabled") is False
        and operator_report.get("blocking_reasons") == []
        and operator_report.get("actual_http_send_started_count", 999) <= 4
        and required_sources.issubset(observed_sources)
        and all(
            isinstance(route, str) and route.startswith("/v1/")
            for route in route_templates
        )
    )
    return {
        "provided": True,
        "path": str(path),
        "sha256": sha256_file(path),
        "ok": ok,
        "fixture_kind": payload.get("fixture_kind"),
        "live_trading_enabled": payload.get("live_trading_enabled"),
        "order_endpoints_used": payload.get("order_endpoints_used"),
        "blocking_reasons_count": len(operator_report.get("blocking_reasons", [])),
        "actual_http_send_started_count": operator_report.get(
            "actual_http_send_started_count"
        ),
        "actual_http_send_completed_count": operator_report.get(
            "actual_http_send_completed_count"
        ),
        "observed_sources": sorted(str(source) for source in observed_sources),
        "route_templates": sorted(route_templates),
        "http_statuses": statuses,
    }


def typed_readonly_summary(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {"provided": False, "ok": False}
    payload = load_json(path)
    records = payload.get("records", [])
    record_names = {
        record.get("name") or record.get("probe") or record.get("kind")
        for record in records
        if isinstance(record, dict)
    }
    encoded = json.dumps(payload, sort_keys=True)
    ok_records = {
        record.get("probe")
        for record in records
        if isinstance(record, dict) and record.get("ok") is True
    }
    ok = (
        payload.get("fixture_kind") == "finam-typed-readonly-redacted-v1"
        and "asset_params_typed" in ok_records
        and "asset_schedule_typed" in ok_records
        and "account_typed" in record_names
        and "account_orders_typed" in ok_records
        and "latest_trades_typed" in ok_records
        and "asset_typed" in ok_records
        and "raw_token" not in encoded
    )
    return {
        "provided": True,
        "path": str(path),
        "sha256": sha256_file(path),
        "ok": ok,
        "fixture_kind": payload.get("fixture_kind"),
        "record_names": sorted(str(name) for name in record_names if name),
        "ok_record_names": sorted(str(name) for name in ok_records if name),
    }


def evidence_summary(root: Path) -> dict[str, Any]:
    source = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    doc = (root / DOC).read_text(encoding="utf-8")
    required_patterns = [
        "pub struct M3j2FreshReadonlyEvidenceInput",
        "pub struct M3j2FreshReadonlyEvidenceReport",
        "pub fn m3j2_fresh_readonly_evidence_report",
        "m3j_step: \"M3j-2\"",
        "real_finam_readonly_evidence: true",
        "m3j2_fresh_readonly_evidence_ok",
        "evidence_fresh",
        "account_scope_exactly_one",
        "symbol_scope_exactly_one",
        "readonly_sources_complete",
        "no_unknown_active_orders_evidence",
        "no_orphan_active_orders_evidence",
        "flat_or_expected_position_evidence",
        "schedule_session_loaded",
        "instrument_params_validated",
        "broker_truth_snapshots_fresh",
        "redaction_ok",
        "live_micro_go: false",
        "m3j2_fresh_readonly_evidence_closes_readonly_slot_but_still_no_go",
        "m3j2_stale_unredacted_scope_or_live_boundary_cannot_close_readonly_slot",
    ]
    forbidden_patterns = [
        "M3j2FreshReadonlyEvidenceReport::reqwest",
        "M3j2FreshReadonlyEvidenceReport::POST",
        "M3j2FreshReadonlyEvidenceReport::DELETE",
        "api.finam.ru/order",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "doc_sha256": sha256_file(root / DOC),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3j2_fresh_readonly_evidence_ok": "m3j2_fresh_readonly_evidence_ok" in source,
        "redaction_ok": "redaction_ok" in source,
        "live_micro_go_false": "live_micro_go: false" in source,
        "no_live_boundary": (
            "live_ready_allowed: false" in source
            and "runtime_live_attachment_allowed: false" in source
            and "external_finam_post_delete_allowed: false" in source
            and "command_consumer_to_real_finam_transport_allowed: false" in source
            and "non_loopback_order_endpoint_allowed: false" in source
        ),
        "no_stop_sltp_bracket": "stop_sltp_bracket_replace_multileg_allowed: false"
        in source,
        "closure_documented": "M3j-2 closes the fresh read-only evidence slot" in doc,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3j-2 fresh read-only FINAM evidence package."
    )
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--runtime-readonly-evidence", type=Path)
    parser.add_argument("--typed-readonly-fixture", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j2-fresh-readonly-evidence-package.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]

    source_commit = git["stdout_tail"].strip()
    checks = [run_text(command, root) for command in CHECKS]

    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path, source_commit)

    runtime_summary = runtime_broker_truth_summary(
        (root / args.runtime_readonly_evidence).resolve()
        if args.runtime_readonly_evidence
        else None
    )
    typed_summary = typed_readonly_summary(
        (root / args.typed_readonly_fixture).resolve()
        if args.typed_readonly_fixture
        else None
    )
    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    policy_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["m3j2_fresh_readonly_evidence_ok"]
        and summary["redaction_ok"]
        and summary["live_micro_go_false"]
        and summary["no_live_boundary"]
        and summary["no_stop_sltp_bracket"]
        and summary["closure_documented"]
    )
    runtime_ok = runtime_summary["ok"] and typed_summary["ok"]
    evidence_ready = all_checks_ok and archive_ok and policy_ok and runtime_ok
    evidence = {
        "m3j_step": "M3j-2",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "source_archive_name": archive_summary["archive_name"]
        if archive_summary
        else None,
        "source_archive_sha256": archive_summary["archive_sha256"]
        if archive_summary
        else None,
        "source_archive_content_binding_verified": archive_summary["clean"]
        if archive_summary
        else False,
        "summary": summary,
        "runtime_readonly_evidence": runtime_summary,
        "typed_readonly_fixture": typed_summary,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "runtime_evidence_ok": runtime_ok,
        "m3j2_fresh_readonly_evidence_ok": summary["m3j2_fresh_readonly_evidence_ok"]
        and runtime_ok,
        "live_micro_go": False,
        "no_live_boundary": summary["no_live_boundary"],
        "no_stop_sltp_bracket": summary["no_stop_sltp_bracket"],
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "evidence_ready_for_review": evidence_ready,
    }

    output = (root / args.output).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
    output_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{output_sha256}  {output.name}\n"
    )
    print(json.dumps({"output": str(output), "sha256": output_sha256}, indent=2))
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
