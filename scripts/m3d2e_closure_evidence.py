#!/usr/bin/env python3
"""Generate M3d-2e protected endpoint closure evidence."""

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
BROKER_FINAM_ORDER_REQUEST = Path("crates/broker-finam/src/order_request.rs")
SCANNER = Path("scripts/forbidden_surface_scan.sh")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3d2d", "--", "--nocapture"],
    ["cargo", "test", "-p", "finam-gateway", "m3d2e", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
    ["python3", "-m", "py_compile", "scripts/m3d2e_closure_evidence.py"],
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


def real_transport_constructor_locations(root: Path) -> list[str]:
    allowed_files = {
        Path("crates/finam-gateway/src/m3d2_real_order_transport.rs"),
        Path("crates/finam-gateway/src/m3d2_real_transport_lifecycle.rs"),
    }
    locations: list[str] = []
    for path in (root / "crates").glob("**/*.rs"):
        relative = path.relative_to(root)
        source = path.read_text(encoding="utf-8")
        test_module_idx = source.find("#[cfg(test)]\nmod tests")
        for token in (
            "M3d2RealOrderEndpointTransport::try_new",
            "M3d2RealOrderEndpointTransportConfig::default()",
        ):
            start = 0
            while True:
                idx = source.find(token, start)
                if idx == -1:
                    break
                in_test_module = test_module_idx != -1 and idx > test_module_idx
                if relative not in allowed_files or not in_test_module:
                    locations.append(
                        f"{relative}:{source[:idx].count(chr(10)) + 1}:{token}"
                    )
                start = idx + len(token)
    return locations


def closure_summary(root: Path) -> dict[str, Any]:
    lifecycle = (root / LIFECYCLE).read_text(encoding="utf-8")
    transport = (root / TRANSPORT).read_text(encoding="utf-8")
    gateway_lib = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    broker_finam = (root / BROKER_FINAM_ORDER_REQUEST).read_text(encoding="utf-8")
    scanner = (root / SCANNER).read_text(encoding="utf-8")

    lifecycle_matrix_patterns = [
        "m3d2e_lifecycle_matrix_maps_all_required_transport_outcomes",
        "place_accepted_with_broker_id",
        "place_accepted_without_broker_id",
        "place_validation_reject_400",
        "place_unauthorized_401",
        "place_rate_limited_429",
        "place_maintenance_500",
        "place_service_interval_503",
        "place_malformed_2xx",
        "place_body_read_failed",
        "place_timeout_504",
        "place_send_error_after_possible_send",
        "cancel_accepted_200_without_id",
        "cancel_accepted_202_without_id",
        "cancel_accepted_204_no_body",
        "cancel_accepted_same_broker_id",
        "cancel_accepted_different_broker_id",
        "cancel_404_requires_reconciliation",
        "cancel_409_requires_reconciliation",
        "cancel_410_requires_reconciliation",
        "cancel_unauthorized_401",
        "cancel_rate_limited_429",
        "cancel_maintenance_503",
        "cancel_timeout_504",
        "cancel_decode_failure",
        "cancel_body_read_failed",
        "audit_last_event",
        "assert_report_matches",
    ]

    transport_firewall_patterns = [
        "pub enum M3d2ExternalOrderEndpointMode",
        "LocalMockOnly",
        "ExternalFinamDisabled",
        "FutureExternalFinamRequiresLiveGate",
        "pub enum M3d2OrderEndpointBaseUrlKind",
        "ExternalOrderEndpointBlocked",
        "classify_order_endpoint_base_url",
        "external_endpoint_firewall_allows",
        "external_finam_order_calls_allowed_by_default",
    ]

    direct_literals = direct_endpoint_gate_literals(root)
    constructor_locations = real_transport_constructor_locations(root)
    exact_counts = {
        ".post(": transport.count(".post("),
        ".delete(": transport.count(".delete("),
        ".send(": transport.count(".send("),
    }
    return {
        "lifecycle_path": str(LIFECYCLE),
        "lifecycle_sha256": sha256_file(root / LIFECYCLE),
        "transport_path": str(TRANSPORT),
        "transport_sha256": sha256_file(root / TRANSPORT),
        "broker_finam_order_request_sha256": sha256_file(root / BROKER_FINAM_ORDER_REQUEST),
        "lifecycle_matrix_patterns_present": contains_all(
            lifecycle, lifecycle_matrix_patterns
        ),
        "firewall_patterns_present": contains_all(transport, transport_firewall_patterns)
        | {
            "m3d2e_command_consumer_cannot_route_to_real_transport_by_default": (
                "m3d2e_command_consumer_cannot_route_to_real_transport_by_default"
                in lifecycle
            )
        },
        "cancel_204_no_body_acceptance_present": (
            "context == FinamOrderEndpointContext::Cancel" in broker_finam
            and "body.trim().is_empty() || *status == 204" in broker_finam
        ),
        "exact_send_surface_counts": exact_counts,
        "exact_send_surface_counts_ok": exact_counts
        == {".post(": 1, ".delete(": 1, ".send(": 1},
        "direct_endpoint_gate_literal_locations": direct_literals,
        "direct_endpoint_gate_literal_count": len(direct_literals),
        "forbidden_real_transport_constructor_locations": constructor_locations,
        "forbidden_real_transport_constructor_location_count": len(constructor_locations),
        "scanner_firewall_guards_present": (
            "real order transport construction/default " in scanner
            and "M3d2RealOrderEndpointTransport::try_new" in scanner
            and "M3d2RealOrderEndpointTransportConfig::default()" in scanner
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
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-2e protected endpoint closure evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2e-closure-evidence.json"),
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

    closure = closure_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    lifecycle_matrix_ok = (
        all(closure["lifecycle_matrix_patterns_present"].values())
        and closure["cancel_204_no_body_acceptance_present"]
    )
    external_endpoint_enablement_firewall_ok = (
        all(closure["firewall_patterns_present"].values())
        and closure["scanner_firewall_guards_present"]
        and closure["forbidden_real_transport_constructor_location_count"] == 0
        and closure["exact_send_surface_counts_ok"]
        and closure["direct_endpoint_gate_literal_count"] == 0
        and all(closure["production_gate_boundary"].values())
    )
    evidence_ready = (
        all_checks_ok
        and archive_ok
        and lifecycle_matrix_ok
        and external_endpoint_enablement_firewall_ok
    )

    evidence = {
        "m3d_step": "M3d-2e",
        "m3d2_protected_endpoint_stage_closed": evidence_ready,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "closure": closure,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "lifecycle_matrix_ok": lifecycle_matrix_ok,
        "external_endpoint_enablement_firewall_ok": external_endpoint_enablement_firewall_ok,
        "command_consumer_enabled": False,
        "real_order_endpoint_enabled": False,
        "endpoint_gate_constructible_in_production": False,
        "external_finam_order_calls_allowed_by_default": False,
        "runtime_live_attachment_allowed": False,
        "stop_sltp_bracket_enabled": False,
        "trading_boundary": {
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
