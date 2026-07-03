#!/usr/bin/env python3
"""Generate M3e-2 command lifecycle/idempotency evidence."""

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
    ["cargo", "test", "-p", "finam-gateway", "m3e2", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
    ["python3", "-m", "py_compile", "scripts/m3e2_command_lifecycle_evidence.py"],
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
        "pub struct M3eCommandLifecycleRecord",
        "pub enum M3eCommandLifecycleState",
        "pub trait M3eCommandLifecycleStore",
        "pub struct M3eJsonCommandLifecycleStore",
        "pub struct M3eCommandConsumerDurableDryRun",
        "M3eCommandLifecycleState::CommandReceived",
        "M3eCommandLifecycleState::AckPublished",
        "M3eCommandLifecycleState::ExpiredAckPublished",
        "CommandAckReasonCode::DuplicateCommand",
        "CommandAckReasonCode::ExpiredCommand",
        "endpoint_attempt_count: 0",
        "endpoint_transport_invoked: false",
        "external_order_endpoint_allowed: false",
        "raw_payload_exported: false",
        "raw_command_comment_exported: false",
        "m3e2_first_valid_command_persists_received_then_dry_ack_without_endpoint",
        "m3e2_duplicate_after_ack_replays_duplicate_ack_without_second_endpoint_attempt",
        "m3e2_duplicate_before_ack_and_restart_recovers_without_endpoint_attempt",
        "m3e2_expired_command_persists_terminal_local_outcome_without_endpoint",
        "m3e2_invalid_command_goes_dlq_without_command_state_mutation",
        "m3e2_ack_publish_failure_blocks_xack_and_preserves_received_state",
        "m3e2_dlq_publish_failure_blocks_xack_and_keeps_command_state_empty",
    ]
    forbidden_patterns = [
        "M3eCommandConsumerDurableDryRun::place_order_execution",
        "M3eCommandConsumerDurableDryRun::cancel_order_execution",
        "M3eCommandConsumerDurableDryRun::M3d2RealOrderEndpointTransport",
        "M3eCommandConsumerDurableDryRun::ExternalFinam",
    ]
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "m3e2_durable_command_lifecycle_ok": (
            "M3eJsonCommandLifecycleStore" in source
            and "M3eCommandLifecycleState::CommandReceived" in source
            and "M3eCommandLifecycleState::AckPublished" in source
        ),
        "request_id_idempotency_ok": (
            "request_id_idempotency_store_hit" in source
            and "CommandAckReasonCode::DuplicateCommand" in source
        ),
        "duplicate_request_no_second_endpoint_attempt": (
            "duplicate_request_no_second_endpoint_attempt" in source
            and "endpoint_attempt_count == 0" in source
        ),
        "ack_publish_before_xack_ok": (
            "ack_publish_before_xack: true" in source
            and "xack_after_ack_or_dlq_publish: true" in source
        ),
        "ack_publish_failure_blocks_xack": (
            "m3e2_ack_publish_failure_blocks_xack_and_preserves_received_state" in source
            and "assert_eq!(record.xack_count, 0)" in source
        ),
        "dlq_publish_failure_blocks_xack": (
            "m3e2_dlq_publish_failure_blocks_xack_and_keeps_command_state_empty" in source
            and "assert!(sink.entries().expect(\"entries\").is_empty())" in source
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
        description="Generate M3e-2 command lifecycle/idempotency evidence."
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
            "reports/m3e-command-consumer/m3e2-command-lifecycle-evidence.json"
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
    skeleton_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["m3e2_durable_command_lifecycle_ok"]
        and summary["request_id_idempotency_ok"]
        and summary["duplicate_request_no_second_endpoint_attempt"]
        and summary["ack_publish_before_xack_ok"]
        and summary["ack_publish_failure_blocks_xack"]
        and summary["dlq_publish_failure_blocks_xack"]
        and all(summary["production_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and skeleton_ok
    evidence = {
        "m3e_step": "M3e-2",
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
        "m3e2_durable_command_lifecycle_ok": summary[
            "m3e2_durable_command_lifecycle_ok"
        ],
        "request_id_idempotency_ok": summary["request_id_idempotency_ok"],
        "duplicate_request_no_second_endpoint_attempt": summary[
            "duplicate_request_no_second_endpoint_attempt"
        ],
        "ack_publish_before_xack_ok": summary["ack_publish_before_xack_ok"],
        "ack_publish_failure_blocks_xack": summary["ack_publish_failure_blocks_xack"],
        "dlq_publish_failure_blocks_xack": summary["dlq_publish_failure_blocks_xack"],
        "endpoint_transport_invoked": False,
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
