#!/usr/bin/env python3
"""Validate the Stage 8B-P R2A GET-only preparation contract."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
AUTHORITY = ROOT / "docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json"
R1B = ROOT / "docs/stage-8/stage8b-p-r1b-authorization-authority.json"
NETWORK = ROOT / "docs/stage-8/stage8b-p-r1b-network-endpoint-authority.json"
RUN = ROOT / "docs/stage-8/stage8b-p-r1b-run-identity-authority.json"
MATRIX = ROOT / "docs/stage-8/STAGE8B_P_R2A_ACCEPTANCE_MATRIX_2026-08-25.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8B_P_R2A_NEGATIVE_INVENTORY_2026-08-25.md"
DESIGN = ROOT / "docs/stage-8/STAGE8B_P_R2A_READONLY_PREFLIGHT_CONTRACT_2026-08-25.md"
PREPARE = ROOT / "scripts/stage8b_p_r2a_prepare.py"
MERGE_REF = "f1070a428c884f846ed3a2007e38f2401b62e5ce"
R1B_REF = "b9a423c4ffd96bf4a5f69027aa4fef4dcc503830"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"stage8b-p-r2a-check: FAIL {message}")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    authority = json.loads(AUTHORITY.read_text())

    require(authority.get("stage") == "8B-P" and authority.get("revision") == "R2A", "stage drift")
    require(authority.get("status") == "GET_ONLY_PREFLIGHT_CONTRACT_CANDIDATE", "status drift")
    lineage = authority["lineage"]
    require(lineage["accepted_main_merge_ref"] == MERGE_REF, "merge ref drift")
    require(lineage["accepted_r1b_ref"] == R1B_REF, "R1B ref drift")
    require(lineage["r1b_authority_sha256"] == sha(R1B), "R1B authority drift")
    require(lineage["r1b_network_authority_sha256"] == sha(NETWORK), "network authority drift")
    require(lineage["r1b_run_authority_sha256"] == sha(RUN), "run authority drift")

    executable = authority["qualified_executable"]
    require(executable["build_identity_sha256"] == "ca330934df540de69b52d0463a665a1ab0ff89fa13eeb663b162763cc6bc83a0", "build drift")
    require(executable["executable_sha256"] == "677f277defb2591011486a061cb251264e3fd05bbc9f684b3ec9ff6ae55f3f06", "executable drift")
    require(executable["target"] == "aarch64-apple-darwin" and executable["command"] == "finam-real-readonly-evidence", "executable contract drift")
    require(executable["rebuild_allowed"] is False and executable["alternate_executable_allowed"] is False, "executable substitution enabled")

    selection = authority["operator_selection"]
    require(selection["operation_enum"] == ["PLACE", "CANCEL"], "operation enum drift")
    require(selection["instrument"] == "IMOEXF@RTSX", "instrument drift")
    require((selection["place_order_type"], selection["place_time_in_force"], selection["place_quantity"]) == ("ORDER_TYPE_LIMIT", "TIME_IN_FORCE_DAY", "1"), "PLACE policy drift")
    for key in ("required_outside_repository", "cancel_exact_broker_order_id_required", "cancel_same_lifecycle_required", "unknown_fields_forbidden", "raw_account_forbidden_in_evidence", "token_forbidden_in_selection"):
        require(selection[key] is True, f"selection protection weakened: {key}")

    transport = authority["readonly_transport"]
    require(transport["method_allowlist"] == ["GET"], "method allowlist drift")
    require(transport["source_order"] == ["GetOrder", "OrdersSnapshot", "TradesSnapshot", "PositionSnapshot"], "source order drift")
    require(transport["route_templates"] == ["/v1/accounts/{account_id}/orders/{order_id}", "/v1/accounts/{account_id}/orders", "/v1/accounts/{account_id}/trades", "/v1/accounts/{account_id}"], "route drift")
    require((transport["max_requests"], transport["request_timeout_ms"], transport["min_request_interval_ms"], transport["preflight_max_age_ms"]) == (4, 10000, 250, 60000), "numeric bound drift")
    for key in ("retry_disabled", "redirect_disabled", "proxy_disabled", "background_loop_disabled", "scheduler_disabled", "redacted_evidence_only"):
        require(transport[key] is True, f"transport protection weakened: {key}")
    require(transport["raw_response_exported"] is False, "raw response export enabled")

    required = authority["required_current_inputs"]
    require(len(required) == 17 and len(set(required)) == 17, "current input inventory drift")
    for name in ("stage7b_current_recovery_seal", "stage6_exact_dispatch_ready_command", "kill_switch_run_allowed", "single_finam_ownership", "account_orders", "positions", "trades", "ambiguity_orphan_unresolved_lifecycle", "target_instrument_pre_run_position"):
        require(name in required, f"required current input missing: {name}")

    semantics = authority["evidence_semantics"]
    require(semantics["type_name"] == "R2ReadOnlyPreflightEvidence" and semantics["not_equal_to"] == "Stage8bK2FreshSources", "R2/K2 separation drift")
    for key, value in semantics.items():
        if key not in ("type_name", "not_equal_to"):
            require(value is True, f"evidence protection weakened: {key}")

    execution = authority["r2a_execution"]
    require(execution == {"operator_selection_present": False, "credential_used": False, "token_details_get_sent": False, "broker_get_sent": False, "readonly_http_request_count": 0, "r2_run_evidence_present": False, "r2b_actual_get_unlocked": False}, "R2A execution surface opened")
    authorization = authority["authorization"]
    require(authorization["status"] == "NOT_ISSUED", "authorization issued")
    require(all(value is False for key, value in authorization.items() if key != "status"), "effect surface opened")
    promotion = authority["promotion"]
    require(promotion["next_if_independently_accepted"] == "Stage8B-P-R2B exact operator-selected GET-only evidence run", "promotion drift")
    require(all(value is True for key, value in promotion.items() if key != "next_if_independently_accepted"), "promotion prerequisite weakened")

    with MATRIX.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    require(len(rows) == 48 and [row["id"] for row in rows] == [f"P2A-{index:03d}" for index in range(1, 49)], "matrix drift")
    require(all(row["status"] == "PASS" for row in rows), "matrix not green")
    require(authority["acceptance_rows"] == 48, "matrix count authority drift")
    require(len(re.findall(r"^\d+\. ", NEGATIVE.read_text(), re.MULTILINE)) == 40 and authority["negative_mutations"] == 40, "negative count drift")
    for phrase in ("No broker request was sent", "R2ReadOnlyPreflightEvidence != Stage8bK2FreshSources", "separate R2B", "Authorization remains `NOT_ISSUED`"):
        require(phrase in DESIGN.read_text(), f"design statement missing: {phrase}")

    source = PREPARE.read_text()
    for forbidden in ("import requests", "import urllib", "import subprocess", "FINAM_SECRET_TOKEN", "FINAM_ACCOUNT_ID", "POST", "DELETE"):
        require(forbidden not in source, f"prepare helper opened surface: {forbidden}")
    result = subprocess.run(["python3", str(PREPARE), "--self-test"], cwd=ROOT, text=True, capture_output=True)
    require(result.returncode == 0 and "PASS self_test=2/2" in result.stdout, "prepare self-test failed")

    if not args.no_git:
        require(subprocess.run(["git", "merge-base", "--is-ancestor", MERGE_REF, "HEAD"], cwd=ROOT).returncode == 0, "accepted merge not ancestor")
        changed = subprocess.run(["git", "diff", "--name-only", MERGE_REF, "--", "Cargo.toml", "Cargo.lock", "crates", "config", ".github/workflows"], cwd=ROOT, text=True, capture_output=True, check=True).stdout.splitlines()
        require(not changed, f"production/config/workflow drift: {changed}")

    print("stage8b-p-r2a-check: PASS rows=48 negatives=40 plan_only=true broker_get=false arm=false attempt=false effect_transport=false finam_post_delete=false authorization=NOT_ISSUED")


if __name__ == "__main__":
    main()
