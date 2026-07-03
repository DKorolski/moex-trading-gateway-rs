#!/usr/bin/env python3
"""Generate M3d-2b strict FINAM request/response contract evidence."""

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
GATEWAY_LIB = Path("crates/finam-gateway/src/lib.rs")
REAL_ENDPOINT = Path("crates/finam-gateway/src/real_order_endpoint.rs")

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


def source_contains_all(source: str, patterns: list[str]) -> dict[str, bool]:
    return {pattern: pattern in source for pattern in patterns}


def strict_contract_summary(root: Path) -> dict[str, Any]:
    harness = (root / HARNESS).read_text(encoding="utf-8")
    gateway_lib = (root / GATEWAY_LIB).read_text(encoding="utf-8")
    real_endpoint = (root / REAL_ENDPOINT).read_text(encoding="utf-8")

    positive_tests = [
        "local_mock_endpoint_accepts_exact_place_limit_finam_body",
        "local_mock_endpoint_accepts_exact_place_market_finam_body",
        "local_mock_endpoint_accepts_exact_cancel_wire_request",
    ]
    negative_tests = [
        "local_mock_endpoint_rejects_missing_client_order_id",
        "local_mock_endpoint_rejects_missing_time_in_force",
        "local_mock_endpoint_rejects_side_buy_alias_instead_of_finam_enum",
        "local_mock_endpoint_rejects_order_type_limit_alias_instead_of_finam_enum",
        "local_mock_endpoint_rejects_market_with_limit_price",
        "local_mock_endpoint_rejects_limit_without_limit_price",
        "local_mock_endpoint_rejects_stop_price_legs_and_sltp_fields",
        "local_mock_endpoint_rejects_wrong_route_extra_segment",
        "local_mock_endpoint_rejects_missing_authorization",
        "local_mock_endpoint_rejects_wrong_content_type",
        "local_mock_endpoint_rejects_non_json_body",
    ]
    response_matrix_tests = [
        "local_mock_response_matrix_covers_required_endpoint_outcomes",
    ]
    strict_patterns = [
        "SIDE_BUY",
        "SIDE_SELL",
        "ORDER_TYPE_MARKET",
        "ORDER_TYPE_LIMIT",
        "TIME_IN_FORCE_DAY",
        "TIME_IN_FORCE_GOOD_TILL_CANCEL",
        "TIME_IN_FORCE_IOC",
        "TIME_IN_FORCE_FOK",
        "client_order_id_missing",
        "time_in_force_missing",
        "limit_price_forbidden_for_market",
        "limit_price_missing_for_limit",
        "plain_order_field_not_allowed",
        "quantity_value_not_decimal_string",
        "symbol_format_not_pinned_broker_symbol",
    ]
    renderer_patterns = [
        "classify_m3d2_local_mock_place_spec",
        "classify_m3d2_local_mock_cancel_spec",
        "build_place_order_request",
        "build_cancel_order_request",
        "PreflightApprovedPlaceOrder",
        "PreflightApprovedCancelOrder",
    ]
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
        "LiveReady",
        "stop_sltp_bracket_enabled: true",
    ]
    forbidden_in_harness = [token for token in forbidden_tokens if token in harness]
    forbidden_in_design_endpoint = [
        token
        for token in [
            ".post(",
            ".delete(",
            ".request(",
            ".send(",
            "Method::POST",
            "Method::DELETE",
            "reqwest",
            "HttpClient",
            "Adapter",
            "Backend",
        ]
        if token in real_endpoint
    ]
    boundary_source = {
        "implementation_review_const_false": (
            "const REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED: bool = false;"
            in gateway_lib
        ),
        "endpoint_gate_constructor_checks_review_const": (
            "if !REAL_ORDER_ENDPOINT_IMPLEMENTATION_REVIEW_ACCEPTED" in gateway_lib
        ),
        "default_real_order_endpoint_enabled_false": (
            "real_order_endpoint_enabled: false" in gateway_lib
        ),
        "default_command_consumer_enabled_false": (
            "command_consumer_enabled: false" in gateway_lib
        ),
        "scanner_allowlist_mode_not_enabled": (
            "scanner_allowlist_mode_enabled: true" not in harness
            and "scanner_allowlist_mode_enabled: true" not in gateway_lib
        ),
    }

    return {
        "harness_path": str(HARNESS),
        "harness_sha256": sha256_file(root / HARNESS),
        "positive_tests_present": source_contains_all(harness, positive_tests),
        "negative_tests_present": source_contains_all(harness, negative_tests),
        "response_matrix_tests_present": source_contains_all(
            harness, response_matrix_tests
        ),
        "strict_contract_patterns_present": source_contains_all(harness, strict_patterns),
        "renderer_binding_patterns_present": source_contains_all(harness, renderer_patterns),
        "forbidden_token_count_in_harness": len(forbidden_in_harness),
        "forbidden_tokens_in_harness": forbidden_in_harness,
        "forbidden_token_count_in_design_endpoint": len(forbidden_in_design_endpoint),
        "forbidden_tokens_in_design_endpoint": forbidden_in_design_endpoint,
        "source_derived_boundary": boundary_source,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate M3d-2b strict source-bound contract evidence."
    )
    parser.add_argument(
        "--source-archive",
        type=Path,
        help="Optional clean handoff archive to bind into evidence.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3d-protected-endpoint/m3d2b-strict-contract-evidence.json"),
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

    strict_summary = strict_contract_summary(root)
    all_checks_ok = all(check["exit_code"] == 0 for check in checks)
    archive_ok = archive_summary is None or archive_summary["clean"]
    strict_contract_ok = (
        all(strict_summary["positive_tests_present"].values())
        and all(strict_summary["negative_tests_present"].values())
        and all(strict_summary["response_matrix_tests_present"].values())
        and all(strict_summary["strict_contract_patterns_present"].values())
        and all(strict_summary["renderer_binding_patterns_present"].values())
        and strict_summary["forbidden_token_count_in_harness"] == 0
        and strict_summary["forbidden_token_count_in_design_endpoint"] == 0
        and all(strict_summary["source_derived_boundary"].values())
    )
    evidence_ready = all_checks_ok and archive_ok and strict_contract_ok

    evidence = {
        "m3d_step": "M3d-2b",
        "strict_finam_request_response_contract_evidence": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit,
        "source_archive": archive_summary,
        "strict_contract": strict_summary,
        "checks": checks,
        "all_checks_ok": all_checks_ok,
        "strict_contract_ok": strict_contract_ok,
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
        "next_stage_allowed_after_review": "M3d-2c real transport behind gate review",
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
