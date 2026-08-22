#!/usr/bin/env python3
"""Validate the docs/checker-only Stage 8B-D R2 contract."""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(os.environ.get("STAGE8B_ROOT", Path(__file__).resolve().parents[1]))
DOC = ROOT / "docs/stage-8/STAGE8B_DESIGN_2026-08-21.md"
MATRIX = ROOT / "docs/stage-8/STAGE8B_DESIGN_ACCEPTANCE_MATRIX_2026-08-21.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8B_DESIGN_NEGATIVE_INVENTORY_2026-08-21.md"
AUTHORITY = ROOT / "docs/stage-8/stage8b-design-authority.json"
BASE = "7bc9fdab190e011111b15ebdf2f35ff2263a8e34"
ACCEPTED_STAGE8A5 = "bf58b47fdef8af774a4107455dfcc6204e594283"
ACCEPTED_GOV_CI_1B = "13f659f368cbb36a2d38c2b0b88efa376f0b690c"
RETAINED_R1 = "b3358ba2268da3db4eb8352c097495ebb85575d7"
REVIEW_SHA = "72fa3c350dd34aef2d98230dec5547ba25bd7bc752b5b74eedf046e8502b13fc"
BRANCH = "stage8b-d-r2"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-design-check: FAIL {message}")


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def require_true(section: dict[str, Any], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is True, f"required authority weakened: {key}")


def require_false(section: dict[str, Any], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is False, f"forbidden authority opened: {key}")


