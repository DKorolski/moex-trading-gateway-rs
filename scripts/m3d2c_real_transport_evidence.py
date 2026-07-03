#!/usr/bin/env python3
"""Generate M3d-2c gated real transport evidence against local mock only."""

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
GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
SCANNER = Path("scripts/forbidden_surface_scan.sh")

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


def contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def transport_summary(root: Path) -> dict[str, Any]:
    transport = (root / TRANSPORT).read_text(encoding="utf-8")
    gateway_lib = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    scanner = (root / SCANNER).read_text(encoding="utf-8")
    required_tests = [
        "m3d2c_place_transport_sends_exact_post_to_local_mock_with_bearer_auth",
        "m3d2c_cancel_transport_sends_exact_delete_to_local_mock_with_bearer_auth",
        "m3d2c_post_send_semantics_preserve_reconciliation_and_no_blind_retry",
    ]
    required_transport_patterns = [
        "pub enum FinamAuthorizationHeaderMode",
        "BearerJwt",
        "Authorization",
        "https://api.finam.ru/docs/rest/llms.txt",
        "EndpointGateApproved",
        "FinamPlaceOrderRequestSpec",
        "FinamCancelOrderRequestSpec",
        "post_send_semantics",
        "SubmittedPendingBrokerOrderIdReconciliation",
        "CancelAcceptedPendingReconciliation",
        "ReconciliationRequired",
        "TimeoutUnknownPending",
        "raw_token_exported: false",
        "raw_path_exported: false",
        "raw_body_exported: false",
    ]
    exact_counts = {
        ".post(": transport.count(".post("),
        ".delete(": transport.count(".delete("),
        ".send(": transport.count(".send("),
    }
    source_boundary = {
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
        "scanner_exact_transport_allowlist_present": (
            "m3d2_real_order_transport.rs" in scanner
            and "exactly one .post(" in scanner
            and "M3d-2c transport allowlist mismatch" in scanner
        ),
    }
    return {
        "transport_path": str(TRANSPORT),
        "transport_sha256": sha256_file(root / TRANSPORT),
        "required_tests_present": contains_all(transport, required_tests),
        "required_transport_patterns_present": contains_all(
            transport, required_transport_patterns
        ),
        "exact_send_surface_counts": exact_counts,
        "exact_send_surface_counts_ok": exact_counts == {
            ".post(": 1,
            ".delete(": 1,
            ".send(": 1,
        },
        "source_derived_boundary": source_boundary,
        "auth_header_policy": {
            "mode": "BearerJwt",
            "header": "Authorization",
            "scheme": "Bearer",
            "official_source": "https://api.finam.ru/docs/rest/llms.txt",
            "raw_token_exported": False,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-2c gated real transport evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2c-real-transport-evidence.json"),
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

    transport = transport_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    transport_ok = (
        all(transport["required_tests_present"].values())
        and all(transport["required_transport_patterns_present"].values())
        and transport["exact_send_surface_counts_ok"]
        and all(transport["source_derived_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and transport_ok

    evidence = {
        "m3d_step": "M3d-2c",
        "real_transport_behind_gate_local_mock_only": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "transport": transport,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "transport_ok": transport_ok,
        "trading_boundary": {
            "real_transport_source_added": True,
            "real_broker_calls_allowed_by_default": False,
            "endpoint_calls_allowed": False,
            "real_order_endpoint_enabled": False,
            "endpoint_gate_constructible_in_production": False,
            "command_consumer_enabled": False,
            "live_ready_allowed": False,
            "runtime_live_attachment_allowed": False,
            "stop_sltp_bracket_enabled": False,
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

