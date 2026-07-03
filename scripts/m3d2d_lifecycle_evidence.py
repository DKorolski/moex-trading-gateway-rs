#!/usr/bin/env python3
"""Generate M3d-2d durable real-transport lifecycle evidence.

M3d-2d binds the already-reviewed M3d-2c reqwest transport into the durable
order-path lifecycle, but keeps execution local-mock only and disabled for any
external FINAM order endpoint calls.
"""

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


LIFECYCLE = Path("crates/finam-gateway/src/m3d2_real_transport_lifecycle.rs")
TRANSPORT = Path("crates/finam-gateway/src/m3d2_real_order_transport.rs")
GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
SCANNER = Path("scripts/forbidden_surface_scan.sh")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3d2d", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
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


def clean_handoff_summary(path: Path) -> dict[str, Any]:
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
    handoff_marker_present = False
    with ZipFile(path) as archive:
        names = archive.namelist()
        handoff_marker_present = "handoff-commit.txt" in names
        for name in names:
            if name.endswith(".log") or any(marker in name for marker in forbidden_markers):
                forbidden_entries.append(name)
    return {
        "archive_name": path.name,
        "archive_sha256": sha256_file(path),
        "handoff_commit_marker_present": handoff_marker_present,
        "forbidden_entry_count": len(forbidden_entries),
        "forbidden_entries": forbidden_entries[:20],
        "clean": handoff_marker_present and not forbidden_entries,
    }


def contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def direct_endpoint_gate_literals(root: Path) -> list[str]:
    locations: list[str] = []
    for path in (root / "crates").glob("**/*.rs"):
        source = path.read_text(encoding="utf-8")
        for line_no, line in enumerate(source.splitlines(), start=1):
            compact = " ".join(line.split())
            if "EndpointGateApproved { _private: ()" in compact:
                locations.append(f"{path.relative_to(root)}:{line_no}:{compact}")
    return locations


def source_slice(source: str, start: str, end: str) -> str:
    try:
        tail = source.split(start, 1)[1]
    except IndexError:
        return ""
    marker = tail.find(end)
    return tail if marker == -1 else tail[:marker]


