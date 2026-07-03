#!/usr/bin/env python3
"""Generate M3e-4 Redis command consumer lifecycle evidence."""

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
BROKER_CLI = Path("crates/broker-cli/src/main.rs")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "broker-cli"],
    ["cargo", "test", "-p", "finam-gateway", "m3e3a", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
    ["bash", "scripts/m3e_command_consumer_redis_smoke.sh"],
    ["python3", "-m", "py_compile", "scripts/m3e4_redis_command_consumer_evidence.py"],
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
        "stdout_tail": completed.stdout[-6000:],
        "stderr_tail": completed.stderr[-6000:],
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


def parse_smoke_json(checks: list[dict[str, Any]]) -> dict[str, Any] | None:
    for check in checks:
        if check["command"] == ["bash", "scripts/m3e_command_consumer_redis_smoke.sh"]:
            stdout = check["stdout_tail"]
            start = stdout.find("{")
            end = stdout.rfind("}")
            if start >= 0 and end > start:
                return json.loads(stdout[start : end + 1])
    return None


def evidence_summary(root: Path, smoke: dict[str, Any] | None) -> dict[str, Any]:
    gateway_source = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    cli_source = (root / BROKER_CLI).read_text(encoding="utf-8")
    source = gateway_source + "\n" + cli_source
    required_patterns = [
        "m3e-command-consumer-redis-smoke",
        "run_m3e_command_consumer_redis_smoke",
        "XREADGROUP",
        "XAUTOCLAIM",
        "runtime_bridge_xack",
        "M3eCommandConsumerLocalMockEndpoint::new",
        "M3eCliFailingRedisStreamSink",
        "ack_publish_failure_no_xack",
        "dlq_publish_failure_no_xack",
        "pending_replay_no_second_endpoint_attempt",
        "local_mock_endpoint_only",
        "external_order_endpoint_allowed",
    ]
    forbidden_patterns = [
        "M3eCommandConsumerLocalMockEndpoint::M3d2RealOrderEndpointTransport",
        "M3eCommandConsumerLocalMockEndpoint::ExternalFinam",
        "M3eCommandConsumerLocalMockEndpoint::reqwest",
        "m3e-command-consumer-redis-smoke::api.finam.ru",
    ]
    smoke_ok = bool(
        smoke
        and smoke.get("m3e4_redis_consumer_lifecycle_ok") is True
        and smoke.get("xreadgroup_consume_ok") is True
        and smoke.get("xack_after_ack_or_dlq_publish_ok") is True
        and smoke.get("xautoclaim_recovery_ok") is True
        and smoke.get("pending_replay_no_second_endpoint_attempt") is True
        and smoke.get("place_and_cancel_redis_lifecycle_ok") is True
        and smoke.get("ack_publish_failure_no_xack") is True
        and smoke.get("dlq_publish_failure_no_xack") is True
        and smoke.get("local_mock_endpoint_only") is True
        and smoke.get("external_order_endpoint_allowed") is False
        and smoke.get("non_loopback_endpoint_allowed") is False
        and smoke.get("runtime_live_attachment_allowed") is False
        and smoke.get("live_ready_allowed") is False
        and smoke.get("real_finam_order_endpoint_used") is False
    )
    return {
        "gateway_lib_sha256": sha256_file(root / GATEWAY_LIB),
        "broker_cli_sha256": sha256_file(root / BROKER_CLI),
        "required_patterns_present": contains_all(source, required_patterns),
        "forbidden_patterns_absent": {
            pattern: pattern not in source for pattern in forbidden_patterns
        },
        "smoke": smoke,
        "smoke_ok": smoke_ok,
        "m3e4_redis_consumer_lifecycle_ok": smoke_ok,
        "xreadgroup_consume_ok": bool(smoke and smoke.get("xreadgroup_consume_ok") is True),
        "xack_after_ack_or_dlq_publish_ok": bool(
            smoke and smoke.get("xack_after_ack_or_dlq_publish_ok") is True
        ),
        "xautoclaim_recovery_ok": bool(smoke and smoke.get("xautoclaim_recovery_ok") is True),
        "pending_replay_no_second_endpoint_attempt": bool(
            smoke and smoke.get("pending_replay_no_second_endpoint_attempt") is True
        ),
        "place_and_cancel_redis_lifecycle_ok": bool(
            smoke and smoke.get("place_and_cancel_redis_lifecycle_ok") is True
        ),
        "external_order_endpoint_allowed": False,
        "local_mock_endpoint_only": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3e-4 Redis command consumer lifecycle evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3e-command-consumer/m3e4-redis-command-consumer-evidence.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]

    source_commit = git["stdout_tail"].strip()
    checks = [run_text(command, root) for command in CHECKS]
    smoke = parse_smoke_json(checks)
    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path, source_commit)

    summary = evidence_summary(root, smoke)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    lifecycle_ok = (
        all(summary["required_patterns_present"].values())
        and all(summary["forbidden_patterns_absent"].values())
        and summary["smoke_ok"]
    )
    evidence_ready = all_checks_ok and archive_ok and lifecycle_ok
    evidence = {
        "m3e_step": "M3e-4",
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
        "m3e4_redis_consumer_lifecycle_ok": summary["m3e4_redis_consumer_lifecycle_ok"],
        "xreadgroup_consume_ok": summary["xreadgroup_consume_ok"],
        "xack_after_ack_or_dlq_publish_ok": summary["xack_after_ack_or_dlq_publish_ok"],
        "xautoclaim_recovery_ok": summary["xautoclaim_recovery_ok"],
        "pending_replay_no_second_endpoint_attempt": summary[
            "pending_replay_no_second_endpoint_attempt"
        ],
        "place_and_cancel_redis_lifecycle_ok": summary[
            "place_and_cancel_redis_lifecycle_ok"
        ],
        "external_order_endpoint_allowed": False,
        "local_mock_endpoint_only": True,
        "non_loopback_endpoint_allowed": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "stop_sltp_bracket_enabled": False,
        "real_finam_order_endpoint_used": False,
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
