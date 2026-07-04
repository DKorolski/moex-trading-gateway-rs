#!/usr/bin/env python3
"""Generate M3j-6 tiny live-order-path diff design evidence."""

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
DOC = Path("docs/m3j6-tiny-live-order-path-diff-design.md")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3j", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["python3", "-m", "py_compile", "scripts/m3j6_tiny_live_order_path_diff_design.py"],
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
        "pub struct M3j6TinyLiveOrderPathDiffDesignInput",
        "pub struct M3j6TinyLiveOrderPathDiffDesignReport",
        "pub fn m3j6_tiny_live_order_path_diff_design_report",
        "m3j_step: \"M3j-6\"",
        "tiny_live_order_path_diff_design_ok",
        "default_disabled_ok",
        "all_gates_required",
        "kill_switch_coverage_ok",
        "first_micro_scope_ok",
        "rollback_and_reconciliation_ok",
        "scanner_and_redaction_ok",
        "real_send_reachable_by_default: false",
        "live_micro_go: false",
        "m3j6_tiny_live_order_path_diff_design_is_ok_but_still_no_live",
        "m3j6_reachable_send_missing_kill_switch_or_open_boundary_blocks_design",
    ]
    forbidden_patterns = [
        "M3j6TinyLiveOrderPathDiffDesignReport::reqwest",
        "M3j6TinyLiveOrderPathDiffDesignReport::POST",
        "M3j6TinyLiveOrderPathDiffDesignReport::DELETE",
        "api.finam.ru/order",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "doc_sha256": sha256_file(root / DOC),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "tiny_live_order_path_diff_design_present": "m3j6_tiny_live_order_path_diff_design_report"
        in source,
        "real_send_unreachable_by_default": "real_send_reachable_by_default: false"
        in source,
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
        "design_documented": "M3j-6 is a design/review package" in doc,
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3j-6 tiny live-order-path diff design evidence."
    )
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--m3j5-evidence", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j6-tiny-live-order-path-diff-design.json"),
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

    m3j5_evidence = artifact_summary(
        (root / args.m3j5_evidence).resolve() if args.m3j5_evidence else None
    )
    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    artifact_manifest_ok = m3j5_evidence["provided"]
    policy_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["tiny_live_order_path_diff_design_present"]
        and summary["real_send_unreachable_by_default"]
        and summary["live_micro_go_false"]
        and summary["no_live_boundary"]
        and summary["no_stop_sltp_bracket"]
        and summary["design_documented"]
    )
    evidence_ready = all_checks_ok and archive_ok and artifact_manifest_ok and policy_ok
    evidence = {
        "m3j_step": "M3j-6",
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
        "artifact_manifest": {"m3j5_evidence": m3j5_evidence},
        "design_only": True,
        "tiny_live_order_path_diff_design_ok": summary[
            "tiny_live_order_path_diff_design_present"
        ],
        "real_send_reachable_by_default": False,
        "live_order_path_implemented": False,
        "live_micro_go": False,
        "no_live_boundary": summary["no_live_boundary"],
        "no_stop_sltp_bracket": summary["no_stop_sltp_bracket"],
        "real_finam_order_endpoint_used": False,
        "external_order_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "artifact_manifest_ok": artifact_manifest_ok,
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
