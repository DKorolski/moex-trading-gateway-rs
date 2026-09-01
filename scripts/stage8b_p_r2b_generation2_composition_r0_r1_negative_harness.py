#!/usr/bin/env python3
"""Targeted R0-R1 evidence mutations required by independent review."""

from __future__ import annotations

import copy
import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Callable

import stage8b_p_r2b_generation2_composition_r0_r1_check as checker
import stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle as oracle


ROOT = Path(__file__).resolve().parents[1]
HELPER_LOG = "\n".join(oracle.LOG_MARKERS.values()) + "\n"


def valid_terminal(error: str = "NETWORK_CONNECT_FAILURE") -> dict[str, object]:
    timeout_stage = "connect" if error == "TIMEOUT" else None
    attempt = {
        "ordinal": 1,
        "network_class": "AuthService",
        "method": "POST",
        "route_template": "/v1/sessions",
        "query_policy_id": None,
        "request_started_at_utc": "2026-09-01T09:00:00Z",
        "request_finished_at_utc": "2026-09-01T09:00:01Z",
        "status": None,
        "response_body_length": None,
        "configured_body_cap": 65536,
        "body_overflow": False,
        "response_stage_error": False,
        "semantic_receipt_sha256": None,
        "error_category": error,
        "timeout_stage": timeout_stage,
        "raw_body_exported": False,
    }
    helper = {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B",
        "operation": "PLACE",
        "run_nonce_sha256": "1" * 64,
        "signed_run_package_sha256": "2" * 64,
        "contract_snapshot_sha256": "3" * 64,
        "helper_executable_sha256": checker.HELPER_SHA256,
        "production_composition_sha256": "4" * 64,
        "started_at_utc": "2026-09-01T09:00:00Z",
        "finished_at_utc": "2026-09-01T09:00:01Z",
        "terminal_outcome": "FAILURE",
        "terminal_error_category": error,
        "terminal_error_detail_redacted": "request boundary failure",
        "request_attempts": [attempt],
        "broker_truth_summary": None,
        "operator_arm_issued": False,
        "dispatch_attempt_recorded": False,
        "effect_transport_entered": False,
        "order_post_sent": False,
        "order_delete_sent": False,
        "raw_body_exported": False,
        "credential_exported": False,
        "account_id_exported": False,
    }
    return {
        "schema_version": 1,
        "stage": "Stage 8B-P R2B root terminal envelope",
        "admission_commitment_sha256": "5" * 64,
        "launcher_executable_sha256": "6" * 64,
        "signed_run_package_sha256": "2" * 64,
        "helper_executable_sha256": checker.HELPER_SHA256,
        "nonce_marker_device": 1,
        "nonce_marker_inode": 2,
        "admission_record_device": 1,
        "admission_record_inode": 3,
        "child_pid": 100,
        "child_exit_code": 1,
        "child_signal": None,
        "root_terminal_outcome": "FAILURE",
        "root_error_category": error,
        "child_reported_outcome": "FAILURE",
        "child_protocol_valid": True,
        "child_exit_consistent": True,
        "validated_helper_terminal": helper,
    }


def assert_positive_oracles() -> None:
    for error in oracle.ALLOWED_REQUEST_ERRORS:
        proof = oracle.validate_document(valid_terminal(error), HELPER_LOG, durable_evidence=True)
        if (
            proof.get("oracle") != checker.ORACLE_ID
            or proof.get("actual_read_attempts") is not True
            or proof.get("failed_attempt", {}).get("error_category") != error
        ):
            raise SystemExit(f"stage8b-generation2-r0-r1-negative: FAIL positive={error}")


def expect_oracle_reject(name: str, mutation: Callable[[dict[str, object]], None]) -> None:
    document = copy.deepcopy(valid_terminal())
    mutation(document)
    try:
        oracle.validate_document(document, HELPER_LOG, durable_evidence=True)
    except (KeyError, TypeError, ValueError):
        print(f"PASS {name}")
        return
    raise SystemExit(f"stage8b-generation2-r0-r1-negative: FAIL accepted={name}")


def helper(document: dict[str, object]) -> dict[str, object]:
    value = document["validated_helper_terminal"]
    assert isinstance(value, dict)
    return value


def attempt(document: dict[str, object]) -> dict[str, object]:
    attempts = helper(document)["request_attempts"]
    assert isinstance(attempts, list) and isinstance(attempts[0], dict)
    return attempts[0]


