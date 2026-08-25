#!/usr/bin/env python3
"""Reject the 50 Stage 8B-P R1A corrective contract mutations."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = "scripts/stage8b_p_r1a_authorization_check.py"
AUTHORITY = "docs/stage-8/stage8b-p-r1a-authorization-authority.json"
FRESHNESS = "docs/stage-8/stage8b-p-r1a-freshness-budget-authority.json"
NETWORK = "docs/stage-8/stage8b-p-r1a-network-policy-authority.json"


def mutate(root: Path, path: str, keys: tuple[str, ...], value: Any) -> None:
    target_path = root / path
    document = json.loads(target_path.read_text())
    target = document
    for key in keys[:-1]:
        target = target[key]
    target[keys[-1]] = value
    target_path.write_text(json.dumps(document, indent=2) + "\n")


def edit_list(
    root: Path, path: str, keys: tuple[str, ...], edit: Callable[[list[Any]], None]
) -> None:
    target_path = root / path
    document = json.loads(target_path.read_text())
    target = document
    for key in keys:
        target = target[key]
    edit(target)
    target_path.write_text(json.dumps(document, indent=2) + "\n")


def case(path: str, keys: tuple[str, ...], value: Any) -> Callable[[Path], None]:
    return lambda root: mutate(root, path, keys, value)


def list_case(
    path: str, keys: tuple[str, ...], edit: Callable[[list[Any]], None]
) -> Callable[[Path], None]:
    return lambda root: edit_list(root, path, keys, edit)


def cases() -> list[tuple[str, Callable[[Path], None]]]:
    a = AUTHORITY
    f = FRESHNESS
    n = NETWORK
    return [
        ("r1-authority-digest", case(a, ("lineage", "r1_authority_sha256"), "0" * 64)),
        ("freshness-authority-digest", case(a, ("bound_authorities", "freshness_budget_authority_sha256"), "0" * 64)),
        ("network-authority-digest", case(a, ("bound_authorities", "network_policy_authority_sha256"), "0" * 64)),
        ("execution-build-identity", case(a, ("accepted_execution_build", "execution_build_identity_sha256"), "0" * 64)),
        ("weaker-build-reconstruction", case(a, ("accepted_execution_build", "weaker_subset_reconstruction_allowed"), True)),
        ("process-boot-field-omitted", list_case(a, ("canonical_manifest", "common_required_fields"), lambda x: x.remove("process_boot_fingerprint_sha256"))),
        ("cross-boot-substitution", case(a, ("process_boot_contract", "cross_boot_substitution_fails_closed"), False)),
        ("restart-boot-reuse", case(a, ("process_boot_contract", "restart_reuse_allowed"), True)),
        ("unknown-common-field", list_case(a, ("canonical_manifest", "common_required_fields"), lambda x: x.append("unreviewed_authority"))),
        ("operation-discriminator-expanded", case(a, ("canonical_manifest", "operation_values"), ["PLACE", "CANCEL", "REPLACE"])),
        ("unknown-fields-allowed", case(a, ("canonical_manifest", "unknown_fields_allowed"), True)),
        ("irrelevant-variant-fields-allowed", case(a, ("canonical_manifest", "irrelevant_variant_fields_allowed"), True)),
        ("flat-union-allowed", case(a, ("canonical_manifest", "flat_place_cancel_union_allowed"), True)),
        ("price-target-conflation", case(a, ("canonical_manifest", "conflated_limit_price_or_cancel_target_allowed"), True)),
        ("place-field-omitted", list_case(a, ("canonical_manifest", "place_required_fields"), lambda x: x.pop())),
        ("cancel-field-omitted", list_case(a, ("canonical_manifest", "cancel_required_fields"), lambda x: x.pop())),
        ("place-instrument", case(a, ("place_contract", "instrument"), "RTS-9.26@RTSX")),
        ("place-market", case(a, ("place_contract", "order_type"), "ORDER_TYPE_MARKET")),
        ("place-tif", case(a, ("place_contract", "time_in_force"), "TIME_IN_FORCE_GTC")),
        ("place-quantity", case(a, ("place_contract", "quantity_canonical_decimal"), "2")),
        ("decimal-grammar", case(a, ("place_contract", "canonical_decimal_regex"), ".+")),
        ("noncanonical-decimal", case(a, ("place_contract", "noncanonical_decimal_fails_closed"), False)),
        ("decimal-overflow", case(a, ("place_contract", "decimal_overflow_fails_closed"), False)),
        ("notional-exceedance", case(a, ("place_contract", "price_times_quantity_exceeds_max_notional_fails_closed"), False)),
        ("pre-attempt-notional-check", case(a, ("place_contract", "notional_check_before_attempt_append"), False)),
        ("pre-k4-notional-check", case(a, ("place_contract", "notional_recheck_immediately_before_k4_transport"), False)),
        ("cancel-order-id", case(a, ("cancel_contract", "exact_broker_order_id_required"), False)),
        ("cancel-lifecycle", case(a, ("cancel_contract", "same_durable_lifecycle_required"), False)),
        ("cancel-working-proof", case(a, ("cancel_contract", "currently_working_proof_required"), False)),
        ("cancel-account-wide", case(a, ("cancel_contract", "account_wide_order_selection_allowed"), True)),
        ("endpoint-identity-not-required", case(a, ("endpoint_and_network_contract", "endpoint_identity_sha256_required"), False)),
        ("network-policy-not-required", case(a, ("endpoint_and_network_contract", "network_policy_sha256_required"), False)),
        ("endpoint-formula-not-required", case(a, ("endpoint_and_network_contract", "endpoint_identity_exact_formula_required"), False)),
        ("network-tls-disabled", case(n, ("transport", "tls_required"), False)),
        ("network-host-drift", case(n, ("transport", "exact_host"), "example.invalid")),
        ("network-redirect", case(n, ("transport", "redirects_allowed"), True)),
        ("network-proxy", case(n, ("transport", "proxy_allowed"), True)),
        ("network-retry", case(n, ("transport", "automatic_transport_retry_allowed"), True)),
        ("place-method-substitution", case(n, ("operations", "PLACE", "method"), "DELETE")),
        ("cancel-route-substitution", case(n, ("operations", "CANCEL", "route_template_id"), "PlaceOrderV1")),
        ("caller-selected-freshness", case(f, ("clock_semantics", "caller_selected_budget_allowed"), True)),
        ("runtime-skew-widened", case(f, ("cross_source_budgets", "runtime_current_sources", "max_skew_ms"), 5001)),
        ("api-snapshot-age-widened", case(f, ("source_budgets", "api_snapshot", "max_age_ms"), 86400001)),
        ("issued-arm-field-permitted", list_case(a, ("pre_arm_contract", "issued_arm_shaped_fields_forbidden"), lambda x: x.pop())),
        ("r2-issues-arm", case(a, ("pre_arm_contract", "r2_may_issue_arm"), True)),
        ("r2-equals-k2", case(a, ("r2_k2_separation", "r2_readonly_preflight_evidence_equals_k2_fresh_sources"), True)),
        ("r2-satisfies-k1-k2", case(a, ("r2_k2_separation", "r2_evidence_satisfies_k1_or_k2_freshness"), True)),
        ("r2-carried-to-xe", case(a, ("r2_k2_separation", "r2_evidence_carryable_into_xe_as_current_truth"), True)),
        ("post-arm-reread-removed", case(a, ("r2_k2_separation", "fresh_reread_and_reduction_after_arm_at_k2_required"), False)),
        ("authorization-issued", case(a, ("authorization", "status"), "ISSUED")),
    ]


def main() -> None:
    mutations = cases()
    if len(mutations) != 50:
        raise SystemExit(f"stage8b-p-r1a-authorization-negative: FAIL inventory={len(mutations)}")

    with tempfile.TemporaryDirectory(prefix="stage8b-p-r1a-negative-") as temp:
        root = Path(temp) / "root"
        shutil.copytree(ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports"))
        originals = {path: (root / path).read_bytes() for path in (AUTHORITY, FRESHNESS, NETWORK)}
        for index, (name, mutation) in enumerate(mutations, 1):
            for path, content in originals.items():
                (root / path).write_bytes(content)
            mutation(root)
            result = subprocess.run(
                ["python3", CHECKER, "--no-git"], cwd=root, text=True,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-p-r1a-authorization-negative: FAIL mutation passed: {name}")
            print(f"PASS {index:02d}/50 {name}")

    print("stage8b-p-r1a-authorization-negative: PASS 50/50 inherited_r1=48 total=98")


if __name__ == "__main__":
    main()
