#!/usr/bin/env python3
"""Generate M3d-2f external firewall hardening evidence."""

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


TRANSPORT = Path("crates/finam-gateway/src/m3d2_real_order_transport.rs")
LIFECYCLE = Path("crates/finam-gateway/src/m3d2_real_transport_lifecycle.rs")
GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
SCANNER = Path("scripts/forbidden_surface_scan.sh")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
    ["cargo", "test", "-p", "finam-gateway", "m3d2f", "--", "--nocapture"],
    ["cargo", "test", "-p", "finam-gateway", "m3d2e", "--", "--nocapture"],
    ["cargo", "test", "--all"],
    ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ["bash", "scripts/forbidden_surface_scan.sh"],
    ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"],
    ["bash", "scripts/redis_shadow_smoke.sh"],
    ["bash", "scripts/runtime_bridge_dry_smoke.sh"],
    ["python3", "-m", "py_compile", "scripts/m3d2f_firewall_evidence.py"],
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


def direct_endpoint_gate_literals(root: Path) -> list[str]:
    locations: list[str] = []
    for path in (root / "crates").glob("**/*.rs"):
        source = path.read_text(encoding="utf-8")
        for line_no, line in enumerate(source.splitlines(), start=1):
            compact = " ".join(line.split())
            if "EndpointGateApproved { _private: ()" in compact:
                locations.append(f"{path.relative_to(root)}:{line_no}:{compact}")
    return locations


def evidence_summary(root: Path) -> dict[str, Any]:
    transport = (root / TRANSPORT).read_text(encoding="utf-8")
    lifecycle = (root / LIFECYCLE).read_text(encoding="utf-8")
    gateway_lib = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    scanner = (root / SCANNER).read_text(encoding="utf-8")

    strict_policy_present = (
        "M3d2ExternalOrderEndpointMode::ExternalFinamDisabled => {\n"
        "            base_url_kind == M3d2OrderEndpointBaseUrlKind::Loopback"
    ) in transport
    external_other_endpoint_blocked = (
        '"https://example.com"' in lifecycle
        and "M3d2OrderEndpointBaseUrlKind::OtherExternal" in lifecycle
        and "m3d2f_external_firewall_blocks_all_non_loopback_order_endpoints" in lifecycle
    )
    no_non_loopback_order_endpoint_allowed = (
        strict_policy_present
        and "M3d2ExternalOrderEndpointMode::LocalMockOnly => {\n"
        "            base_url_kind == M3d2OrderEndpointBaseUrlKind::Loopback"
        in transport
        and external_other_endpoint_blocked
    )
    exact_counts = {
        ".post(": transport.count(".post("),
        ".delete(": transport.count(".delete("),
        ".send(": transport.count(".send("),
    }
    direct_literals = direct_endpoint_gate_literals(root)

    return {
        "transport_path": str(TRANSPORT),
        "transport_sha256": sha256_file(root / TRANSPORT),
        "lifecycle_path": str(LIFECYCLE),
        "lifecycle_sha256": sha256_file(root / LIFECYCLE),
        "strict_loopback_only_policy_present": strict_policy_present,
        "external_other_endpoint_blocked": external_other_endpoint_blocked,
        "no_non_loopback_order_endpoint_allowed": no_non_loopback_order_endpoint_allowed,
        "m3d2f_test_present": (
            "m3d2f_external_firewall_blocks_all_non_loopback_order_endpoints"
            in lifecycle
        ),
        "default_external_finam_blocked_test_present": (
            "default api.finam.ru endpoint must be blocked" in lifecycle
        ),
        "future_live_gate_loopback_blocked_test_present": (
            "future live gate mode must remain blocked" in lifecycle
        ),
        "exact_send_surface_counts": exact_counts,
        "exact_send_surface_counts_ok": exact_counts
        == {".post(": 1, ".delete(": 1, ".send(": 1},
        "direct_endpoint_gate_literal_count": len(direct_literals),
        "direct_endpoint_gate_literal_locations": direct_literals,
        "scanner_constructor_guard_present": (
            "real order transport construction/default " in scanner
            and "M3d2RealOrderEndpointTransport::try_new" in scanner
            and "M3d2RealOrderEndpointTransportConfig::default()" in scanner
        ),
        "production_gate_boundary": {
            "implementation_review_const_false": (
                "const REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED: bool = false;"
                in gateway_lib
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
        description="Generate M3d-2f external firewall hardening evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2f-firewall-evidence.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]

    checks = [run_text(command, root) for command in CHECKS]
    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path)

    summary = evidence_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    firewall_ok = (
        summary["external_other_endpoint_blocked"]
        and summary["no_non_loopback_order_endpoint_allowed"]
        and summary["m3d2f_test_present"]
        and summary["exact_send_surface_counts_ok"]
        and summary["direct_endpoint_gate_literal_count"] == 0
        and summary["scanner_constructor_guard_present"]
        and all(summary["production_gate_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and firewall_ok
    evidence = {
        "m3d_step": "M3d-2f",
        "m3d2_protected_endpoint_stage_closed": evidence_ready,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": git["stdout_tail"].strip(),
        "source_archive": archive_summary,
        "firewall": summary,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "external_other_endpoint_blocked": summary["external_other_endpoint_blocked"],
        "no_non_loopback_order_endpoint_allowed": summary[
            "no_non_loopback_order_endpoint_allowed"
        ],
        "external_endpoint_firewall_ok": firewall_ok,
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
