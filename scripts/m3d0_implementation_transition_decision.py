#!/usr/bin/env python3
"""Generate M3d-0 implementation-transition decision package.

This package records reviewer acceptance of M3c-26 and prepares the future
exact-two-route scanner transition rules. It intentionally does not add or
authorize real order endpoint calls.
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


APPROVED_IMPLEMENTATION_MODULE = "crates/finam-gateway/src/real_order_endpoint.rs"

EXACT_ALLOWED_ROUTES = [
    {
        "http_method": "POST",
        "purpose": "PlaceOrder",
        "route_template": "/v1/accounts/{account_id}/orders",
    },
    {
        "http_method": "DELETE",
        "purpose": "CancelOrder",
        "route_template": "/v1/accounts/{account_id}/orders/{order_id}",
    },
]

SCANNER_MUST_FAIL_SURFACES = [
    "same_module_extra_post",
    "same_module_extra_delete",
    "generic_request_post",
    "generic_request_delete",
    "route_string_bypass",
    "non_reqwest_order_endpoint_abstraction",
    "wrong_module_post_delete",
    "sltp_or_bracket_post_delete",
    "runtime_command_consumer_bypass",
]


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def run_text(command: list[str], cwd: Path) -> tuple[int, str, str]:
    completed = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    return completed.returncode, completed.stdout.strip(), completed.stderr.strip()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    if not isinstance(payload, dict):
        raise ValueError(f"expected JSON object in {path}")
    return payload


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Generate source-bound M3d-0 exact-two-route implementation "
            "transition decision package."
        )
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the M3d-0 package.",
    )
    parser.add_argument(
        "--m3c-pre-implementation-package",
        default=Path("reports/m3c-order-endpoint-gate/pre-implementation-gate-package.json"),
        type=Path,
        help="Accepted M3c-26 pre-implementation package JSON.",
    )
    parser.add_argument(
        "--output",
        default=Path(
            "reports/m3d-implementation-transition/"
            "implementation-transition-decision.json"
        ),
        type=Path,
        help="M3d-0 implementation transition decision JSON output path.",
    )
    args = parser.parse_args()

    root = repo_root()
    source_archive = (root / args.source_archive).resolve()
    m3c_package_path = (root / args.m3c_pre_implementation_package).resolve()
    output = (root / args.output).resolve()

    if not source_archive.exists():
        print(f"source archive does not exist: {source_archive}", file=sys.stderr)
        return 2
    if not m3c_package_path.exists():
        print(
            f"M3c pre-implementation package does not exist: {m3c_package_path}",
            file=sys.stderr,
        )
        return 2

    git_code, source_commit_full_sha, git_stderr = run_text(
        ["git", "rev-parse", "HEAD"], root
    )
    if git_code != 0:
        print(git_stderr, file=sys.stderr)
        return git_code

    archive_sha = sha256_file(source_archive)
    m3c_package = read_json(m3c_package_path)
    m3c_package_sha = sha256_file(m3c_package_path)

    scan_code, scan_stdout, scan_stderr = run_text(
        ["bash", "scripts/forbidden_surface_scan.sh"], root
    )
    negative_code, negative_stdout, negative_stderr = run_text(
        ["bash", "scripts/forbidden_surface_negative_harness.sh"], root
    )
    transition_code, transition_stdout, transition_stderr = run_text(
        ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"], root
    )

    m3c_source_binding_match = (
        m3c_package.get("source_commit_full_sha") == source_commit_full_sha
        and m3c_package.get("source_archive_name") == source_archive.name
        and m3c_package.get("source_archive_sha256") == archive_sha
    )
    m3c_package_accepted = (
        m3c_package.get("m3c_step") == "M3c-26"
        and m3c_package.get("pre_implementation_gate_package") is True
        and m3c_package.get("package_ready_for_review") is True
        and m3c_package.get("evidence_pending_count") == 0
        and m3c_package.get("trading_boundary_closed") is True
        and m3c_package.get("implementation_decision_request", {}).get(
            "not_authorized_by_this_package"
        )
        is True
    )

    current_boundary = {
        "endpoint_calls_allowed": False,
        "marker_constructible": False,
        "real_post_delete_added": False,
        "real_order_endpoint_enabled": False,
        "command_consumer_enabled": False,
        "order_placement_enabled": False,
        "cancel_enabled": False,
        "runtime_live_attachment_allowed": False,
        "live_ready_allowed": False,
        "first_live_micro_allowed": False,
        "stop_sltp_bracket_enabled": False,
    }

    scanner_green = scan_code == 0 and negative_code == 0 and transition_code == 0
    package_ready_for_review = (
        m3c_source_binding_match
        and m3c_package_accepted
        and scanner_green
        and all(value is False for value in current_boundary.values())
    )

    package = {
        "m3d_step": "M3d-0",
        "implementation_transition_decision_artifact": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": archive_sha,
        "source_archive_content_binding_verified": m3c_source_binding_match,
        "m3c26_acceptance_recorded": True,
        "m3c26_reviewed_commit": "e006d76e95d8c54de80edf1a4fb1a4c81bdaee0b",
        "m3c26_reviewed_source_archive_sha256": (
            "be116a1c714d31207b69374b3bf5847d613045e4c6af745ba591e4a76f015741"
        ),
        "m3c26_reviewed_pre_implementation_package_sha256": (
            "f6af0329b6e345986bccd483d413219b702b3495226e9edb03fb3f7a47685f28"
        ),
        "m3c_pre_implementation_package": {
            "path": str(args.m3c_pre_implementation_package),
            "sha256": m3c_package_sha,
            "source_commit_full_sha": m3c_package.get("source_commit_full_sha"),
            "source_archive_name": m3c_package.get("source_archive_name"),
            "source_archive_sha256": m3c_package.get("source_archive_sha256"),
            "package_ready_for_review": m3c_package.get("package_ready_for_review"),
            "trading_boundary_closed": m3c_package.get("trading_boundary_closed"),
            "evidence_pending_count": m3c_package.get("evidence_pending_count"),
            "accepted_for_transition_input": m3c_package_accepted,
        },
        "decision_scope": {
            "transition_preparation_only": True,
            "implementation_source_allowed_now": False,
            "endpoint_calls_allowed_now": False,
            "scanner_allowlist_mode_enabled_now": False,
            "reviewer_decision_required_before_executable_order_endpoints": True,
        },
        "scanner_transition_decision": {
            "current_mode": "CurrentDenyAllOrderPostDelete",
            "future_mode": "FutureExactTwoRouteAllowlistAfterExplicitReview",
            "future_mode_activation_allowed_by_this_package": False,
            "approved_implementation_module": APPROVED_IMPLEMENTATION_MODULE,
            "exact_allowed_routes": EXACT_ALLOWED_ROUTES,
            "exact_allowed_route_count": len(EXACT_ALLOWED_ROUTES),
            "allowlist_must_reject_all_other_post_delete": True,
            "scanner_must_fail_surfaces": SCANNER_MUST_FAIL_SURFACES,
            "scanner_must_fail_surface_count": len(SCANNER_MUST_FAIL_SURFACES),
            "generic_request_bypass_forbidden": True,
            "route_string_bypass_forbidden": True,
            "non_reqwest_abstraction_bypass_forbidden": True,
        },
        "gate_marker_decision": {
            "endpoint_gate_approved_constructible_now": False,
            "route_rendering_must_require_endpoint_gate_approved": True,
            "http_send_must_require_endpoint_gate_approved": True,
            "diagnostics_must_not_construct_gate_marker": True,
            "diagnostics_must_not_feed_transport": True,
            "while_marker_unconstructible_routes_remain_non_executable": True,
        },
        "source_reporting_decision": {
            "real_post_delete_added_now": False,
            "real_order_endpoint_enabled_now": False,
            "endpoint_calls_allowed_now": False,
            "future_real_post_delete_added_does_not_imply_endpoint_calls_allowed": True,
            "future_endpoint_calls_allowed_requires_gate_marker_constructible": True,
            "future_endpoint_calls_allowed_requires_real_order_endpoint_enabled": True,
            "future_endpoint_calls_allowed_requires_operator_arm": True,
            "future_endpoint_calls_allowed_requires_durable_state_checkpoint": True,
        },
        "trading_boundary": current_boundary,
        "scanners": {
            "forbidden_surface_scan": {
                "status": "Ok" if scan_code == 0 else "Failed",
                "exit_code": scan_code,
                "stdout": scan_stdout,
                "stderr": scan_stderr,
            },
            "forbidden_surface_negative_harness": {
                "status": "Ok" if negative_code == 0 else "Failed",
                "exit_code": negative_code,
                "stdout": negative_stdout,
                "stderr": negative_stderr,
            },
            "order_endpoint_scanner_transition_spec": {
                "status": "Ok" if transition_code == 0 else "Failed",
                "exit_code": transition_code,
                "stdout": transition_stdout,
                "stderr": transition_stderr,
            },
        },
        "package_ready_for_review": package_ready_for_review,
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(package, indent=2, sort_keys=True) + "\n")
    package_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{package_sha256}  {output.name}\n"
    )

    print(json.dumps({"output": str(output), "sha256": package_sha256}, indent=2))
    return 0 if package_ready_for_review else 1


if __name__ == "__main__":
    raise SystemExit(main())
