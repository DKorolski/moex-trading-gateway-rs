#!/usr/bin/env python3
"""Reject the 48 Stage 8B-P R1 design-only authorization mutations."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r1_authorization_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r1-authorization-authority.json"


def mutate(root: Path, keys: tuple[str, ...], value: Any) -> None:
    path = root / AUTHORITY
    document = json.loads(path.read_text())
    target = document
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    path.write_text(json.dumps(document, indent=2) + "\n")


def delete_manifest_field(root: Path) -> None:
    path = root / AUTHORITY
    document = json.loads(path.read_text())
    document["exact_run_manifest_required_fields"].pop()
    path.write_text(json.dumps(document, indent=2) + "\n")


def add_manifest_field(root: Path) -> None:
    path = root / AUTHORITY
    document = json.loads(path.read_text())
    document["exact_run_manifest_required_fields"].append("unreviewed_authority")
    path.write_text(json.dumps(document, indent=2) + "\n")


def case(keys: tuple[str, ...], value: Any) -> Callable[[Path], None]:
    return lambda root: mutate(root, keys, value)


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    return [
        ("predecessor-ref", case(("accepted_predecessor", "main_ref"), "06a59bca74f94881c70d9fa39bbdf1c357e65f95")),
        ("predecessor-tree", case(("accepted_predecessor", "main_tree"), "0c613dbf15858671eb6a0e5ee1435a2bc2b9f172")),
        ("gov-status", case(("accepted_predecessor", "gov_p1_status"), "PENDING")),
        ("preconditions-hash", case(("accepted_predecessor", "preconditions_authority_sha256"), "0" * 64)),
        ("governance-hash", case(("accepted_predecessor", "governance_observation_sha256"), "0" * 64)),
        ("source-ref", case(("accepted_transport_build", "source_ref"), "0" * 40)),
        ("source-archive", case(("accepted_transport_build", "source_archive_sha256"), "0" * 64)),
        ("executable", case(("accepted_transport_build", "executable_sha256"), "0" * 64)),
        ("target", case(("accepted_transport_build", "target_triple"), "x86_64-unknown-linux-gnu")),
        ("rust-release", case(("accepted_transport_build", "rust_release"), "stable")),
        ("broker-cli-legacy-send", case(("accepted_transport_build", "legacy_actual_send_feature_broker_cli"), True)),
        ("gateway-legacy-send", case(("accepted_transport_build", "legacy_actual_send_feature_finam_gateway"), True)),
        ("production-drift", case(("accepted_transport_build", "production_code_drift_since_qualification"), True)),
        ("fresh-contract-hash", case(("fresh_contract", "snapshot_sha256"), "0" * 64)),
        ("response-count", case(("fresh_contract", "official_response_count"), 6)),
        ("non-200", case(("fresh_contract", "all_http_200"), False)),
        ("contract-hash-drift", case(("fresh_contract", "all_hashes_identical_to_accepted_contract"), False)),
        ("material-contract-drift", case(("fresh_contract", "material_contract_drift"), True)),
        ("credentials-used", case(("fresh_contract", "credentials_used"), True)),
        ("broker-get-sent", case(("fresh_contract", "broker_readonly_get_sent"), True)),
        ("order-request-sent", case(("fresh_contract", "finam_order_request_sent"), True)),
        ("manifest-field-removed", delete_manifest_field),
        ("manifest-field-added", add_manifest_field),
        ("operation-count", case(("future_exact_run_policy", "operation_count"), 2)),
        ("operation-market", case(("future_exact_run_policy", "allowed_operations"), ["PLACE", "CANCEL", "MARKET"])),
        ("instrument", case(("future_exact_run_policy", "place_instrument"), "RTS-9.26@RTSX")),
        ("order-type", case(("future_exact_run_policy", "place_order_type"), "ORDER_TYPE_MARKET")),
        ("time-in-force", case(("future_exact_run_policy", "place_time_in_force"), "TIME_IN_FORCE_GTC")),
        ("quantity", case(("future_exact_run_policy", "place_max_quantity"), "2")),
        ("cancel-unbound", case(("future_exact_run_policy", "cancel_requires_exact_working_order_same_lifecycle"), False)),
        ("market-open", case(("future_exact_run_policy", "market_allowed"), True)),
        ("protective-open", case(("future_exact_run_policy", "stop_sltp_bracket_allowed"), True)),
        ("retry-open", case(("future_exact_run_policy", "automatic_retry_allowed"), True)),
        ("resend-open", case(("future_exact_run_policy", "same_request_resend_allowed"), True)),
        ("limitcancel-open", case(("future_exact_run_policy", "limit_cancel_pair_in_one_run_allowed"), True)),
        ("cached-snapshot", case(("future_get_only_preflight", "caller_built_or_cached_snapshot_allowed"), True)),
        ("preflight-issues-arm", case(("future_get_only_preflight", "preflight_may_issue_operator_arm"), True)),
        ("preflight-records-attempt", case(("future_get_only_preflight", "preflight_may_record_dispatch_attempt"), True)),
        ("preflight-enters-transport", case(("future_get_only_preflight", "preflight_may_enter_transport_boundary"), True)),
        ("arm-issued", case(("operator_arm_contract", "issued_by_this_package"), True)),
        ("arm-constructible", case(("operator_arm_contract", "constructible_by_this_package"), True)),
        ("arm-not-one-shot", case(("operator_arm_contract", "one_shot"), False)),
        ("arm-serializable", case(("operator_arm_contract", "clone_copy_serialize_allowed"), True)),
        ("arm-reconstructible", case(("operator_arm_contract", "reconstructible_after_restart"), True)),
        ("authorization-issued", case(("authorization", "status"), "ISSUED")),
        ("stage8b-p-open", case(("authorization", "stage8b_p_open"), True)),
        ("broker-effect-open", case(("closed_surfaces", "broker_effect"), False)),
        ("skip-r2", case(("authorization", "next_if_accepted"), "Stage8B-XE")),
    ]


def main() -> None:
    mutations = cases()
    if len(mutations) != 48:
        raise SystemExit(f"stage8b-p-r1-authorization-negative: FAIL inventory={len(mutations)}")
    with tempfile.TemporaryDirectory(prefix="stage8b-p-r1-negative-") as temp:
        base = Path(temp) / "base"
        shutil.copytree(ROOT, base, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
        for index, (name, mutation) in enumerate(mutations, 1):
            root = Path(temp) / f"case-{index:02d}"
            shutil.copytree(base, root)
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"],
                cwd=root,
                text=True,
                capture_output=True,
            )
            if result.returncode == 0:
                raise SystemExit(
                    f"stage8b-p-r1-authorization-negative: FAIL mutation passed: {name}"
                )
            print(f"PASS {index:02d}/48 {name}")
    print("stage8b-p-r1-authorization-negative: PASS 48/48")


if __name__ == "__main__":
    main()
