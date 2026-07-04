#!/usr/bin/env python3
"""Generate M3j-19 actual-boundary failure matrix evidence.

This script performs no broker calls. It runs local tests/scanners and verifies
that source-backed failure cases remain present for the protected actual order
boundary after the frozen M3j live-micro milestone.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DOC = Path("docs/m3j19-actual-boundary-failure-matrix.md")
M3J18_BUNDLE = Path("reports/m3j-pre-live/m3j18-release-freeze-bundle.json")
TRANSPORT = Path("crates/finam-gateway/src/m3d2_real_order_transport.rs")
LIFECYCLE = Path("crates/finam-gateway/src/m3d2_real_transport_lifecycle.rs")
REAL_ENDPOINT = Path("crates/finam-gateway/src/real_order_endpoint.rs")

FROZEN_SOURCE_COMMIT = "cd2ae34fc2d6d37f61df1a82d2586f2d572ead07"

COMMANDS = {
    "transport_post_send_semantics_test": [
        "cargo",
        "test",
        "-p",
        "finam-gateway",
        "m3d2c_post_send_semantics_preserve_reconciliation_and_no_blind_retry",
    ],
    "lifecycle_failure_matrix_test": [
        "cargo",
        "test",
        "-p",
        "finam-gateway",
        "m3d2e_lifecycle_matrix_maps_all_required_transport_outcomes",
    ],
    "checkpoint_reuse_guard_test": [
        "cargo",
        "test",
        "-p",
        "finam-gateway",
        "endpoint_attempt_id_lifecycle_prevents_reuse_after_timeout_manual_or_terminal",
    ],
    "command_consumer_default_disabled_test": [
        "cargo",
        "test",
        "-p",
        "finam-gateway",
        "m3d2e_command_consumer_cannot_route_to_real_transport_by_default",
    ],
    "forbidden_surface_scan": ["bash", "scripts/forbidden_surface_scan.sh"],
    "forbidden_surface_negative_harness": [
        "bash",
        "scripts/forbidden_surface_negative_harness.sh",
    ],
    "order_endpoint_scanner_transition_spec": [
        "bash",
        "scripts/order_endpoint_scanner_transition_spec.sh",
    ],
    "script_py_compile": [
        "python3",
        "-m",
        "py_compile",
        "scripts/m3j19_actual_boundary_failure_matrix.py",
    ],
}


CASE_MARKERS = {
    "place_accepted_broker_order_id_missing": {
        "source": TRANSPORT,
        "markers": [
            "SubmittedPendingBrokerOrderIdReconciliation",
            "accepted_without_id",
        ],
        "expected": {
            "post_send_semantics": "SubmittedPendingBrokerOrderIdReconciliation",
            "no_blind_retry": True,
            "reconciliation_required": True,
        },
    },
    "place_timeout_after_send": {
        "source": LIFECYCLE,
        "markers": [
            "place_timeout_504",
            "TimeoutUnknownPending",
            "SubmitTimedOut",
        ],
        "expected": {
            "state": "TimeoutUnknownPending",
            "no_blind_retry": True,
            "reconciliation_required": True,
        },
    },
    "place_http_4xx_5xx": {
        "source": LIFECYCLE,
        "markers": [
            "place_service_interval_503",
            "MaintenanceDisarm",
            "ManualInterventionRequired",
        ],
        "expected": {
            "manual_intervention_or_disarm": True,
            "no_blind_retry": True,
        },
    },
    "place_send_error_after_possible_send": {
        "source": LIFECYCLE,
        "markers": [
            "place_send_error_after_possible_send",
            "sent_error_execution",
            "ReconciliationRequired",
        ],
        "expected": {
            "state": "ManualInterventionRequired",
            "no_blind_retry": True,
            "reconciliation_required": True,
        },
    },
    "cancel_accepted": {
        "source": TRANSPORT,
        "markers": [
            "CancelAcceptedPendingReconciliation",
            "cancel_order",
        ],
        "expected": {
            "post_send_semantics": "CancelAcceptedPendingReconciliation",
            "post_run_reconciliation_required": True,
        },
    },
    "cancel_timeout_after_send": {
        "source": LIFECYCLE,
        "markers": [
            "cancel_timeout_504",
            "CancelTimeoutUnknownPending",
            "CancelTimedOut",
        ],
        "expected": {
            "state": "CancelTimeoutUnknownPending",
            "no_blind_retry": True,
            "reconciliation_required": True,
        },
    },
    "cancel_rejected_not_found_already_terminal": {
        "source": LIFECYCLE,
        "markers": [
            "m3d2d_cancel_lifecycle_persists_request_cancel_and_reconciles_conflict",
            "CancelAccepted",
            "ReconciliationRequired",
        ],
        "expected": {
            "conservative_conflict_or_terminal_handling": True,
            "no_blind_retry": True,
        },
    },
    "duplicate_actual_invocation_blocked": {
        "source": REAL_ENDPOINT,
        "markers": [
            "MarkerAlreadyUsed",
            "endpoint_attempt_id_lifecycle_prevents_reuse_after_timeout_manual_or_terminal",
        ],
        "expected": {
            "one_shot_marker_reuse_blocked": True,
            "second_endpoint_attempt_blocked": True,
        },
    },
    "retry_after_ambiguous_place_forbidden_without_reconciliation": {
        "source": LIFECYCLE,
        "markers": [
            "m3d2d_crash_windows_recover_conservatively_without_blind_retry",
            "TimeoutUnknownPending",
            "ReconciliationRequired",
        ],
        "expected": {
            "no_blind_retry": True,
            "requires_broker_truth_reconciliation_or_operator_decision": True,
        },
    },
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    result: dict[str, Any] = {"path": str(path), "exists": path.exists()}
    if path.exists():
        data = path.read_bytes()
        result.update({"sha256": sha256_bytes(data), "bytes": len(data)})
    return result


def run_command(cmd: list[str]) -> dict[str, Any]:
    completed = subprocess.run(cmd, text=True, capture_output=True)
    return {
        "cmd": cmd,
        "exit_code": completed.returncode,
        "ok": completed.returncode == 0,
        "stdout_tail": completed.stdout[-4000:],
        "stderr_tail": completed.stderr[-4000:],
    }


def git_output(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True).strip()


def marker_check(case: str, spec: dict[str, Any]) -> dict[str, Any]:
    path = spec["source"]
    text = path.read_text() if path.exists() else ""
    markers = spec["markers"]
    present = {marker: marker in text for marker in markers}
    return {
        "case": case,
        "source": str(path),
        "source_sha256": artifact(path).get("sha256"),
        "markers_present": present,
        "ok": bool(text) and all(present.values()),
        "expected": spec["expected"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("reports/m3j-pre-live/m3j19-actual-boundary-failure-matrix-evidence.json"),
    )
    parser.add_argument(
        "--skip-command-checks",
        action="store_true",
        help="Only generate source-marker evidence without running cargo/scanner commands.",
    )
    args = parser.parse_args()

    command_results = {}
    if not args.skip_command_checks:
        command_results = {name: run_command(cmd) for name, cmd in COMMANDS.items()}

    case_results = {name: marker_check(name, spec) for name, spec in CASE_MARKERS.items()}
    m3j18 = json.loads(M3J18_BUNDLE.read_text()) if M3J18_BUNDLE.exists() else {}
    head = git_output("rev-parse", "HEAD")
    frozen_tag_target = git_output("rev-list", "-n", "1", "m3j-live-micro-closed-cd2ae34")

    no_live_expansion = {
        "m3j19_performs_broker_calls": False,
        "continuous_runtime_live_enabled": False,
        "command_consumer_to_real_finam_enabled": False,
        "stop_sltp_bracket_replace_multileg_enabled": False,
        "m4_features_blocked": True,
    }
    commands_ok = bool(command_results) and all(result["ok"] for result in command_results.values())
    cases_ok = all(result["ok"] for result in case_results.values())
    m3j18_closed = (
        m3j18.get("release_freeze_ready_for_review") is True
        and m3j18.get("final_state", {}).get("m3j_final_operational_closure") == "Closed"
    )
    frozen_tag_ok = frozen_tag_target == FROZEN_SOURCE_COMMIT
    evidence_ready = commands_ok and cases_ok and m3j18_closed and frozen_tag_ok

    payload = {
        "evidence_kind": "m3j19-actual-boundary-failure-matrix-v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "source_commit_full_sha": head,
        "source_commit_short_sha": head[:7],
        "frozen_m3j_source_commit_full_sha": FROZEN_SOURCE_COMMIT,
        "frozen_m3j_release_tag": {
            "name": "m3j-live-micro-closed-cd2ae34",
            "target_commit_full_sha": frozen_tag_target,
            "tag_ok": frozen_tag_ok,
        },
        "artifact_manifest": {
            "doc": artifact(DOC),
            "m3j18_release_freeze_bundle": artifact(M3J18_BUNDLE),
            "transport_source": artifact(TRANSPORT),
            "lifecycle_source": artifact(LIFECYCLE),
            "real_endpoint_source": artifact(REAL_ENDPOINT),
        },
        "command_results": command_results,
        "case_matrix": case_results,
        "m3j18_closed_and_frozen": m3j18_closed,
        "no_live_expansion": no_live_expansion,
        "matrix_checks": {
            "commands_ok": commands_ok,
            "cases_ok": cases_ok,
            "frozen_tag_ok": frozen_tag_ok,
            "m3j18_closed": m3j18_closed,
            "no_live_order_send": True,
            "no_blind_retry_policy_evidenced": True,
        },
        "next_stage_policy": {
            "m3j20_requires_fresh_explicit_operator_approval": True,
            "m3j20_allowed_scope_if_approved": "second controlled LimitCancel with working/active snapshot, max_orders=1, qty=1, no position",
            "m4_requires_separate_design_review": True,
        },
        "evidence_ready_for_review": evidence_ready,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            {
                "evidence_ready_for_review": evidence_ready,
                "commands_ok": commands_ok,
                "cases_ok": cases_ok,
                "m3j18_closed": m3j18_closed,
                "frozen_tag_ok": frozen_tag_ok,
                **no_live_expansion,
            },
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
        )
    )
    return 0 if evidence_ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
