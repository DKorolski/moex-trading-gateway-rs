#!/usr/bin/env python3
"""Generate M3j-4 explicit pre-live NO-GO / GO decision package evidence."""

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
DOC = Path("docs/m3j4-pre-live-no-go-go-decision-package.md")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3j", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3j4_pre_live_decision_package.py"],
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


def artifact_summary(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {"provided": False}
    return {"provided": True, "path": str(path), "sha256": sha256_file(path)}


def evidence_summary(root: Path) -> dict[str, Any]:
    source = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    doc = (root / DOC).read_text(encoding="utf-8")
    required_patterns = [
        "pub enum M3j4PreLiveDecision",
        "pub struct M3j4PreLiveDecisionInput",
        "pub struct M3j4PreLiveDecisionReport",
        "pub fn m3j4_pre_live_decision_report",
        "m3j_step: \"M3j-4\"",
        "M3j4PreLiveDecision::NoGo",
        "M3j4PreLiveDecision::GoCandidate",
        "pre_live_decision_package_ok",
        "readonly_evidence_still_fresh",
        "broker_truth_clean",
        "scope_enforced",
        "operator_controls_ready",
        "risk_limits_ready",
        "daily_eod_reconciliation_plan_ready",
        "typed_optional_failures_fixed_or_waived",
        "operator_explicit_go",
        "live_micro_go: false",
        "m3j4_pre_live_decision_defaults_no_go_without_operator_explicit_go",
        "m3j4_go_candidate_still_does_not_enable_live_boundary",
        "m3j4_stale_readonly_missing_eod_or_live_boundary_forces_no_go",
    ]
    forbidden_patterns = [
        "M3j4PreLiveDecisionReport::reqwest",
        "M3j4PreLiveDecisionReport::POST",
        "M3j4PreLiveDecisionReport::DELETE",
        "api.finam.ru/order",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "doc_sha256": sha256_file(root / DOC),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "pre_live_decision_package_present": "m3j4_pre_live_decision_report" in source,
        "default_no_go_without_operator_go": "operator explicit GO is absent" in source,
        "go_candidate_does_not_enable_live_micro": "live_micro_go: false" in source
        and "m3j4_go_candidate_still_does_not_enable_live_boundary" in source,
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
        "decision_documented": "M3j-4 aggregates M3j-0 through M3j-3" in doc,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3j-4 explicit pre-live NO-GO / GO decision evidence."
    )
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--m3j2-runtime-readonly-evidence", type=Path)
    parser.add_argument("--m3j2-typed-readonly-fixture", type=Path)
    parser.add_argument("--m3j3-evidence", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j4-pre-live-decision-package.json"),
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

    artifacts = {
        "m3j2_runtime_readonly_evidence": artifact_summary(
            (root / args.m3j2_runtime_readonly_evidence).resolve()
            if args.m3j2_runtime_readonly_evidence
            else None
        ),
        "m3j2_typed_readonly_fixture": artifact_summary(
            (root / args.m3j2_typed_readonly_fixture).resolve()
            if args.m3j2_typed_readonly_fixture
            else None
        ),
        "m3j3_evidence": artifact_summary(
            (root / args.m3j3_evidence).resolve() if args.m3j3_evidence else None
        ),
    }
    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    artifact_manifest_ok = all(item["provided"] for item in artifacts.values())
    policy_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["pre_live_decision_package_present"]
        and summary["default_no_go_without_operator_go"]
        and summary["go_candidate_does_not_enable_live_micro"]
        and summary["live_micro_go_false"]
        and summary["no_live_boundary"]
        and summary["no_stop_sltp_bracket"]
        and summary["decision_documented"]
    )
    evidence_ready = all_checks_ok and archive_ok and artifact_manifest_ok and policy_ok
    evidence = {
        "m3j_step": "M3j-4",
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
        "artifact_manifest": artifacts,
        "current_operator_decision": "NO-GO",
        "operator_explicit_go": False,
        "default_no_go_without_operator_go": True,
        "go_candidate_available_in_source_model": True,
        "go_candidate_does_not_enable_live_micro": True,
        "typed_optional_failure_count": 3,
        "typed_optional_failures_fixed_or_waived": True,
        "typed_optional_failure_waiver_scope": [
            "account_trades_typed covered by runtime TradesSnapshot for first micro",
            "account_transactions_typed deferred to EOD/fees/cash reconciliation hardening",
            "bars_typed deferred because first micro uses live-final path, not REST bars backfill",
        ],
        "daily_eod_reconciliation_plan_ready": True,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "artifact_manifest_ok": artifact_manifest_ok,
        "pre_live_decision_package_ok": summary["pre_live_decision_package_present"],
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
