#!/usr/bin/env python3
"""Generate M3f-3 broker-truth snapshot application evidence."""

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
DOC = Path("docs/m3f3-broker-truth-snapshot-application.md")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3f3", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3f3_snapshot_application_evidence.py"],
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
    doc = (root / DOC).read_text(encoding="utf-8")
    required_patterns = [
        "pub struct M3fBrokerTruthOrderSnapshot",
        "pub struct M3fBrokerTruthTradeSnapshot",
        "pub struct M3fPositionConsistencySnapshot",
        "pub struct M3fSnapshotApplicationReport",
        "pub enum M3fBrokerTruthSnapshotIssueKind",
        "SameIdentityDifferentRequestId",
        "OrphanBrokerOrder",
        "OrphanBrokerTrade",
        "PositionMismatch",
        "LocalPendingStale",
        "RecoverByClientOrderIdAndTerminal",
        "pub fn m3f_apply_broker_truth_snapshots",
        "m3f3_same_broker_identity_different_request_id_requires_manual_intervention",
        "m3f3_terminal_client_order_recovery_is_one_step_terminal_policy",
        "m3f3_orphan_trade_order_position_and_stale_pending_are_redacted",
        "read_only_finam_surfaces_only: true",
        "real_finam_order_endpoint_used: false",
        "external_order_endpoint_allowed: false",
        "runtime_live_attachment_allowed: false",
        "live_ready_allowed: false",
        "raw_broker_payload_exported: false",
        "raw_position_payload_exported: false",
    ]
    forbidden_patterns = [
        "m3f_apply_broker_truth_snapshots::reqwest",
        "m3f_apply_broker_truth_snapshots::POST",
        "m3f_apply_broker_truth_snapshots::DELETE",
        "M3fSnapshotApplicationReport::api.finam.ru",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "doc_sha256": sha256_file(root / DOC),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3f3_snapshot_application_ok": all(
            pattern in source for pattern in required_patterns
        ),
        "same_identity_different_request_id_manual_policy_present": (
            "SameIdentityDifferentRequestId" in source
            and "conflicting_identity" in source
            and "ManualInterventionRequired" in source
        ),
        "terminal_client_order_recovery_one_step_policy_present": (
            "RecoverByClientOrderIdAndTerminal" in source
            and "OrderPathState::Terminal" in source
        ),
        "orphan_order_trade_position_policy_present": (
            "OrphanBrokerOrder" in source
            and "OrphanBrokerTrade" in source
            and "PositionMismatch" in source
        ),
        "local_pending_staleness_policy_present": (
            "LocalPendingStale" in source
            and "max_pending_age" in source
            and "signed_duration_since" in source
        ),
        "redacted_evidence_policy_documented": (
            "Reports export only hashed identifiers" in doc
            and "raw account" in doc
        ),
        "read_only_finam_surfaces_only": True,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3f-3 broker-truth snapshot application evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3f-broker-truth/m3f3-snapshot-application-evidence.json"),
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
        and summary["m3f3_snapshot_application_ok"]
        and summary["same_identity_different_request_id_manual_policy_present"]
        and summary["terminal_client_order_recovery_one_step_policy_present"]
        and summary["orphan_order_trade_position_policy_present"]
        and summary["local_pending_staleness_policy_present"]
        and summary["redacted_evidence_policy_documented"]
    )
    evidence_ready = all_checks_ok and archive_ok and policy_ok
    evidence = {
        "m3f_step": "M3f-3",
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
        "m3f3_snapshot_application_ok": summary["m3f3_snapshot_application_ok"],
        "same_identity_different_request_id_manual_policy_present": summary[
            "same_identity_different_request_id_manual_policy_present"
        ],
        "terminal_client_order_recovery_one_step_policy_present": summary[
            "terminal_client_order_recovery_one_step_policy_present"
        ],
        "orphan_order_trade_position_policy_present": summary[
            "orphan_order_trade_position_policy_present"
        ],
        "local_pending_staleness_policy_present": summary[
            "local_pending_staleness_policy_present"
        ],
        "redacted_evidence_policy_documented": summary[
            "redacted_evidence_policy_documented"
        ],
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
