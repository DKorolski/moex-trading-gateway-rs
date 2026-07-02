#!/usr/bin/env python3
"""Generate an M3c positive-GetOrder waiver package without live order calls."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


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


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate source-bound M3c positive GetOrder waiver package."
    )
    parser.add_argument(
        "--source-archive",
        required=True,
        type=Path,
        help="Clean handoff archive to bind into the waiver package.",
    )
    parser.add_argument(
        "--output",
        default=Path("reports/m3c-order-endpoint-gate/positive-get-order-waiver.json"),
        type=Path,
        help="Positive GetOrder waiver package output path.",
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

    scan_code, scan_stdout, _scan_stderr = run_text(
        ["bash", "scripts/forbidden_surface_scan.sh"], root
    )
    scan_script = root / "scripts/forbidden_surface_scan.sh"

    waiver = {
        "m3c_step": "M3c-23",
        "positive_get_order_waiver_package": True,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": source_commit_full_sha,
        "source_archive_name": source_archive.name,
        "source_archive_sha256": sha256_file(source_archive),
        "slot": "positive_get_order_evidence_or_waiver",
        "requested_slot_status_after_reviewer_acceptance": "WaiverAccepted",
        "current_slot_status_until_reviewer_acceptance": "Pending",
        "waiver_reason": {
            "controlled_positive_get_order_requires_known_existing_broker_order_id": True,
            "known_existing_broker_order_id_not_stored_in_repo_or_handoff": True,
            "fabricating_real_get_order_evidence_is_disallowed": True,
            "real_order_post_delete_remain_prohibited": True,
            "safe_alternative_is_reviewer_accepted_waiver_before_implementation_gate": True,
        },
        "existing_coverage": {
            "fixture_doc": "docs/m3b24-m3c0-pre-order-readiness-closeout.md",
            "fixture_test": "m3b24_get_order_200_real_shape_fixture_covers_exact_and_mismatch_redacted",
            "get_order_200_exact_identity_fixture_covered": True,
            "get_order_200_mismatch_fixture_covered": True,
            "exact_identity_strength": "BrokerOrderIdExact",
            "mismatch_reason": "MismatchedOrderIdentity",
            "raw_ids_comments_redacted_in_fixture_report": True,
        },
        "operator_commitment_if_waiver_rejected": {
            "run_controlled_readonly_probe_only_with_reviewer_approved_inputs": True,
            "required_inputs": [
                "readonly FINAM token",
                "allowed account id",
                "known existing broker order id",
                "instrument symbol",
            ],
            "allowed_command": "broker-cli finam-real-readonly-evidence",
            "max_requests_lte_4": True,
            "get_only_broker_truth_probe": True,
            "order_endpoints_used": False,
        },
        "trading_boundary": {
            "endpoint_calls_allowed": False,
            "marker_constructible": False,
            "real_post_delete_added": False,
            "real_order_endpoint_enabled": False,
            "place_order_post_allowed": False,
            "cancel_order_delete_allowed": False,
            "command_consumer_connected_to_strategies": False,
            "real_finam_ack_lifecycle_enabled": False,
            "runtime_live_attachment": False,
            "live_ready": False,
            "first_live_micro": False,
            "stop_sltp_bracket": False,
        },
        "forbidden_surface_scan": {
            "status": "Ok" if scan_code == 0 else "Failed",
            "exit_code": scan_code,
            "script_path": "scripts/forbidden_surface_scan.sh",
            "script_sha256": sha256_file(scan_script),
            "stdout": scan_stdout,
        },
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(waiver, indent=2, sort_keys=True) + "\n")
    waiver_sha256 = sha256_file(output)
    output.with_suffix(output.suffix + ".sha256").write_text(
        f"{waiver_sha256}  {output.name}\n"
    )

    print(json.dumps({"output": str(output), "sha256": waiver_sha256}, indent=2))
    return 0 if scan_code == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
