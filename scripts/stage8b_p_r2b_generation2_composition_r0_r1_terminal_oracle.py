#!/usr/bin/env python3
"""Exact typed request-attempt oracle for Generation-2 Phase-6 evidence."""

from __future__ import annotations

import json
import os
import re
import stat
import sys
from datetime import datetime
from pathlib import Path
from typing import Any


ORACLE_ID = "EXACT_TYPED_ROOT_TERMINAL_EVIDENCE"
ALLOWED_REQUEST_ERRORS = ("NETWORK_CONNECT_FAILURE", "TIMEOUT")
EXPECTED_METHOD = "POST"
EXPECTED_ROUTE = "/v1/sessions"
EXPECTED_ORDINAL = 1
EXPECTED_NETWORK_CLASS = "AuthService"
EXPECTED_BODY_CAP = 64 * 1024
HEX64 = re.compile(r"[0-9a-f]{64}")
ROOT_KEYS = {
    "schema_version",
    "stage",
    "admission_commitment_sha256",
    "launcher_executable_sha256",
    "signed_run_package_sha256",
    "helper_executable_sha256",
    "nonce_marker_device",
    "nonce_marker_inode",
    "admission_record_device",
    "admission_record_inode",
    "child_pid",
    "child_exit_code",
    "child_signal",
    "root_terminal_outcome",
    "root_error_category",
    "child_reported_outcome",
    "child_protocol_valid",
    "child_exit_consistent",
    "validated_helper_terminal",
}
HELPER_KEYS = {
    "schema_version",
    "stage",
    "operation",
    "run_nonce_sha256",
    "signed_run_package_sha256",
    "contract_snapshot_sha256",
    "helper_executable_sha256",
    "production_composition_sha256",
    "started_at_utc",
    "finished_at_utc",
    "terminal_outcome",
    "terminal_error_category",
    "terminal_error_detail_redacted",
    "request_attempts",
    "broker_truth_summary",
    "operator_arm_issued",
    "dispatch_attempt_recorded",
    "effect_transport_entered",
    "order_post_sent",
    "order_delete_sent",
    "raw_body_exported",
    "credential_exported",
    "account_id_exported",
}
ATTEMPT_KEYS = {
    "ordinal",
    "network_class",
    "method",
    "route_template",
    "query_policy_id",
    "request_started_at_utc",
    "request_finished_at_utc",
    "status",
    "response_body_length",
    "configured_body_cap",
    "body_overflow",
    "response_stage_error",
    "semantic_receipt_sha256",
    "error_category",
    "timeout_stage",
    "raw_body_exported",
}
LOG_MARKERS = {
    "helper_identity_validation_succeeded": "stage8b-r2b-helper: identity-verified",
    "helper_receipt_validation_succeeded": "stage8b-r2b-helper: receipt-verified",
    "helper_authority_validation_succeeded": "stage8b-r2b-helper: authority-verified",
    "projected_credentials_loaded": "stage8b-r2b-helper: credentials-loaded",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} schema drift")


def utc(value: object, label: str) -> datetime:
    require(isinstance(value, str) and value.endswith("Z"), f"{label} UTC grammar")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise ValueError(f"{label} timestamp invalid") from error
    return parsed


def require_hex64(value: object, label: str) -> None:
    require(isinstance(value, str) and HEX64.fullmatch(value) is not None, f"{label} digest grammar")


