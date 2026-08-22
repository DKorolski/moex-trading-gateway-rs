#!/usr/bin/env python3
"""Validate the specification-only Stage 8B-S authority contract."""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(os.environ.get("STAGE8B_SPEC_ROOT", Path(__file__).resolve().parents[1]))
DOC = ROOT / "docs/stage-8/STAGE8B_IMPLEMENTATION_SPEC_2026-08-22.md"
MATRIX = ROOT / "docs/stage-8/STAGE8B_SPEC_ACCEPTANCE_MATRIX_2026-08-22.csv"
NEGATIVE = ROOT / "docs/stage-8/STAGE8B_SPEC_NEGATIVE_INVENTORY_2026-08-22.md"
AUTHORITY = ROOT / "docs/stage-8/stage8b-spec-authority.json"
BRANCH = "stage8b-s"
BASE = "50ed5382fdbe2d62ed253d65a312f951e2a267ff"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-spec-check: FAIL {message}")


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def require_true(section: dict[str, Any], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is True, f"required authority weakened: {key}")


def require_false(section: dict[str, Any], *keys: str) -> None:
    for key in keys:
        require(section.get(key) is False, f"forbidden authority opened: {key}")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def check(git_scope: bool) -> None:
    for path in (DOC, MATRIX, NEGATIVE, AUTHORITY):
        require(path.is_file(), f"missing file: {path.relative_to(ROOT)}")

    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 1, "schema drift")
    require(authority.get("stage") == "8B-S", "stage drift")
    require(authority.get("status") == "specification_checker_only_candidate", "status drift")
    require(authority.get("branch") == BRANCH, "branch authority drift")
    require(authority.get("accepted_stage8a5_ref") == "bf58b47fdef8af774a4107455dfcc6204e594283", "Stage 8A5 ref drift")
    require(authority.get("accepted_gov_ci_merge_ref") == "7bc9fdab190e011111b15ebdf2f35ff2263a8e34", "GOV merge ref drift")
    predecessor = authority.get("accepted_stage8b_d", {})
    require(predecessor.get("candidate_ref") == "f296d0be782b8aa550a20e27600ba16826214349", "R2 candidate drift")
    require(predecessor.get("merge_ref") == BASE, "R2 merge drift")
    require(predecessor.get("accepted_tree") == "f40e2e5f40d7e3ed1dd5f5a252832734265094df", "R2 tree drift")
    require(predecessor.get("handoff_sha256") == "ac351d9c03c98d59e90affeb423dbb7fff2cd3722b3d601889c53ae90c6cc06b", "R2 handoff drift")
    require(predecessor.get("review_sha256") == "ba624781b59741aae1c59acbf430f897c7c591ac78aecc9e0a0463883ffacaa0", "R2 review drift")
    require(authority.get("phase_order") == ["8B-D", "8B-S", "8B-I", "8B-P", "8B-X"], "phase order drift")
    require(authority.get("next_if_accepted") == "8B-I_no_send_implementation_and_crash_replay", "next stage drift")

    composition = authority.get("sole_composition", {})
    require(composition.get("crate") == "finam-gateway", "composition crate drift")
    require(composition.get("visibility") == "pub(crate)", "composition visibility drift")
    require(composition.get("public_output") == "redacted_diagnostic_only", "public output drift")
    require_true(composition, "consumes_stage8a1_current_capability", "consumes_stage7b_durable_authority", "parallel_transport_forbidden", "runtime_dependency_forbidden")

    expected_types = [
        "Stage8bExecutionQualifiedBuild", "Stage8bKeyedAccountBinding",
        "Stage8bFreshContractAuthority", "Stage8bAcceptedRunSpec",
        "Stage8bOperatorArm", "Stage8bFreshPreflightApproved",
        "Stage8bAttemptCommitOwner", "Stage8bSealedAttemptCommitted",
        "Stage8bExactTransportPermit", "Stage8bPossibleEffectOwner",
        "Stage8bDurableClosureOwner", "Stage8bClosureReceipt",
    ]
    require(authority.get("linear_types") == expected_types, "linear type inventory drift")
    require(authority.get("forbidden_traits") == ["Clone", "Copy", "Default", "Debug", "Serialize", "Deserialize"], "forbidden trait inventory drift")

    build = authority.get("causal_build", {})
    require_true(build, "build_from_extracted_accepted_archive", "archive_member_and_mode_verification", "pre_and_post_build_tree_verification", "offline_build_after_dependency_preparation", "cargo_lock_and_all_manifests_bound", "toolchain_target_profile_binary_bound", "canonical_metadata_projection_excludes_local_paths", "resolved_feature_graph_required")
    require_false(build, "broker_cli_m3j16_actual_one_shot", "finam_gateway_m3j16_actual_one_shot", "unknown_feature_state_authorizable")

    privacy = authority.get("privacy", {})
    require(privacy.get("account_binding") == "HMAC-SHA256", "account algorithm drift")
    require(privacy.get("domain") == "moex-stage8b-account-binding-v1\\u0000", "account domain drift")
    require(privacy.get("minimum_key_bits") == 256, "account key size drift")
    require(privacy.get("endpoint_identity_components") == ["method", "route_template_id", "keyed_account_binding", "endpoint_renderer_sha256"], "endpoint identity drift")
    require_true(privacy, "exact_utf8_no_normalization", "constant_time_verification")
    require_false(privacy, "plain_digest_fallback", "rendered_path_sha256_publishable", "raw_account_export", "secret_key_export")

    run = authority.get("run_contract", {})
    require(run.get("max_effects") == 1, "effect budget drift")
    require(run.get("allowed_actions") == ["PLACE", "CANCEL"], "action inventory drift")
    require(run.get("place_order_type") == "LIMIT", "order type drift")
    require(run.get("place_tif") == "DAY", "TIF drift")
    require(run.get("max_lots") == 1, "quantity drift")
    require(run.get("instrument") == "IMOEXF@RTSX", "instrument drift")
    require_true(run, "cancel_same_durable_lifecycle", "cancel_requires_currently_working", "silent_rewrite_forbidden")

    require(authority.get("kill_switch_boundaries") == [
        "BeforeOperatorArm", "FinalFreshPreflightBeforeAttempt",
        "AfterFsyncAndCoveringSeal", "ImmediatelyBeforeTransportWrite",
        "BeforePostEffectContinuation",
    ], "kill-switch boundary drift")
    require(authority.get("freshness_sources") == [
        "trusted_clock", "readiness", "current_control", "ownership", "schedule",
        "instrument", "account_orders", "positions", "trades", "exact_order",
        "api_snapshot",
    ], "freshness source drift")
    require(authority.get("freshness_budgets_frozen_before") == "8B-P", "freshness deadline drift")
    require(authority.get("historical_ack_implies_current_readiness") is False, "historical readiness opened")
    require(authority.get("crash_windows") == [
        "BeforeAttempt", "AttemptCommittedNoTransport", "PossibleSendNoResponse",
        "ResponseNoDurableOutcome", "DurableOutcomeNoPublication",
        "RestartAtEveryBoundary",
    ], "crash window drift")

    recovery = authority.get("recovery_rules", {})
    require(recovery.get("response_no_durable_outcome") == "broker_truth_only_never_resend", "response recovery drift")
    require(recovery.get("durable_outcome_no_publication") == "settlement_publication_only_never_resend", "publication recovery drift")
    require_false(recovery, "automatic_retry", "automatic_cleanup", "broker_truth_may_rewrite_identity")
    require(authority.get("closure_states") == ["Stage8BClosedSafe", "ResidualWorkingOrder", "ResidualPosition", "OutcomeUnknown", "BrokerTruthConflict"], "closure state drift")

    stage11 = authority.get("stage11", {})
    require(stage11.get("minimum_complete_active_sessions") == 3, "Stage 11 count drift")
    require_true(stage11, "consecutive_trading_days_after_last_blocking_fix", "representative_lifecycle_coverage_required", "deterministic_replay_for_unobserved_reachable_paths", "separate_recovery_qualification", "oracle_source_build_binary_config_profile_hash_bound", "calendar_and_exclusions_frozen_before_series")
    require_false(stage11, "no_activity_session_sufficient")

    promotion = authority.get("promotion_gates", {})
    require(promotion.get("fresh_official_finam_contract_before") == "8B-P", "contract refresh deadline drift")
    require(promotion.get("branch_protection_or_equivalent_before") == "8B-P", "governance deadline drift")
    require(promotion.get("immutable_action_and_toolchain_pins_before") == "8B-P", "immutable pin deadline drift")
    require_true(promotion, "material_contract_drift_blocks")

    closed = authority.get("closed_surfaces", {})
    require(len(closed) == 13 and all(value is True for value in closed.values()), "closed surface opened")
    require(authority.get("acceptance_matrix_rows") == 80, "matrix authority count drift")
    require(authority.get("negative_cases") == 60, "negative authority count drift")

    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    require([row.get("id") for row in rows] == [f"S-{number:03d}" for number in range(1, 81)], "matrix IDs/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") and row.get("status") == "pending" for row in rows), "matrix row incomplete")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", NEGATIVE.read_text(encoding="utf-8"), flags=re.MULTILINE)]
    require(numbers == list(range(1, 61)), "negative inventory must be exact 1..60")

    doc = DOC.read_text(encoding="utf-8")
    for marker in (
        "specification/checker-only candidate", "Stage8a1CurrentlyAuthorizedCapability",
        "Stage8bExactTransportPermit", "builds from that extracted root",
        "canonical cargo-metadata projection", "No artifact publishes SHA-256 of a rendered path",
        "K1`: immediately before operator-arm issuance", "K5`: before post-effect continuation",
        "Historical ACK, historical readiness", "AttemptCommittedNoTransport",
        "ResponseNoDurableOutcome", "DurableOutcomeNoPublication",
        "recover from broker truth only, never resend", "settlement/publication only",
        "A complete elapsed session with no representative lifecycle activity",
        "official FINAM contract is fetched again", "Stage 8B-I may add only no-send",
        "Stage 8B-S keeps closed: production implementation",
    ):
        require(marker in doc, f"missing contract marker: {marker}")

    if git_scope:
        require(git("branch", "--show-current") == BRANCH, "branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", BASE, "HEAD"], cwd=ROOT, check=True)
        for path in git("diff", "--name-only", BASE, "--").splitlines():
            require(not path.startswith(("crates/", ".github/workflows/")), f"production/workflow delta: {path}")
            require(path not in ("Cargo.toml", "Cargo.lock"), f"Cargo delta: {path}")
            require(path.startswith(("docs/", "scripts/")) or path == "README.md", f"spec scope widened: {path}")

    print("stage8b-spec-check: PASS rows=80 negatives=60 specification=true implementation=false execution=false stage8b_i=false stage8b_p=false stage8b_x=false stage12=false")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    check(git_scope=not args.no_git)


if __name__ == "__main__":
    main()
