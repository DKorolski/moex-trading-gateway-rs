#!/usr/bin/env python3
"""Generate M3j-18 release-freeze immutable evidence bundle.

No broker calls are performed. The bundle only hashes already-created M3j-16b,
M3j-17 and M3j-17a artifacts and records the frozen runtime boundary state.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ARTIFACTS = {
    "source_archive_cd2ae34": Path("reports/handoff/moex-trading-project-cd2ae34.zip"),
    "m3j16b_pre_send_gate": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j16b-repeat2-pre-send-gate-report.json"
    ),
    "m3j16b_actual_rawcapture": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j16b-repeat2-actual-rawcapture-report.json"
    ),
    "m3j16b_post_run_reconciliation": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j16b-repeat2-post-run-reconciliation-report.json"
    ),
    "m3j16b_raw_capture_summary": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j16b-repeat2-raw-capture-summary.json"
    ),
    "m3j16b_eod_summary": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j16b-repeat2-eod-summary.json"
    ),
    "m3j16b_actual_evidence": Path(
        "reports/m3j-pre-live/m3j16b-repeat2-actual-rawcapture-evidence.json"
    ),
    "m3j17_auth_single_json": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j17-auth-single-json-report.json"
    ),
    "m3j17_final_broker_truth_refresh": Path(
        "reports/m3j16-limit-cancel-one-shot/m3j17-final-broker-truth-refresh-report.json"
    ),
    "m3j17_closure_evidence": Path(
        "reports/m3j-pre-live/m3j17-post-live-micro-closure-evidence.json"
    ),
    "m3j17a_operator_signoff": Path(
        "reports/m3j-pre-live/m3j17a-operator-signoff-closure.json"
    ),
}


FROZEN_SOURCE_COMMIT = "cd2ae34fc2d6d37f61df1a82d2586f2d572ead07"
FROZEN_SOURCE_ARCHIVE_SHA256 = "cc23bc83c31d66bf79491331e48b2817f6e8133763f9372943a506f2ca5cf046"
EXPECTED_SIGNOFF_SHA256 = "5826c70da36b352454a34e139795dd22274cb3a44d535157296ca7bd92fe9c85"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path), "exists": path.exists()}
    if path.exists():
        data = path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def git_output(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j18-release-freeze-bundle.json"),
    )
    parser.add_argument(
        "--tag",
        default="m3j-live-micro-closed-cd2ae34",
        help="Expected immutable tag name for the frozen cd2ae34 source milestone.",
    )
    args = parser.parse_args()

    manifest = {name: artifact(path) for name, path in ARTIFACTS.items()}
    signoff = load_json(ARTIFACTS["m3j17a_operator_signoff"])
    final_truth = load_json(ARTIFACTS["m3j17_final_broker_truth_refresh"])[
        "pre_boundary_broker_truth"
    ]
    actual = load_json(ARTIFACTS["m3j16b_actual_rawcapture"])
    closure = load_json(ARTIFACTS["m3j17_closure_evidence"])

    tag_target = None
    tag_present = False
    try:
        tag_target = git_output("rev-list", "-n", "1", args.tag)
        tag_present = True
    except subprocess.CalledProcessError:
        tag_target = None

    all_artifacts_present = all(item["exists"] for item in manifest.values())
    source_archive_sha_ok = (
        manifest["source_archive_cd2ae34"].get("sha256") == FROZEN_SOURCE_ARCHIVE_SHA256
    )
    signoff_sha_ok = (
        manifest["m3j17a_operator_signoff"].get("sha256") == EXPECTED_SIGNOFF_SHA256
    )
    final_truth_clean = (
        final_truth.get("active_orders_count") == 0
        and final_truth.get("unknown_active_orders_count") == 0
        and final_truth.get("orphan_active_orders_count") == 0
        and final_truth.get("positions_count") == 0
        and final_truth.get("broker_truth_clean") is True
    )
    actual_limitcancel_completed = (
        actual.get("report", {}).get("boundary_invocation_performed") is True
        and actual.get("report", {}).get("real_finam_order_endpoint_used") is True
        and actual.get("execution_redacted", {}).get("place_attempted") is True
        and actual.get("execution_redacted", {}).get("cancel_attempted") is True
        and actual.get("execution_redacted", {}).get("broker_order_id_present") is True
    )
    runtime_boundaries_frozen = (
        actual.get("report", {}).get("runtime_live_attachment_allowed") is False
        and actual.get("report", {}).get("command_consumer_to_real_finam_allowed") is False
        and actual.get("report", {}).get("stop_sltp_bracket_replace_multileg_allowed") is False
    )
    signoff_ok = (
        signoff.get("operator_signoff_status") == "SignedOff"
        and signoff.get("operator_confirmations_all_true") is True
        and signoff.get("stage_status", {}).get("m3j_final_operational_closure") == "Closed"
    )
    closure_ok = closure.get("closure_ready_for_review") is True
    tag_ok = tag_present and tag_target == FROZEN_SOURCE_COMMIT

    release_freeze_ready = all(
        [
            all_artifacts_present,
            source_archive_sha_ok,
            signoff_sha_ok,
            final_truth_clean,
            actual_limitcancel_completed,
            runtime_boundaries_frozen,
            signoff_ok,
            closure_ok,
            tag_ok,
        ]
    )

    bundle = {
        "bundle_kind": "m3j18-release-freeze-immutable-evidence-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "frozen_source_commit_full_sha": FROZEN_SOURCE_COMMIT,
        "frozen_source_commit_short_sha": FROZEN_SOURCE_COMMIT[:7],
        "frozen_source_archive_sha256": FROZEN_SOURCE_ARCHIVE_SHA256,
        "release_freeze_tag": {
            "name": args.tag,
            "present": tag_present,
            "target_commit_full_sha": tag_target,
            "points_to_frozen_source_commit": tag_ok,
        },
        "artifact_manifest": manifest,
        "accepted_scope": {
            "symbol": "IMOEXF@RTSX",
            "side": "buy",
            "order_type": "limit",
            "qty": "1",
            "limit_price": "2210",
            "max_orders": 1,
            "place_then_cancel_only": True,
            "no_stop_sltp_bracket_replace_multileg": True,
        },
        "final_state": {
            "m3j_final_operational_closure": "Closed",
            "actual_limitcancel_completed": actual_limitcancel_completed,
            "post_run_broker_truth_clean": final_truth_clean,
            "operator_signoff_status": signoff.get("operator_signoff_status"),
            "continuous_runtime_live_enabled": False,
            "command_consumer_to_real_finam_enabled": False,
            "m4_features_blocked": True,
            "active_orders_count": final_truth.get("active_orders_count"),
            "unknown_active_orders_count": final_truth.get("unknown_active_orders_count"),
            "orphan_active_orders_count": final_truth.get("orphan_active_orders_count"),
            "positions_count": final_truth.get("positions_count"),
        },
        "freeze_checks": {
            "all_artifacts_present": all_artifacts_present,
            "source_archive_sha_ok": source_archive_sha_ok,
            "signoff_sha_ok": signoff_sha_ok,
            "final_truth_clean": final_truth_clean,
            "actual_limitcancel_completed": actual_limitcancel_completed,
            "runtime_boundaries_frozen": runtime_boundaries_frozen,
            "operator_signoff_ok": signoff_ok,
            "technical_closure_ok": closure_ok,
            "tag_ok": tag_ok,
        },
        "redaction": {
            "raw_secret_exported": False,
            "raw_jwt_exported": False,
            "raw_account_exported": False,
            "raw_broker_payload_exported": False,
            "raw_body_files_in_handoff": False,
        },
        "next_stage_policy": {
            "m3j18_performs_broker_calls": False,
            "continuous_runtime_live_must_not_be_inferred": True,
            "m4_requires_separate_design_review": True,
            "recommended_next_stage": "M3j-19 actual boundary failure matrix",
        },
        "release_freeze_ready_for_review": release_freeze_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(bundle, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            {
                "release_freeze_ready_for_review": release_freeze_ready,
                "frozen_source_commit_short_sha": FROZEN_SOURCE_COMMIT[:7],
                "tag_ok": tag_ok,
                "actual_limitcancel_completed": actual_limitcancel_completed,
                "post_run_broker_truth_clean": final_truth_clean,
                "operator_signoff_status": signoff.get("operator_signoff_status"),
                "continuous_runtime_live_enabled": False,
                "command_consumer_to_real_finam_enabled": False,
                "m4_features_blocked": True,
            },
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
    )
    return 0 if release_freeze_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