def validate_document(
    root: dict[str, Any],
    helper_log: str,
    *,
    durable_evidence: bool,
) -> dict[str, Any]:
    exact_keys(root, ROOT_KEYS, "root terminal")
    require(root["schema_version"] == 1, "root schema version drift")
    require(root["stage"] == "Stage 8B-P R2B root terminal envelope", "root stage drift")
    for key in (
        "admission_commitment_sha256",
        "launcher_executable_sha256",
        "signed_run_package_sha256",
        "helper_executable_sha256",
    ):
        require_hex64(root[key], f"root {key}")
    for key in (
        "nonce_marker_device",
        "nonce_marker_inode",
        "admission_record_device",
        "admission_record_inode",
        "child_pid",
    ):
        require(type(root[key]) is int and root[key] > 0, f"root admission field invalid: {key}")
    require(type(root["child_exit_code"]) is int and root["child_exit_code"] != 0, "child failure exit missing")
    require(root["child_signal"] is None, "child signal terminal is not request evidence")
    require(root["root_terminal_outcome"] == "FAILURE", "root outcome is not fail-closed")
    require(root["child_reported_outcome"] == "FAILURE", "child outcome drift")
    require(root["child_protocol_valid"] is True, "typed child terminal protocol invalid")
    require(root["child_exit_consistent"] is True, "child exit/terminal contradiction")

    helper = root["validated_helper_terminal"]
    require(isinstance(helper, dict), "validated helper terminal missing")
    exact_keys(helper, HELPER_KEYS, "helper terminal")
    require(helper["schema_version"] == 1 and helper["stage"] == "Stage 8B-P R2B", "helper terminal stage drift")
    require(helper["operation"] in {"PLACE", "CANCEL"}, "helper operation drift")
    for key in (
        "run_nonce_sha256",
        "signed_run_package_sha256",
        "contract_snapshot_sha256",
        "helper_executable_sha256",
        "production_composition_sha256",
    ):
        require_hex64(helper[key], f"helper {key}")
    require(helper["signed_run_package_sha256"] == root["signed_run_package_sha256"], "package root/helper binding drift")
    require(helper["helper_executable_sha256"] == root["helper_executable_sha256"], "helper root/helper binding drift")
    require(utc(helper["started_at_utc"], "helper start") <= utc(helper["finished_at_utc"], "helper finish"), "helper chronology drift")
    require(helper["terminal_outcome"] == "FAILURE", "helper outcome is not fail-closed")
    require(helper["broker_truth_summary"] is None, "broker truth present on first failed auth attempt")

    attempts = helper["request_attempts"]
    require(isinstance(attempts, list) and len(attempts) == 1, "exact first failed request attempt missing")
    attempt = attempts[0]
    require(isinstance(attempt, dict), "failed request attempt shape drift")
    exact_keys(attempt, ATTEMPT_KEYS, "failed request attempt")
    require(type(attempt["ordinal"]) is int and attempt["ordinal"] == EXPECTED_ORDINAL, "first attempt ordinal drift")
    require(attempt["network_class"] == EXPECTED_NETWORK_CLASS, "first attempt network class drift")
    require(attempt["method"] == EXPECTED_METHOD, "first attempt method drift")
    require(attempt["route_template"] == EXPECTED_ROUTE, "first attempt route drift")
    require(attempt["query_policy_id"] is None, "auth attempt query policy drift")
    require(utc(attempt["request_started_at_utc"], "attempt start") <= utc(attempt["request_finished_at_utc"], "attempt finish"), "attempt chronology drift")
    require(attempt["status"] is None, "network failure carries HTTP status")
    require(attempt["response_body_length"] is None, "network failure carries response body")
    require(attempt["semantic_receipt_sha256"] is None, "failed attempt carries success receipt")
    require(attempt["configured_body_cap"] == EXPECTED_BODY_CAP, "auth body cap drift")
    require(attempt["body_overflow"] is False, "network failure claims body overflow")
    require(attempt["response_stage_error"] is False, "network failure claims response-stage error")
    require(attempt["raw_body_exported"] is False, "raw body exported")
    error = attempt["error_category"]
    require(error in ALLOWED_REQUEST_ERRORS, "request error category not allowed")
    if error == "TIMEOUT":
        require(isinstance(attempt["timeout_stage"], str) and bool(attempt["timeout_stage"]), "request timeout stage missing")
    else:
        require(attempt["timeout_stage"] is None, "connect failure carries timeout stage")
    require(helper["terminal_error_category"] == error, "helper category is not failed-attempt category")
    require(root["root_error_category"] == error, "root category is not failed-attempt category")
    require(isinstance(helper["terminal_error_detail_redacted"], str) and helper["terminal_error_detail_redacted"], "redacted error detail missing")

    effect_flags = {
        "operator_arm_issued": helper["operator_arm_issued"],
        "dispatch_attempt_recorded": helper["dispatch_attempt_recorded"],
        "effect_transport_entered": helper["effect_transport_entered"],
        "order_post_sent": helper["order_post_sent"],
        "order_delete_sent": helper["order_delete_sent"],
        "raw_body_exported": helper["raw_body_exported"],
        "credential_exported": helper["credential_exported"],
        "account_id_exported": helper["account_id_exported"],
    }
    require(all(value is False for value in effect_flags.values()), "effect or export flag opened")
    log_evidence = {key: marker in helper_log for key, marker in LOG_MARKERS.items()}
    require(all(log_evidence.values()), "helper validation log marker missing")
    require(durable_evidence, "root terminal durability not established")

    actual_read_attempts = bool(attempts)
    return {
        "oracle": ORACLE_ID,
        "category_only_oracle": False,
        "root_admission_succeeded": True,
        "typed_terminal_protocol_valid": True,
        "root_terminal_evidence_durable": True,
        **log_evidence,
        "failed_attempt_required": True,
        "actual_read_attempts": actual_read_attempts,
        "attempt_count": len(attempts),
        "failed_attempt": {
            "ordinal": attempt["ordinal"],
            "network_class": attempt["network_class"],
            "method": attempt["method"],
            "route_template": attempt["route_template"],
            "error_category": error,
            "status": attempt["status"],
            "response_body_length": attempt["response_body_length"],
            "timeout_stage": attempt["timeout_stage"],
        },
        "allowed_request_error_categories": list(ALLOWED_REQUEST_ERRORS),
        "request_timeout_requires_failed_attempt": True,
        "root_lifecycle_timeout": False,
        "effect_flags": effect_flags,
        "broker_dispatch": False,
        "real_order_flags": False,
    }


def validate_files(terminal_path: Path, helper_log_path: Path) -> dict[str, Any]:
    metadata = os.lstat(terminal_path)
    durable = (
        stat.S_ISREG(metadata.st_mode)
        and not stat.S_ISLNK(metadata.st_mode)
        and metadata.st_uid == 0
        and metadata.st_gid == 0
        and stat.S_IMODE(metadata.st_mode) == 0o400
        and metadata.st_nlink == 1
    )
    root = json.loads(terminal_path.read_text(encoding="utf-8"))
    require(isinstance(root, dict), "root terminal is not an object")
    return validate_document(
        root,
        helper_log_path.read_text(encoding="utf-8"),
        durable_evidence=durable,
    )


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: terminal_oracle TERMINAL_JSON HELPER_LOG OUTPUT_JSON")
    terminal_path, helper_log_path, output_path = map(Path, sys.argv[1:])
    if output_path.exists() or not output_path.parent.is_dir():
        raise SystemExit("stage8b-generation2-r0-r1-terminal-oracle: FAIL unsafe output")
    try:
        proof = validate_files(terminal_path, helper_log_path)
    except (KeyError, OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(f"stage8b-generation2-r0-r1-terminal-oracle: FAIL {error}") from error
    output_path.write_text(json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(
        "stage8b-generation2-r0-r1-terminal-oracle: PASS "
        f"request={EXPECTED_METHOD}:{EXPECTED_ROUTE}:{EXPECTED_ORDINAL} "
        f"error={proof['failed_attempt']['error_category']} category_only=false"
    )


if __name__ == "__main__":
    main()