def require_markers(text: str, markers: tuple[str, ...]) -> None:
    for marker in markers:
        require(marker in text, f"missing contract marker: {marker}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def check(git_scope: bool) -> None:
    for path in (DOC, MATRIX, NEGATIVE, AUTHORITY):
        require(path.is_file(), f"missing file: {path.relative_to(ROOT)}")

    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    expected = {
        "schema_version": 2,
        "stage": "8B-D-R2",
        "status": "design_candidate_independent_review_required",
        "design_base_ref": BASE,
        "accepted_stage8a5_ref": ACCEPTED_STAGE8A5,
        "accepted_stage8a5_review_sha256": REVIEW_SHA,
        "accepted_gov_ci_1b_ref": ACCEPTED_GOV_CI_1B,
        "retained_r1_ref": RETAINED_R1,
        "acceptance_rows": 70,
        "negative_cases": 50,
        "scope": "design_checker_only_single_operator_armed_engineering_effect",
        "next_after_acceptance": "Stage 8B-S implementation specification only",
    }
    for key, value in expected.items():
        require(authority.get(key) == value, f"authority drift: {key}")

    require(
        authority.get("phase_order") == [
            "8B-D design acceptance",
            "8B-S implementation specification acceptance",
            "8B-I no-send implementation and rehearsal acceptance",
            "8B-P read-only preflight and exact run authorization acceptance",
            "8B-X one-shot engineering effect and safe-closure acceptance",
        ],
        "phase order drift",
    )

    run = authority.get("run_contract", {})
    require(run.get("allowed_action_domain") == ["PLACE", "CANCEL"], "action domain drift")
    require(run.get("place_order_type") == "LIMIT", "PLACE type drift")
    require(run.get("place_time_in_force") == "DAY", "PLACE TIF drift")
    require(run.get("max_quantity_lots") == 1, "quantity drift")
    require(run.get("canonical_instrument") == "IMOEXF", "instrument drift")
    require(run.get("venue_symbol") == "IMOEXF@RTSX", "venue drift")
    require_true(run, "exactly_one_command", "action_is_singleton_in_reviewed_run_contract", "side_price_and_notional_exactly_bound")
    require_false(run, "automatic_followup_command_allowed", "limit_cancel_pair_allowed", "market_order_allowed", "protective_or_multi_leg_allowed")

    build = authority.get("build_manifest", {})
    require_true(
        build,
        "execution_qualified_manifest_required",
        "source_commit_and_archive_sha256_required",
        "cargo_lock_sha256_required",
        "cargo_toml_inventory_sha256_required",
        "rustc_vv_and_commit_required",
        "cargo_version_required",
        "target_triple_required",
        "cargo_metadata_graph_sha256_required",
        "complete_feature_set_required",
        "profile_package_target_required",
        "binary_sha256_required",
        "runtime_config_policy_instrument_api_sha256_required",
        "endpoint_and_body_sha256_required",
        "deterministic_aggregate_sha256_required",
        "github_actions_immutable_revision_required_before_protected_evidence",
        "toolchain_immutable_version_required_before_protected_evidence",
    )
    require_false(
        build,
        "legacy_actual_send_feature_broker_cli",
        "legacy_actual_send_feature_finam_gateway",
        "missing_or_unknown_feature_authorizable",
        "alternate_real_transport_path_allowed",
    )

    account = authority.get("account_binding", {})
    require(account.get("hmac_algorithm") == "HMAC-SHA256", "account HMAC algorithm drift")
    require(account.get("domain_separator") == "moex-stage8b-account-binding-v1\\0", "account domain separator drift")
    require(account.get("minimum_operator_key_bits") == 256, "operator key size drift")
    require_true(account, "canonical_account_bytes_exact_utf8", "constant_time_verification_required", "key_generation_id_bound")
    require_false(
        account,
        "fallback_to_plain_digest_allowed",
        "normalization_allowed",
        "operator_key_in_git_or_handoff_allowed",
        "raw_account_id_in_git_or_handoff_allowed",
        "unkeyed_sha256_privacy_binding_allowed",
    )

    arm = authority.get("operator_arm", {})
    require_true(arm, "durable_one_use", "expires_before_transport", "exact_command_identity_required", "exact_build_config_endpoint_and_body_hashes_required", "account_binding_and_key_generation_required")
    require_false(arm, "clone_serialize_default_allowed", "reconstructible_after_restart", "second_arm_for_same_request_allowed")

    preflight = authority.get("preflight", {})
    require_true(
        preflight,
        "read_only_only",
        "fresh_broker_truth_required",
        "fresh_readiness_required",
        "fresh_schedule_required",
        "run_allowed_kill_switch_required",
        "single_broker_ownership_required",
        "zero_ambiguity_required",
        "zero_unresolved_lifecycle_required",
        "read_immediately_before_effect",
        "account_binding_match_required",
        "build_feature_api_contract_match_required",
        "legacy_actual_send_feature_disabled_required",
    )
    require_false(preflight, "caller_supplied_snapshot_allowed")

    durability = authority.get("durability", {})
    require_true(durability, "stage7b_i3_i4_lineage_required", "attempt_before_send_fsync_and_covering_seal_required", "outcome_unknown_requires_reconciliation", "redis_is_not_execution_authority")
    require_false(durability, "transport_may_run_before_attempt_commit", "same_request_automatic_retry_after_transport_boundary")

    closure = authority.get("closure", {})
    require(
        closure.get("durable_states") == [
            "Stage8BClosedSafe",
            "ResidualWorkingOrder",
            "ResidualPosition",
            "OutcomeUnknown",
            "BrokerTruthConflict",
        ],
        "closure state inventory drift",
    )
    require(closure.get("accepted_state") == "Stage8BClosedSafe", "accepted closure drift")
    require_true(
        closure,
        "ambiguity_zero_required",
        "unknown_orphan_zero_required",
        "active_target_orders_zero_required",
        "exact_terminal_target_lifecycle_required",
        "target_position_equals_approved_baseline_required",
        "account_safety_guard_clean_required",
        "journal_seal_outcome_consistent_required",
        "operator_signoff_required",
        "fresh_reconciliation_after_manual_action_required",
        "residual_working_order_new_reviewed_cancel_or_manual_required",
        "residual_position_manual_emergency_disposition_required",
    )
    require_false(closure, "automatic_residual_resolution_allowed", "new_arm_while_blocked_allowed")

    stage11 = authority.get("stage11_promotion", {})
    require(stage11.get("complete_active_sessions_required") == 3, "Stage 11 session count drift")
    require_true(
        stage11,
        "consecutive_trading_days_required",
        "blocking_fix_resets_clean_session_count",
        "recovery_qualification_is_separate",
        "restart_reconnect_gap_recovery_required",
        "alor_execution_owner_oracle_required",
        "finam_post_delete_disabled_required",
        "finam_strategy_invocation_explicitly_enabled_in_paper",
        "frozen_config_hash_required",
        "same_final_m10_decision_boundary_required",
        "zero_unexplained_blocking_divergences_required",
        "characterization_thresholds_frozen_before_series",
        "stage12_blocked_until_independent_acceptance",
    )
    require_false(stage11, "semantic_cli_overrides_allowed")

    coverage = authority.get("reachable_action_coverage", {})
    require_true(
        coverage,
        "frozen_live_config_required",
        "exact_source_oracle_required",
        "machine_readable_inventory_required",
        "accepted_capabilities_must_cover_reachable_actions",
        "market_cancel_and_protective_qualified_if_reachable",
        "repeat_stage11_if_final_config_changes",
        "stage12_blocked_until_complete",
    )
    require_false(coverage, "limit_engineering_effect_qualifies_market_or_protective", "silent_action_or_quantity_rewrite_allowed")

    governance = authority.get("governance", {})
    require(governance.get("branch_protection_or_equivalent_required_before") == "8B-P", "branch protection deadline drift")
    require_true(governance, "gov_ci_1_closed", "reviewed_change_path_required_before_execution")
    require_false(governance, "force_push_for_execution_promotion_allowed")

    network = authority.get("network", {})
    require_true(network, "exact_finam_host_required", "exact_method_and_route_required", "tls_required")
    require_false(network, "redirects_proxies_and_alternate_hosts_allowed", "generic_arbitrary_request_allowed", "transport_retry_allowed")

    closed = authority.get("closed", {})
    require(len(closed) == 15, "closed-surface inventory drift")
    require(all(value is True for value in closed.values()), "closed surface opened")

    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    require([row.get("id") for row in rows] == [f"8BD-{n:03d}" for n in range(1, 71)], "matrix IDs/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") for row in rows), "matrix row incomplete")

    negative_text = NEGATIVE.read_text(encoding="utf-8")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", negative_text, flags=re.MULTILINE)]
    require(numbers == list(range(1, 51)), "negative inventory must be exact 1..50")

    doc = DOC.read_text(encoding="utf-8")
    require_markers(
        doc,
        (
            "docs/checker-only corrective design candidate",
            "Acceptance may open only the separately",
            "A LimitCancel pair is two",
            "m3j16-actual-one-shot = false",
            "Plain unkeyed SHA-256 is not an",
            "moex-stage8b-account-binding-v1\\0",
            "append exact DispatchAttemptRecorded",
            "timeout, disconnect, partial write, response loss",
            "Ambiguous requests are never",
            "Empty, missing, stale or account-wide row\ncounts do not prove absence or flat",
            "Broker truth cannot rewrite durable identity",
            "Stage8BClosedSafe",
            "ResidualWorkingOrder",
            "target_position == approved_pre_run_baseline",
            "automatic second command is",
            "at least three complete active IMOEXF MOEX sessions",
            "A blocking fix resets the clean-session counter",
            "reachable_actions(frozen_imoexf_config)",
            "Silent MARKET-to-LIMIT conversion",
            "Before Stage 8B-P, `main` must have branch protection",
            "Independent R2 acceptance may authorize only Stage 8B-S",
        ),
    )

    if git_scope:
        require(git("branch", "--show-current") == BRANCH, "branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=ROOT, check=True)
        changed = git("diff", "--name-only", BASE, "--").splitlines()
        allowed_exact = {
            "README.md",
            "docs/current-status.md",
            "docs/roadmap.md",
            "docs/stage-8/stage8-slice-plan.md",
            "docs/stage-8/gov-ci-1-authority.json",
        }
        for path in changed:
            require(
                path in allowed_exact
                or path.startswith("docs/stage-8/STAGE8B_")
                or path == "docs/stage-8/stage8b-design-authority.json"
                or path.startswith("scripts/stage8b_")
                or path == "scripts/make_stage8b_design_handoff.py",
                f"design scope widened: {path}",
            )
            require(not path.startswith(("crates/", ".github/")), f"production/workflow delta: {path}")
            require(path not in ("Cargo.toml", "Cargo.lock"), f"Cargo delta: {path}")

    print("stage8b-design-check: PASS rows=70 negatives=50 design_only=true execution=false stage8b_s=false stage12=false")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    check(git_scope=not args.no_git)


if __name__ == "__main__":
    main()
