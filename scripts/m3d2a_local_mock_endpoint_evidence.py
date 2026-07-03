#!/usr/bin/env python3
"""Generate M3d-2a local mock endpoint evidence without real order calls."""

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


HARNESS = Path("crates/finam-gateway/src/m3d2_local_mock_endpoint.rs")

CHECKS = [
    ["cargo", "fmt", "--all", "--check"],
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


def harness_summary(path: Path) -> dict[str, Any]:
    source = path.read_text(encoding="utf-8")
    forbidden_tokens = [
        ".post(",
        ".delete(",
        ".request(",
        ".send(",
        "Method::POST",
        "Method::DELETE",
        "reqwest",
        "OrderEndpointHttpClient",
        "OrderEndpointHttpTransport",
        "OrderEndpointHttpAdapter",
        "OrderEndpointHttpBackend",
        "EndpointGateApproved {",
        "LiveReady",
        "stop_sltp_bracket_enabled: true",
    ]
    forbidden_present = [
        token for token in forbidden_tokens if token in source
    ]
    required_tests = [
        "local_mock_endpoint_accepts_exact_place_limit_wire_request",
        "local_mock_endpoint_accepts_exact_cancel_wire_request",
        "local_mock_endpoint_rejects_wrong_method_without_raw_secret_export",
    ]
    required_patterns = [
        "pub const M3D2_PLACE_ORDER_ROUTE_TEMPLATE",
        "pub const M3D2_CANCEL_ORDER_ROUTE_TEMPLATE",
        "raw_path_exported: false",
        "raw_authorization_exported: false",
        "raw_body_exported: false",
        "#[cfg(test)]",
        "TcpListener::bind(\"127.0.0.1:0\")",
    ]
    return {
        "harness_path": str(path),
        "harness_sha256": sha256_file(path),
        "required_test_count": len(required_tests),
        "required_tests_present": {
            name: name in source for name in required_tests
        },
        "required_patterns_present": {
            pattern: pattern in source for pattern in required_patterns
        },
        "forbidden_token_count": len(forbidden_present),
        "forbidden_tokens_present": forbidden_present,
        "test_only_socket_helper_present": "#[cfg(test)]" in source
        and "TcpListener::bind(\"127.0.0.1:0\")" in source,
        "redacted_diagnostic_flags_present": all(
            pattern in source
            for pattern in [
                "raw_path_exported: false",
                "raw_authorization_exported: false",
                "raw_body_exported: false",
            ]
        ),
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-2a source-bound local mock endpoint evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2a-local-mock-evidence.json"),
    )
    args = parser.parse_args()

    root = repo_root()
    git = run_text(["git", "rev-parse", "HEAD"], root)
    if git["exit_code"] != 0:
        print(git["stderr_tail"], file=sys.stderr)
        return git["exit_code"]
    source_commit = git["stdout_tail"].strip()

    harness_path = root / HARNESS
    if not harness_path.exists():
        print(f"missing harness: {harness_path}", file=sys.stderr)
        return 2

    checks = [run_text(command, root) for command in CHECKS]
    archive_summary = None
    if args.source_archive:
        archive_path = (root / args.source_archive).resolve()
        if not archive_path.exists():
            print(f"source archive does not exist: {archive_path}", file=sys.stderr)
            return 2
        archive_summary = clean_handoff_summary(archive_path)

    harness = harness_summary(harness_path)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    harness_ok = (
        harness["forbidden_token_count"] == 0
        and all(harness["required_tests_present"].values())
        and all(harness["required_patterns_present"].values())
        and harness["test_only_socket_helper_present"]
        and harness["redacted_diagnostic_flags_present"]
    )
    evidence_ready = all_checks_ok and archive_ok and harness_ok

    evidence = {
        "m3d_step": "M3d-2a",
        "local_mock_endpoint_wire_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "harness": harness,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "harness_ok": harness_ok,
        "trading_boundary": {
            "real_post_delete_added": False,
            "endpoint_calls_allowed": False,
            "real_order_endpoint_enabled": False,
            "scanner_allowlist_mode_enabled": False,
            "endpoint_gate_constructible": False,
            "command_consumer_enabled": False,
            "live_ready_allowed": False,
            "stop_sltp_bracket_enabled": False,
            "runtime_live_attachment_allowed": False,
        },
        "allowed_scope": {
            "local_loopback_mock_only": True,
            "exact_two_route_design_templates": [
                "/v1/accounts/{account_id}/orders",
                "/v1/accounts/{account_id}/orders/{order_id}",
            ],
            "place_market_limit_cancel_only": True,
            "real_broker_calls_allowed": False,
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