def lifecycle_summary(root: Path) -> dict[str, Any]:
    lifecycle = (root / LIFECYCLE).read_text(encoding="utf-8")
    transport = (root / TRANSPORT).read_text(encoding="utf-8")
    gateway_lib = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    scanner = (root / SCANNER).read_text(encoding="utf-8")

    place_fn = source_slice(
        lifecycle,
        "pub async fn m3d2d_place_via_real_transport",
        "pub async fn m3d2d_cancel_via_real_transport",
    )
    cancel_fn = source_slice(
        lifecycle,
        "pub async fn m3d2d_cancel_via_real_transport",
        "pub fn m3d2d_persist_place_begin_submit",
    )

    required_tests = [
        "m3d2d_place_lifecycle_persists_begin_submit_before_real_transport_send",
        "m3d2d_place_ambiguous_missing_id_schedules_reconciliation",
        "m3d2d_cancel_lifecycle_persists_request_cancel_and_reconciles_conflict",
        "m3d2d_crash_windows_recover_conservatively_without_blind_retry",
    ]
    required_lifecycle_patterns = [
        "pub async fn m3d2d_place_via_real_transport",
        "pub async fn m3d2d_cancel_via_real_transport",
        "pub fn m3d2d_persist_place_begin_submit",
        "pub fn m3d2d_persist_cancel_request",
        "OrderPathEvent::BeginSubmit",
        "OrderPathEvent::RequestCancel",
        "OrderPathEvent::SubmitAcceptedWithoutBrokerOrderId",
        "OrderPathEvent::CancelAccepted",
        "OrderPathEvent::RequireManualIntervention",
        "M3d2dAckCandidateDiagnostic",
        "raw_client_order_id_exported: false",
        "raw_broker_order_id_exported: false",
        "raw_account_id_exported: false",
        "raw_token_exported: false",
        "raw_path_exported: false",
        "raw_body_exported: false",
        "SqliteOrderPathStore",
        "SqliteOrderPathReadStore",
        "recover_after_restart",
        "run_mock_server_once",
        "transport(base_url)",
    ]

    place_ordering_ok = (
        "m3d2d_persist_place_begin_submit" in place_fn
        and ".place_order_execution" in place_fn
        and place_fn.find("m3d2d_persist_place_begin_submit")
        < place_fn.find(".place_order_execution")
    )
    cancel_ordering_ok = (
        "m3d2d_persist_cancel_request" in cancel_fn
        and ".cancel_order_execution" in cancel_fn
        and cancel_fn.find("m3d2d_persist_cancel_request")
        < cancel_fn.find(".cancel_order_execution")
    )

    exact_counts = {
        ".post(": transport.count(".post("),
        ".delete(": transport.count(".delete("),
        ".send(": transport.count(".send("),
    }
    direct_literals = direct_endpoint_gate_literals(root)

    return {
        "lifecycle_path": str(LIFECYCLE),
        "lifecycle_sha256": sha256_file(root / LIFECYCLE),
        "transport_path": str(TRANSPORT),
        "transport_sha256": sha256_file(root / TRANSPORT),
        "required_tests_present": contains_all(lifecycle, required_tests),
        "required_lifecycle_patterns_present": contains_all(
            lifecycle, required_lifecycle_patterns
        ),
        "durable_before_send_ordering": {
            "place_begin_submit_before_transport_send": place_ordering_ok,
            "cancel_request_before_transport_send": cancel_ordering_ok,
        },
        "local_mock_only_evidence": {
            "loopback_mock_server_present": "TcpListener::bind(\"127.0.0.1:0\")"
            in lifecycle,
            "tests_pass_dynamic_mock_base_url": "transport(base_url)" in lifecycle,
            "tests_do_not_use_default_external_base_url": (
                "M3d2RealOrderEndpointTransportConfig::default()" in lifecycle
                and "rest_base_url: base_url" in lifecycle
            ),
        },
        "exact_send_surface_counts": exact_counts,
        "exact_send_surface_counts_ok": exact_counts
        == {".post(": 1, ".delete(": 1, ".send(": 1},
        "direct_endpoint_gate_literal_locations": direct_literals,
        "direct_endpoint_gate_literal_count": len(direct_literals),
        "scanner_direct_gate_literal_guard_present": (
            "direct EndpointGateApproved literal construction is forbidden" in scanner
        ),
        "production_gate_boundary": {
            "implementation_review_const_false": (
                "const REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED: bool = false;"
                in gateway_lib
            ),
            "endpoint_gate_test_constructor_cfg_test_only": (
                "#[cfg(test)]\nimpl EndpointGateApproved" in gateway_lib
                and "m3d2c_test_only_for_loopback_transport" in gateway_lib
            ),
            "default_real_order_endpoint_enabled_false": (
                "real_order_endpoint_enabled: false" in gateway_lib
            ),
            "default_command_consumer_enabled_false": (
                "command_consumer_enabled: false" in gateway_lib
            ),
        },
        "redacted_ack_candidate_only": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-2d durable lifecycle evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2d-lifecycle-evidence.json"),
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
        archive_summary = clean_handoff_summary(archive_path)

    lifecycle = lifecycle_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    lifecycle_ok = (
        all(lifecycle["required_tests_present"].values())
        and all(lifecycle["required_lifecycle_patterns_present"].values())
        and all(lifecycle["durable_before_send_ordering"].values())
        and all(lifecycle["local_mock_only_evidence"].values())
        and lifecycle["exact_send_surface_counts_ok"]
        and lifecycle["direct_endpoint_gate_literal_count"] == 0
        and lifecycle["scanner_direct_gate_literal_guard_present"]
        and all(lifecycle["production_gate_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and lifecycle_ok

    evidence = {
        "m3d_step": "M3d-2d",
        "durable_real_transport_lifecycle_local_mock_only": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "lifecycle": lifecycle,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "lifecycle_ok": lifecycle_ok,
        "trading_boundary": {
            "real_transport_source_used": True,
            "local_mock_only": True,
            "real_broker_calls_allowed_by_default": False,
            "endpoint_calls_allowed": False,
            "real_order_endpoint_enabled": False,
            "endpoint_gate_constructible_in_production": False,
            "command_consumer_enabled": False,
            "strategy_runtime_connected": False,
            "live_ready_allowed": False,
            "runtime_live_attachment_allowed": False,
            "stop_sltp_bracket_enabled": False,
            "blind_retry_allowed": False,
        },
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
