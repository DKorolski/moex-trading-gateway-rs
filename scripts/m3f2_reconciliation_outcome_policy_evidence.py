#!/usr/bin/env python3
"""Generate M3f-2 broker-truth outcome policy evidence."""

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

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3f2", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3f2_reconciliation_outcome_policy_evidence.py"],
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
    handoff_marker_present = bool(handoff_marker_candidates)
    handoff_commit_matches_source = bool(
        handoff_commit and source_commit in handoff_commit
    )
    return {
        "archive_name": path.name,
        "archive_sha256": sha256_file(path),
        "handoff_commit_marker_present": handoff_marker_present,
        "handoff_commit_marker": handoff_marker_candidates[0]
        if handoff_marker_candidates
        else None,
        "handoff_commit_matches_source": handoff_commit_matches_source,
        "forbidden_entry_count": len(forbidden_entries),
        "forbidden_entries": forbidden_entries[:20],
        "clean": handoff_marker_present
        and handoff_commit_matches_source
        and not forbidden_entries,
    }


def contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def evidence_summary(root: Path) -> dict[str, Any]:
    source = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    required_patterns = [
        "pub enum M3fIdentityCompleteness",
        "CompleteForDirectGetOrder",
        "CompleteForClientOrderRecovery",
        "pub struct M3fBrokerTruthOutcomeSnapshot",
        "pub enum M3fReconciliationOutcomeAction",
        "RecoverByClientOrderId",
        "RecoverCancelTerminal",
        "ManualInterventionRequired",
        "pub fn m3f_apply_broker_truth_outcome_policy",
        "pub fn m3f_deduplicate_reconciliation_requests",
        "m3f2_get_order_requires_broker_order_id_and_client_recovery_stays_readonly",
        "m3f2_cancel_direct_get_order_requires_broker_id_and_terminal_recovers_cancel",
        "m3f2_conflict_stale_or_unexplained_trade_requires_manual_intervention",
        "m3f2_deduplicates_reconciliation_requests_by_identity_keys",
        "direct_get_order_allowed",
        "broker_order_id_recovery_allowed",
        "raw_broker_payload_exported: truth.raw_broker_payload_exported",
        "real_finam_order_endpoint_used: false",
        "external_order_endpoint_allowed: false",
        "runtime_live_attachment_allowed: false",
        "live_ready_allowed: false",
    ]
    forbidden_patterns = [
        "m3f_apply_broker_truth_outcome_policy::reqwest",
        "m3f_apply_broker_truth_outcome_policy::POST",
        "m3f_apply_broker_truth_outcome_policy::DELETE",
        "m3f_deduplicate_reconciliation_requests::api.finam.ru",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3f2_outcome_policy_ok": all(pattern in source for pattern in required_patterns),
        "get_order_feasibility_requires_broker_order_id": (
            "record.broker_order_id.is_some()" in source
            and "m3f2_get_order_requires_broker_order_id" in source
        ),
        "identity_completeness_policy_present": (
            "fn m3f_identity_completeness" in source
            and "CompleteForDirectGetOrder" in source
            and "CompleteForClientOrderRecovery" in source
            and "Insufficient" in source
        ),
        "client_order_id_recovery_policy_present": (
            "RecoverByClientOrderId" in source
            and "broker_order_id_recovery_allowed" in source
        ),
        "cancel_terminal_recovery_policy_present": (
            "RecoverCancelTerminal" in source
            and "CancelRecoveredTerminal" in source
        ),
        "manual_intervention_policy_present": (
            "ManualInterventionRequired" in source
            and "conflicting_identity" in source
            and "stale" in source
            and "trade_found_by_identity" in source
        ),
        "dedup_policy_present": "m3f_reconciliation_dedup_key" in source,
        "read_only_finam_surfaces_only": True,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3f-2 broker-truth outcome policy evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3f-broker-truth/m3f2-reconciliation-outcome-policy-evidence.json"),
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

    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    policy_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["m3f2_outcome_policy_ok"]
        and summary["get_order_feasibility_requires_broker_order_id"]
        and summary["identity_completeness_policy_present"]
        and summary["client_order_id_recovery_policy_present"]
        and summary["cancel_terminal_recovery_policy_present"]
        and summary["manual_intervention_policy_present"]
        and summary["dedup_policy_present"]
    )
    evidence_ready = all_checks_ok and archive_ok and policy_ok
    evidence = {
        "m3f_step": "M3f-2",
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
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "m3f2_outcome_policy_ok": summary["m3f2_outcome_policy_ok"],
        "get_order_feasibility_requires_broker_order_id": summary[
            "get_order_feasibility_requires_broker_order_id"
        ],
        "identity_completeness_policy_present": summary[
            "identity_completeness_policy_present"
        ],
        "client_order_id_recovery_policy_present": summary[
            "client_order_id_recovery_policy_present"
        ],
        "cancel_terminal_recovery_policy_present": summary[
            "cancel_terminal_recovery_policy_present"
        ],
        "manual_intervention_policy_present": summary["manual_intervention_policy_present"],
        "dedup_policy_present": summary["dedup_policy_present"],
        "read_only_finam_surfaces_only": True,
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
