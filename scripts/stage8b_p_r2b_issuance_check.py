#!/usr/bin/env python3
"""Fail-closed checker for the Stage 8B-P R2B issuance-package R0 design."""

from __future__ import annotations

import csv
import json
from pathlib import Path

import stage8b_p_r2b_issuance_systemd_check as systemd_check

ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-authority.json"
MATRIX = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_ACCEPTANCE_MATRIX_2026-08-29.csv"
DESIGN = ROOT / "docs/stage-8/STAGE8B_P_R2B_ISSUANCE_PACKAGE_R0_2026-08-29.md"
EVIDENCE = ROOT / "docs/stage-8/stage8b-p-r2b-issuance-package-r0-evidence.json"
TARGET = ROOT / "deploy/stage8b-r2b/moex-stage8b-r2b-issuance.target"


def require(value: bool, message: str) -> None:
    if not value:
        raise RuntimeError(message)


def main() -> None:
    systemd_check.check(ROOT)
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    design = DESIGN.read_text(encoding="utf-8")
    design_flat = " ".join(design.split())
    evidence = json.loads(EVIDENCE.read_text(encoding="utf-8"))
    rows = list(csv.DictReader(MATRIX.open(encoding="utf-8")))

    require(authority["schema_version"] == 1, "schema drift")
    require(authority["revision"] == "R0", "revision drift")
    require(authority["status"] == "DESIGN_CANDIDATE_NOT_ISSUED", "status opened")
    predecessor = authority["accepted_predecessor"]
    require(
        predecessor["source_ref"] == "f24f1044ac0b29c2f588853b817e519cfe8d3d8b"
        and predecessor["verdict"] == "ACCEPTED",
        "accepted closure binding drift",
    )

    activation = authority["future_activation_target"]
    require(
        activation["implemented_by_r0"] is False
        and activation["installed_by_r0"] is False
        and activation["enabled_by_r0"] is False
        and activation["manual_start_allowed"] is False
        and activation["signed_local_activation_required"] is True,
        "activation target opened",
    )
    require(not TARGET.exists(), "issuance target implemented before R0 acceptance")

    phases = authority["transaction"]["phases"]
    require([phase["ordinal"] for phase in phases] == list(range(1, 7)), "phase order drift")
    flattened = [unit for phase in phases for unit in phase["units"]]
    require(len(flattened) == authority["transaction"]["service_invocation_count"] == 30, "transaction cardinality drift")
    require(len(set(flattened)) == 30, "duplicate transaction unit")
    require(len(phases[2]["units"]) == len(phases[3]["units"]) == 11, "authority fanout drift")
    require(phases[-1]["units"] == ["moex-stage8b-r2b-readonly-supervisor.service"], "terminal supervisor drift")

    contract = authority["systemd_contract"]
    require(
        contract["currently_shipped_unit_file_count"] == len(systemd_check.UNITS) == 9
        and contract["refuse_manual_start_section"] == "Unit"
        and contract["condition_path_is_regular_allowed"] is False
        and contract["unknown_key_or_lvalue_allowed"] is False
        and contract["target_parser_required"] is True
        and contract["binary_fixed_input_validation_authoritative"] is True,
        "systemd contract drift",
    )
    require(tuple(authority["currently_shipped_unit_files"]) == systemd_check.UNITS, "unit inventory drift")

    refresh = authority["fresh_public_contract_refresh"]
    require(
        refresh["observed_on"] == "2026-08-29"
        and refresh["response_count"] == 7
        and refresh["all_http_200"] is True
        and refresh["all_bytes_and_hashes_match"] is True
        and refresh["credentials_used"] is False
        and refresh["broker_endpoint_called"] is False,
        "public contract refresh drift",
    )
    require(
        evidence["public_contract_refresh"]["result"] == "PASS"
        and evidence["systemd_verification"]["result"] == "PASS"
        and evidence["systemd_verification"]["shipped_unit_files"] == 9
        and evidence["systemd_verification"]["systemd_analyze_verify_exit_code"] == 0
        and evidence["systemd_verification"]["unknown_key_warnings"] == 0
        and evidence["systemd_verification"]["unknown_lvalue_warnings"] == 0
        and evidence["systemd_verification"]["units_loaded"] is False
        and evidence["systemd_verification"]["units_started"] is False
        and evidence["targeted_negative_mutations"] == "16/16"
        and evidence["production_rust_or_cargo_changed"] is False,
        "R0 evidence drift",
    )

    local = authority["operator_local_inputs"]
    require(all(str(value).startswith("ABSENT") for value in local.values()), "operator-local authority invented")
    remaining = authority["remaining_preconditions"]
    require(set(remaining.values()) <= {"PENDING", "ABSENT"}, "precondition improperly closed")
    authorization = authority["authorization"]
    require(
        authorization == {
            "r2b": "NOT_ISSUED",
            "operator_arm_issued": False,
            "activation_authority_present": False,
            "target_start_allowed": False,
        },
        "R2B authorization opened",
    )
    require(all(value is False for value in authority["closed_surfaces"].values()), "effect surface opened")
    for name in (
        "finam_credentials_accessed", "auth_service_called", "broker_account_get_sent",
        "order_post_sent", "order_delete_sent", "dispatch_attempt_recorded",
        "transport_entered", "redis_live_consumer", "broker_dispatch",
        "runtime_live", "real_orders",
    ):
        require(evidence[name] is False, f"evidence opened: {name}")
    require(evidence["authorization_status"] == "NOT_ISSUED", "evidence issued R2B")

    require(len(rows) == authority["acceptance_rows"] == 25, "acceptance row count drift")
    require(all(row["expected"] == "PASS" for row in rows), "acceptance expectation drift")
    for marker in (
        "NOT_ISSUED", "30 service invocations", "ConditionPathIsRegular=",
        "does not create an activation target", "No credentials", "order POST/DELETE",
    ):
        require(marker in design_flat, f"design marker absent: {marker}")

    print(
        "stage8b-p-r2b-issuance-check: PASS revision=R0 rows=25 "
        "service_invocations=30 shipped_units=9 target_implemented=false "
        "operator_selection=ABSENT authorization=NOT_ISSUED finam=false post_delete=false"
    )


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as error:
        raise SystemExit(f"stage8b-p-r2b-issuance-check: FAIL {error}") from error
