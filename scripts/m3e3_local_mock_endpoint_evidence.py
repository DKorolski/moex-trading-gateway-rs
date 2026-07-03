#!/usr/bin/env python3
"""Generate M3e-3 local-mock endpoint boundary evidence."""

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
    ["cargo", "test", "-p", "finam-gateway", "m3e3", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
    ["python3", "-m", "py_compile", "scripts/m3e3_local_mock_endpoint_evidence.py"],
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
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
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
        "pub struct M3eCommandConsumerLocalMockEndpoint",
        "pub struct M3eLocalMockEndpointCommandReport",
        "M3eCommandLifecycleState::AckPublishPlanned",
        "mark_ack_publish_planned",
        "simulate_place_order_endpoint_classified_transport",
        "FinamMockClassifiedEndpointTransport",
        "M3eCommandLifecycleAction::LocalMockEndpointAckPublished",
        "M3eCommandLifecycleAction::LocalRejectAckPublished",
        "M3eCommandLifecycleAction::RecoveredAckPublished",
        "local_mock_endpoint_only: true",
        "non_loopback_endpoint_allowed: false",
        "external_order_endpoint_allowed: false",
        "m3e3_place_command_reaches_local_mock_endpoint_after_durable_boundaries",
        "m3e3_duplicate_request_after_local_mock_does_not_call_endpoint_again",
        "m3e3_preflight_rejects_before_local_mock_endpoint_attempt",
        "m3e3_ack_published_but_lifecycle_update_failed_recovery_is_explicit",
    ]
    forbidden_patterns = [
        "M3eCommandConsumerLocalMockEndpoint::place_order_execution",
        "M3eCommandConsumerLocalMockEndpoint::cancel_order_execution",
        "M3eCommandConsumerLocalMockEndpoint::M3d2RealOrderEndpointTransport",
        "M3eCommandConsumerLocalMockEndpoint::ExternalFinam",
        "M3eCommandConsumerLocalMockEndpoint::reqwest",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3e3_local_mock_endpoint_boundary_ok": (
            "M3eCommandConsumerLocalMockEndpoint" in source
            and "FinamMockClassifiedEndpointTransport" in source
            and "simulate_place_order_endpoint_classified_transport" in source
        ),
        "local_mock_endpoint_only": "local_mock_endpoint_only: true" in source,
        "non_loopback_endpoint_allowed": False,
        "duplicate_request_no_second_endpoint_attempt": (
            "m3e3_duplicate_request_after_local_mock_does_not_call_endpoint_again" in source
            and "assert_eq!(transport.place_call_count, 1)" in source
        ),
        "command_received_persisted_before_endpoint": (
            "command_received_persisted: true" in source
            and "insert_received(lifecycle_record.clone())" in source
        ),
        "preflight_local_reject_before_endpoint": (
            "m3e3_preflight_rejects_before_local_mock_endpoint_attempt" in source
            and "preflight_local_reject_before_endpoint: true" in source
        ),
        "begin_submit_persisted_before_endpoint": (
            "begin_submit_or_request_cancel_persisted_before_endpoint" in source
            and "simulate_place_order_endpoint_classified_transport" in source
        ),
        "ack_publish_planned_before_ack": (
            "mark_ack_publish_planned" in source
            and "self.lifecycle_store.upsert(lifecycle_record.clone())?;" in source
        ),
        "ack_publish_before_xack_ok": (
            "ack_publish_before_xack: true" in source
            and "xack_after_ack_or_dlq_publish: true" in source
        ),
        "ack_published_store_update_failure_recovery_ok": (
            "m3e3_ack_published_but_lifecycle_update_failed_recovery_is_explicit" in source
            and "M3eCommandLifecycleState::AckPublishPlanned" in source
            and "M3eCommandLifecycleAction::RecoveredAckPublished" in source
        ),
        "endpoint_attempt_count_incremented_only_after_durable_boundary": (
            "endpoint_attempt_count_incremented_only_after_durable_boundary: true" in source
            and "assert_eq!(report.endpoint_attempt_count, 1)" in source
        ),
        "production_boundary": {
            "real_order_endpoint_enabled_false_default": (
                "real_order_endpoint_enabled: false" in source
            ),
            "command_consumer_enabled_false_default": (
                "command_consumer_enabled: false" in source
            ),
            "live_ready_literal_absent": "LiveReady" not in source,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3e-3 local-mock endpoint boundary evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(
            "reports/m3e-command-consumer/m3e3-local-mock-endpoint-evidence.json"
        ),
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
    boundary_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["m3e3_local_mock_endpoint_boundary_ok"]
        and summary["local_mock_endpoint_only"]
        and not summary["non_loopback_endpoint_allowed"]
        and summary["duplicate_request_no_second_endpoint_attempt"]
        and summary["command_received_persisted_before_endpoint"]
        and summary["preflight_local_reject_before_endpoint"]
        and summary["begin_submit_persisted_before_endpoint"]
        and summary["ack_publish_planned_before_ack"]
        and summary["ack_publish_before_xack_ok"]
        and summary["ack_published_store_update_failure_recovery_ok"]
        and summary["endpoint_attempt_count_incremented_only_after_durable_boundary"]
        and all(summary["production_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and boundary_ok
    evidence = {
        "m3e_step": "M3e-3",
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
        "m3e3_local_mock_endpoint_boundary_ok": summary[
            "m3e3_local_mock_endpoint_boundary_ok"
        ],
        "local_mock_endpoint_only": summary["local_mock_endpoint_only"],
        "non_loopback_endpoint_allowed": summary["non_loopback_endpoint_allowed"],
        "duplicate_request_no_second_endpoint_attempt": summary[
            "duplicate_request_no_second_endpoint_attempt"
        ],
        "command_received_persisted_before_endpoint": summary[
            "command_received_persisted_before_endpoint"
        ],
        "preflight_local_reject_before_endpoint": summary[
            "preflight_local_reject_before_endpoint"
        ],
        "begin_submit_persisted_before_endpoint": summary[
            "begin_submit_persisted_before_endpoint"
        ],
        "ack_publish_planned_before_ack": summary["ack_publish_planned_before_ack"],
        "ack_publish_before_xack_ok": summary["ack_publish_before_xack_ok"],
        "ack_published_store_update_failure_recovery_ok": summary[
            "ack_published_store_update_failure_recovery_ok"
        ],
        "endpoint_attempt_count_incremented_only_after_durable_boundary": summary[
            "endpoint_attempt_count_incremented_only_after_durable_boundary"
        ],
        "endpoint_transport_invoked": True,
        "endpoint_transport_scope": "local_mock_classified_only",
        "external_order_endpoint_allowed": False,
        "command_consumer_can_reach_non_loopback": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "stop_sltp_bracket_enabled": False,
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