def auth_without_attempt(document: dict[str, object]) -> None:
    document["root_error_category"] = "AUTH_SESSION_FAILURE"
    helper(document)["terminal_error_category"] = "AUTH_SESSION_FAILURE"
    helper(document)["request_attempts"] = []


def request_attempt_missing(document: dict[str, object]) -> None:
    del helper(document)["request_attempts"]


def root_lifecycle_timeout(document: dict[str, object]) -> None:
    document["child_signal"] = 15
    document["root_error_category"] = "TIMEOUT"


def materialize(destination: Path) -> None:
    for relative in checker.required_files():
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / relative, target)


def mutate_text(root: Path, relative: Path, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"fixture cardinality drift: {relative}: {old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def mutate_json(root: Path, relative: Path, keys: tuple[str, ...], value: object) -> None:
    document = json.loads((root / relative).read_text(encoding="utf-8"))
    cursor = document
    for key in keys[:-1]:
        cursor = cursor[key]
    cursor[keys[-1]] = value
    (root / relative).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")


RootMutation = Callable[[Path], None]
SOURCE_CASES: tuple[tuple[str, RootMutation], ...] = (
    (
        "category-only-terminal-oracle",
        lambda root: mutate_text(
            root,
            checker.MATERIALIZER,
            '''python3 "$repo_root/scripts/stage8b_p_r2b_generation2_composition_r0_r1_terminal_oracle.py" \\
  "$terminal_file" /run/r0-r1a-supervisor.log "$request_boundary_proof"''',
            '''grep -Eq 'NETWORK_CONNECT_FAILURE|AUTH_SESSION_FAILURE' "$terminal_file"''',
        ),
    ),
    (
        "request-timeout-rejected",
        lambda root: mutate_text(
            root,
            checker.ORACLE,
            'ALLOWED_REQUEST_ERRORS = ("NETWORK_CONNECT_FAILURE", "TIMEOUT")',
            'ALLOWED_REQUEST_ERRORS = ("NETWORK_CONNECT_FAILURE",)',
        ),
    ),
    (
        "actual-read-attempts-hardcoded",
        lambda root: mutate_text(
            root,
            checker.MATERIALIZER,
            ' "actual_read_attempts":request_boundary_proof["actual_read_attempts"],',
            ' "actual_read_attempts":True,',
        ),
    ),
    (
        "helper-effect-identity-drift",
        lambda root: mutate_json(
            root,
            checker.r0.HELPER_AUTHORITY,
            ("effect_build_identity_sha256",),
            "0" * 64,
        ),
    ),
)


def run_checker_mutations() -> int:
    artifact_root = checker.r0.resolve_artifact_root(ROOT, None)
    passed = 0
    for name, mutation in SOURCE_CASES:
        with tempfile.TemporaryDirectory(prefix=f"stage8b-g2-r0-r1-{name}-") as temporary:
            root = Path(temporary)
            materialize(root)
            mutation(root)
            try:
                checker.check(root, artifact_root)
            except (
                RuntimeError,
                KeyError,
                IndexError,
                TypeError,
                ValueError,
                OSError,
                json.JSONDecodeError,
                subprocess.SubprocessError,
            ):
                passed += 1
                print(f"PASS {name}")
                continue
            raise SystemExit(f"stage8b-generation2-r0-r1-negative: FAIL accepted={name}")
    return passed


def main() -> None:
    assert_positive_oracles()
    direct_cases: tuple[tuple[str, Callable[[dict[str, object]], None]], ...] = (
        ("auth-session-failure-without-attempt", auth_without_attempt),
        ("failed-attempt-missing", request_attempt_missing),
        ("first-attempt-ordinal-drift", lambda document: attempt(document).__setitem__("ordinal", 2)),
        ("first-attempt-method-drift", lambda document: attempt(document).__setitem__("method", "GET")),
        ("first-attempt-route-drift", lambda document: attempt(document).__setitem__("route_template", "/v1/orders")),
        ("root-lifecycle-timeout-counted-as-request-attempt", root_lifecycle_timeout),
        ("network-failure-with-http-status", lambda document: attempt(document).__setitem__("status", 503)),
        ("effect-flag-opened", lambda document: helper(document).__setitem__("effect_transport_entered", True)),
    )
    for name, mutation in direct_cases:
        expect_oracle_reject(name, mutation)
    total = len(direct_cases) + run_checker_mutations()
    print(
        "stage8b-generation2-r0-r1-negative: PASS "
        f"cases={total}/{total} mandatory=11 effect_identity=1 "
        "network_connect_positive=true request_timeout_positive=true"
    )


if __name__ == "__main__":
    main()
