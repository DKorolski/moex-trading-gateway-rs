#!/usr/bin/env python3
"""Generate M3c pre-implementation gate package without order endpoint calls."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


EVIDENCE_FILES = {
    "design_evidence": Path("reports/m3c-order-endpoint-gate/design-evidence.json"),
    "release_profile_evidence": Path(
        "reports/m3c-order-endpoint-gate/release-profile-evidence.json"
    ),
    "route_template_recheck_evidence": Path(
        "reports/m3c-order-endpoint-gate/route-template-recheck-evidence.json"
    ),
    "positive_get_order_waiver": Path(
        "reports/m3c-order-endpoint-gate/positive-get-order-waiver.json"
    ),
    "undocumented_2xx_status_evidence": Path(
        "reports/m3c-order-endpoint-gate/undocumented-2xx-status-evidence.json"
    ),
    "cancel_409_410_status_evidence": Path(
        "reports/m3c-order-endpoint-gate/cancel-409-410-status-evidence.json"
    ),
}

EXPECTED_SLOT_STATES = {
    "release_profile_evidence_or_waiver": "EvidenceProvided",
    "positive_get_order_evidence_or_waiver": "WaiverAccepted",
    "route_template_recheck": "EvidenceProvided",
    "undocumented_2xx_status_semantics": "EvidenceProvided",
    "cancel_409_410_status_semantics": "EvidenceProvided",
}


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


def source_archive_sha(payload: dict[str, Any]) -> str | None:
    if "source_archive_sha256" in payload:
        return payload.get("source_archive_sha256")
    source = payload.get("evidence", {}).get("source", {})
    return source.get("source_archive_sha256")


def source_commit(payload: dict[str, Any]) -> str | None:
    if "source_commit_full_sha" in payload:
        return payload.get("source_commit_full_sha")
    source = payload.get("evidence", {}).get("source", {})
    return source.get("source_commit_full_sha")


def source_archive_name(payload: dict[str, Any]) -> str | None:
    if "source_archive_name" in payload:
        return payload.get("source_archive_name")
    source = payload.get("evidence", {}).get("source", {})
    return source.get("source_archive_name")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate source-bound M3c pre-implementation gate package."
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the pre-implementation package.",
    )
    parser.add_argument(
        "--output",
        default=Path("reports/m3c-order-endpoint-gate/pre-implementation-gate-package.json"),
        type=Path,
        help="Pre-implementation gate package JSON output path.",
    )
    args = parser.parse_args()

    root = repo_root()
    source_archive = (root / args.source_archive).resolve()
    output = (root / args.output).resolve()

    if not source_archive.exists():
        print(f"source archive does not exist: {source_archive}", file=sys.stderr)
        return 2

    git_code, source_commit_full_sha, git_stderr = run_text(
        ["git", "rev-parse", "HEAD"], root
    )
    if git_code != 0:
        print(git_stderr, file=sys.stderr)
        return git_code

    archive_sha = sha256_file(source_archive)
    evidence_payloads: dict[str, dict[str, Any]] = {}
    evidence_manifest = []
    for name, relative_path in EVIDENCE_FILES.items():
        path = root / relative_path
        if not path.exists():
            print(f"evidence file does not exist: {relative_path}", file=sys.stderr)
            return 2
        payload = read_json(path)
        evidence_payloads[name] = payload
        evidence_manifest.append(
            {
                "name": name,
                "path": str(relative_path),
                "sha256": sha256_file(path),
                "source_commit_full_sha": source_commit(payload),
                "source_archive_name": source_archive_name(payload),
                "source_archive_sha256": source_archive_sha(payload),
            }
        )

    scan_code, scan_stdout, _scan_stderr = run_text(
        ["bash", "scripts/forbidden_surface_scan.sh"], root
    )
    negative_code, negative_stdout, _negative_stderr = run_text(
        ["bash", "scripts/forbidden_surface_negative_harness.sh"], root
    )
    transition_code, transition_stdout, _transition_stderr = run_text(
        ["bash", "scripts/order_endpoint_scanner_transition_spec.sh"], root
    )

    design = evidence_payloads["design_evidence"]
    evidence = design.get("evidence", {})
    slot_states = {slot: evidence.get(slot) for slot in EXPECTED_SLOT_STATES}
    slot_states_match = slot_states == EXPECTED_SLOT_STATES
    slot_counts_match = (
        evidence.get("evidence_slot_count") == 5
        and evidence.get("evidence_provided_or_waiver_count") == 5
        and evidence.get("evidence_pending_count") == 0
    )
    source_binding_match = all(
        item["source_commit_full_sha"] == source_commit_full_sha
        and item["source_archive_name"] == source_archive.name
        and item["source_archive_sha256"] == archive_sha
        for item in evidence_manifest
    )

    boundary = {
        "endpoint_calls_allowed": design.get("endpoint_calls_allowed"),
        "marker_constructible": design.get("marker_constructible"),
        "real_post_delete_added": design.get("real_post_delete_added"),
        "real_order_endpoint_enabled": design.get("real_order_endpoint_enabled"),
        "command_consumer_enabled": design.get("command_consumer_enabled"),
        "order_placement_enabled": design.get("order_placement_enabled"),
        "cancel_enabled": design.get("cancel_enabled"),
        "stop_sltp_bracket_enabled": design.get("stop_sltp_bracket_enabled"),
    }
    boundary_closed = all(value is False for value in boundary.values())

    package_ready_for_review = (
        source_binding_match
        and slot_states_match
        and slot_counts_match
        and boundary_closed
        and scan_code == 0
        and negative_code == 0
        and transition_code == 0
    )

    package = {
        "m3c_step": "M3c-26",
        "pre_implementation_gate_package": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": archive_sha,
        "evidence_manifest": evidence_manifest,
        "all_evidence_artifacts_source_bound_to_same_archive": source_binding_match,
        "slot_states": slot_states,
        "slot_states_match_expected": slot_states_match,
        "slot_counts_match_expected": slot_counts_match,
        "expected_slot_states": EXPECTED_SLOT_STATES,
        "evidence_slot_count": evidence.get("evidence_slot_count"),
        "evidence_provided_or_waiver_count": evidence.get(
            "evidence_provided_or_waiver_count"
        ),
        "evidence_pending_count": evidence.get("evidence_pending_count"),
        "trading_boundary": boundary,
        "trading_boundary_closed": boundary_closed,
        "gate_decision": design.get("gate_decision"),
        "implementation_decision_request": {
            "prepared_for_review": True,
            "requests_reviewer_decision_for_future_exact_two_route_allowlist": True,
            "requested_future_routes": [
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
            ],
            "not_authorized_by_this_package": True,
            "real_order_endpoint_calls_allowed_now": False,
            "endpoint_gate_approved_constructible_now": False,
            "runtime_live_attachment_allowed_now": False,
        },
        "scanners": {
            "forbidden_surface_scan": {
                "status": "Ok" if scan_code == 0 else "Failed",
                "exit_code": scan_code,
                "stdout": scan_stdout,
            },
            "forbidden_surface_negative_harness": {
                "status": "Ok" if negative_code == 0 else "Failed",
                "exit_code": negative_code,
                "stdout": negative_stdout,
            },
            "order_endpoint_scanner_transition_spec": {
                "status": "Ok" if transition_code == 0 else "Failed",
                "exit_code": transition_code,
                "stdout": transition_stdout,
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
