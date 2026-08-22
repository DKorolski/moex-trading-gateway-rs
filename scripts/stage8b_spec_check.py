#!/usr/bin/env python3
"""Validate the docs/checker-only corrective Stage 8B-S R3 contract."""

from __future__ import annotations

import argparse
import csv
import hashlib
import hmac
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
R2_AUTHORITY = ROOT / "docs/stage-8/stage8b-design-authority.json"
BRANCH = "stage8b-s-r3"
R1 = "a675a772e02fa6da1a33973127542696019eb2f7"
R2 = "831eec8f830fa57e4ada8c135d803c34bea29298"
MAIN_PREDECESSOR = "50ed5382fdbe2d62ed253d65a312f951e2a267ff"


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


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_hmac_vector(privacy: dict[str, Any]) -> None:
    require(privacy.get("account_binding") == "HMAC-SHA256", "account algorithm drift")
    require(privacy.get("domain_ascii") == "moex-stage8b-account-binding-v1", "domain ASCII drift")
    require(privacy.get("domain_suffix_hex") == "00", "domain suffix must be exact NUL hex")
    require(privacy.get("length_encoding") == "u32be", "account length encoding drift")
    require(privacy.get("minimum_key_bits") == 256, "account key size drift")
    vector = privacy.get("golden_vector", {})
    try:
        key = bytes.fromhex(vector["key_hex"])
        account = bytes.fromhex(vector["account_utf8_hex"])
        declared_message = bytes.fromhex(vector["message_hex"])
        declared_digest = vector["expected_hmac_sha256"]
        domain = privacy["domain_ascii"].encode("ascii")
        suffix = bytes.fromhex(privacy["domain_suffix_hex"])
    except (KeyError, TypeError, ValueError, UnicodeEncodeError) as error:
        fail(f"invalid HMAC vector: {error}")
    require(len(key) == 32, "golden key must be exact 256-bit test key")
    message = domain + suffix + len(account).to_bytes(4, "big") + account
    require(message == declared_message, "golden encoded message mismatch")
    require(hmac.new(key, message, hashlib.sha256).hexdigest() == declared_digest, "golden HMAC mismatch")


def check(git_scope: bool) -> None:
    for path in (DOC, MATRIX, NEGATIVE, AUTHORITY, R2_AUTHORITY):
        require(path.is_file(), f"missing file: {path.relative_to(ROOT)}")
    authority = json.loads(AUTHORITY.read_text(encoding="utf-8"))
    require(authority.get("schema_version") == 3, "schema drift")
    require(authority.get("stage") == "8B-S-R3", "stage drift")
    require(authority.get("status") == "corrective_specification_checker_only_candidate", "status drift")
    require(authority.get("branch") == BRANCH, "branch authority drift")

    retained_r2 = authority.get("retained_stage8b_s_r2", {})
    require(retained_r2.get("source_ref") == R2, "S R2 source drift")
    require(retained_r2.get("handoff_sha256") == "66a54c1948ca09cd06c97a19a7759a44c03522f47557c636e16fb1ff19d13f6d", "S R2 handoff drift")
    require(retained_r2.get("review_sha256") == "48c37291df87453ce342dd32dfc6e91d6d7630b1ea586c8dd51db048933861e6", "S R2 review drift")

    retained = authority.get("retained_stage8b_s_r1", {})
    require(retained.get("source_ref") == R1, "S R1 source drift")
    require(retained.get("handoff_sha256") == "5f875e1cbd2ae9491f1ae4c50d53022f8949dfbabc5a81240ce1a2ef7124f570", "S R1 handoff drift")
    require(retained.get("review_sha256") == "04ffd2f762cb0bb054d4435736dfbc68ab6b238c2eeb2739a3a9fe8df9fa425f", "S R1 review drift")
    predecessor = authority.get("accepted_stage8b_d", {})
    require(predecessor.get("candidate_ref") == "f296d0be782b8aa550a20e27600ba16826214349", "R2 candidate drift")
    require(predecessor.get("merge_ref") == MAIN_PREDECESSOR, "R2 merge drift")
    require(predecessor.get("accepted_tree") == "f40e2e5f40d7e3ed1dd5f5a252832734265094df", "R2 tree drift")
    require(predecessor.get("authority_sha256") == "83e85722fcf41e6abdd215569c4337f6c83994baeafbae47c5ad80bb9e935d09", "R2 authority digest drift")
    require(predecessor.get("handoff_sha256") == "ac351d9c03c98d59e90affeb423dbb7fff2cd3722b3d601889c53ae90c6cc06b", "R2 handoff drift")
    require(predecessor.get("review_sha256") == "ba624781b59741aae1c59acbf430f897c7c591ac78aecc9e0a0463883ffacaa0", "R2 review drift")
    require(sha256(R2_AUTHORITY) == predecessor.get("authority_sha256"), "R2 authority content drift")
    require_true(authority, "additive_over_exact_stage8b_d")
    require_false(authority, "s_fields_may_weaken_or_override_stage8b_d")
    require(authority.get("accepted_stage8a5_ref") == "bf58b47fdef8af774a4107455dfcc6204e594283", "Stage 8A5 ref drift")
    require(authority.get("accepted_gov_ci_merge_ref") == "7bc9fdab190e011111b15ebdf2f35ff2263a8e34", "GOV ref drift")
    require(authority.get("phase_order") == ["8B-D", "8B-S", "8B-I", "8B-IT", "8B-P", "8B-XE"], "phase order drift")
    require(authority.get("next_if_accepted") == "8B-I_no_send_implementation_and_crash_replay", "next stage drift")

    facade = authority.get("public_operator_facade", {})
    require(facade.get("crate") == "finam-gateway", "facade crate drift")
    require(facade.get("name") == "invoke_stage8b_operator_once", "facade name drift")
    require(facade.get("input_type") == "Stage8bOperatorInvocationRequest", "facade input drift")
    require(facade.get("output_type") == "Stage8bOperatorDiagnostic", "facade output drift")
    require(facade.get("accepted_input_fields") == ["invocation_id", "accepted_run_package_path", "local_manifest_root"], "facade input surface drift")
    require_false(facade, "authority_bearing", "raw_account_url_method_header_body_token_client_transport_allowed", "capability_or_arm_input_output_allowed")
    require_true(facade, "returns_redacted_diagnostic_only", "positive_cross_crate_fixture_required_in_8b_i")
    root = authority.get("private_composition_root", {})
    require(root.get("crate") == "finam-gateway" and root.get("name") == "compose_stage8b_effect_authority" and root.get("visibility") == "pub(crate)", "private root drift")
    require_true(root, "single_root_required", "consumes_stage8a1_current_capability", "consumes_stage7b_durable_authority", "parallel_transport_forbidden", "runtime_dependency_forbidden", "compile_fail_privacy_required_in_8b_i")
    require_false(root, "cross_crate_accessible")

    expected_types = ["Stage8bExecutionQualifiedBuild", "Stage8bKeyedAccountBinding", "Stage8bFreshContractAuthority", "Stage8bAcceptedRunSpec", "Stage8bK1ControlApproved", "Stage8bOperatorArm", "Stage8bFreshPreflightApproved", "Stage8bSealedAttemptCommitted", "Stage8bExactTransportPermit", "Stage8bPossibleEffectOwner", "Stage8bDurableClosureOwner", "Stage8bClosureReceipt"]
    require(authority.get("linear_types") == expected_types, "linear type inventory drift")
    require(authority.get("forbidden_traits") == ["Clone", "Copy", "Default", "Debug", "Serialize", "Deserialize"], "forbidden trait drift")

    build = authority.get("causal_build", {})
    require_true(build, "build_from_extracted_accepted_archive", "archive_member_and_mode_verification", "pre_and_post_build_tree_verification", "offline_build_after_dependency_preparation", "cargo_lock_and_all_manifests_bound", "canonical_metadata_projection_excludes_local_paths", "resolved_feature_graph_required")
    require(build.get("exact_toolchain_fields") == ["cargo_version", "toolchain_channel", "target_triple", "rustc_release", "rustc_commit_hash", "rustc_commit_date", "rustc_host", "rustc_llvm_version", "profile", "package", "binary_target", "binary_sha256"], "toolchain field drift")
    require_false(build, "broker_cli_m3j16_actual_one_shot", "finam_gateway_m3j16_actual_one_shot", "unknown_feature_state_authorizable")

    privacy = authority.get("privacy", {})
    check_hmac_vector(privacy)
    require(privacy.get("endpoint_identity_components") == ["method", "route_template_id", "keyed_account_binding", "endpoint_renderer_sha256"], "endpoint identity drift")
    require_true(privacy, "exact_utf8_no_normalization", "constant_time_verification")
    require_false(privacy, "plain_digest_fallback", "rendered_path_sha256_publishable", "raw_account_export", "secret_key_export")

    run = authority.get("run_contract", {})
    require(run.get("max_effects") == 1 and run.get("allowed_actions") == ["PLACE", "CANCEL"], "effect/action drift")
    require(run.get("place_order_type") == "LIMIT" and run.get("place_tif") == "DAY" and run.get("max_lots") == 1 and run.get("instrument") == "IMOEXF@RTSX", "PLACE scope drift")
    notional = run.get("max_notional", {})
    require(notional.get("representation") == "canonical_exact_decimal_string", "max notional representation drift")
    require_true(notional, "required_in_accepted_run_spec", "checked_before_attempt_recording", "checked_immediately_before_transport")
    require_true(run, "cancel_same_durable_lifecycle", "cancel_requires_currently_working", "silent_rewrite_forbidden")

    network = authority.get("network_policy", {})
    require(network.get("exact_host") == "api.finam.ru" and network.get("place_method") == "POST" and network.get("place_route_template") == "PlaceOrderV1" and network.get("cancel_method") == "DELETE" and network.get("cancel_route_template") == "CancelOrderV1", "network allowlist drift")
    require_true(network, "tls_required")
    require_false(network, "redirects_allowed", "proxy_allowed", "alternate_host_allowed", "arbitrary_request_api_allowed", "automatic_transport_retry_allowed")

    arm = authority.get("arm_durability", {})
    require(arm.get("states") == ["NeverIssued", "IssuedUnconsumed", "Consumed", "AttemptCommitted", "Closed"], "arm state drift")
    require(arm.get("uniqueness_key_fields") == ["durable_request_id", "client_order_id", "accepted_run_sha256", "keyed_account_binding"], "arm uniqueness key drift")
    require_true(arm, "issuance_append_fsync_covering_seal_required", "durable_one_use", "restart_reconstructs_observation_reconciliation_only", "expiry_before_transport_required", "exact_command_build_config_policy_endpoint_body_account_run_bound")
    require_false(arm, "second_arm_same_request_allowed", "restart_reconstructs_arm_or_send_authority")
    require(authority.get("kill_switch_chronology") == ["K1_fresh_control_before_arm_issuance", "DurableArmIssuedForExactRun", "K2_exact_arm_preflight_owns_arm", "AttemptAppendFsyncCoveringSeal", "K3_after_seal", "K4_immediately_before_transport_write", "K5_before_post_effect_continuation"], "K1/K2 chronology drift")
    preflight = authority.get("k2_preflight", {})
    require_true(preflight, "cannot_exist_before_exact_arm", "owns_exact_arm_and_run_identity", "single_finam_execution_owner_required", "zero_ambiguity_required", "zero_unresolved_lifecycle_required", "fresh_broker_truth_required", "fresh_readiness_required", "fresh_schedule_required")
    require_false(preflight, "arm_substitution_after_k2_allowed", "reuse_with_second_arm_allowed", "restart_reconstructs_preflight", "caller_built_or_cached_authority_allowed")

    require(authority.get("freshness_sources") == ["trusted_clock", "readiness", "current_control", "ownership", "schedule", "instrument", "account_orders", "positions", "trades", "exact_order", "api_snapshot"], "freshness source drift")
    require(authority.get("freshness_budgets_frozen_before") == "8B-P" and authority.get("historical_ack_implies_current_readiness") is False, "freshness authority drift")
    require(authority.get("crash_windows") == ["BeforeAttempt", "AttemptCommittedNoTransport", "PossibleSendNoResponse", "ResponseNoDurableOutcome", "DurableOutcomeNoPublication", "RestartAtEveryBoundary"], "crash window drift")
    recovery = authority.get("recovery_rules", {})
    require(recovery.get("response_no_durable_outcome") == "broker_truth_only_never_resend" and recovery.get("durable_outcome_no_publication") == "settlement_publication_only_never_resend", "recovery rule drift")
    require_false(recovery, "automatic_retry", "automatic_cleanup", "broker_truth_may_rewrite_identity")

    seams = authority.get("stage8a_successor_seams", {})
    require(seams.get("stage8a2_accepted_ref") == "16180ac4f8eab761b3b055c1f5515f62cd94bfb9" and seams.get("stage8a2_source_sha256") == "1026a24962bf45de8653c80ba095f892af35523da58f4fa4fccad706fb023653", "Stage 8A2 pin drift")
    require(seams.get("builder_bridge") == "compose_stage8b_private_request_parts_from_stage8a2" and seams.get("existing_builders_only") == ["build_place_order_request", "build_cancel_order_request"], "builder seam drift")
    require_false(seams, "independent_serializer_allowed", "classifier_output_is_execution_truth", "second_or_third_classifier_allowed")
    require_true(seams, "raw_specs_getters_and_body_remain_private", "all_possible_effects_reconcile_from_broker_truth")
    require(seams.get("stage8a3_model") == "A_reuse_accepted_classifier" and seams.get("stage8a3_accepted_ref") == "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d" and seams.get("stage8a3_source_sha256") == "f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3", "Stage 8A3 pin drift")
    require(seams.get("classifier_bridge") == "classify_stage8b_transport_observation_with_stage8a3", "classifier seam drift")

    require(authority.get("closure_states") == ["Stage8BClosedSafe", "ResidualWorkingOrder", "ResidualPosition", "OutcomeUnknown", "BrokerTruthConflict"], "closure state drift")
    stage11 = authority.get("stage11", {})
    require(stage11.get("minimum_complete_active_sessions") == 3, "Stage 11 count drift")
    require_true(stage11, "consecutive_trading_days_after_last_blocking_fix", "blocking_change_resets_series", "representative_lifecycle_coverage_required", "deterministic_replay_for_unobserved_reachable_paths", "separate_recovery_qualification", "alor_sole_execution_owner_oracle", "finam_post_delete_and_dispatch_disabled", "same_final_m10_decision_boundary_required", "zero_unexplained_blocking_divergences_required", "oracle_source_build_binary_config_profile_hash_bound", "calendar_and_exclusions_frozen_before_series")
    require_false(stage11, "no_activity_session_sufficient", "semantic_cli_overrides_allowed")

    adapter = authority.get("real_adapter_review", {})
    require(adapter.get("qualification_name") == "8B-IT" and adapter.get("xe_name") == "8B-XE", "adapter phase names drift")
    require_true(adapter, "qualification_no_broker_effect", "qualification_independently_accepted_before_exact_8b_p", "qualification_local_controlled_non_broker_tests", "qualification_permit_only_reachability", "p_build_sha_equals_accepted_adapter_build_sha", "p_source_equals_accepted_adapter_source", "p_executable_equals_accepted_adapter_executable", "p_endpoint_renderer_and_body_schema_equal_accepted_adapter", "xe_first_possible_broker_effect", "post_p_drift_invalidates_p", "material_drift_requires_adapter_requalification_where_relevant", "material_drift_requires_fresh_contract_preflight_and_new_p", "xe_requires_exact_p_bound_build")
    require_false(adapter, "p_issued_before_adapter_qualification_allowed", "adapter_review_and_first_effect_same_event_allowed", "automatic_p_refresh_or_authority_carry_over_allowed")
    require(adapter.get("accepted_adapter_identity_fields") == ["source_commit", "source_archive_sha256", "cargo_manifests_sha256", "cargo_lock_sha256", "resolved_feature_graph_sha256", "resolved_dependency_graph_sha256", "toolchain_identity_sha256", "config_policy_sha256", "instrument_identity_sha256", "api_snapshot_sha256", "endpoint_renderer_sha256", "request_body_schema_sha256", "executable_sha256"], "adapter-qualified identity inventory drift")
    require(adapter.get("post_p_drift_fields") == ["source", "cargo_manifests", "cargo_lock", "resolved_feature_graph", "toolchain", "dependencies", "config", "policy", "instrument", "api_snapshot", "endpoint_renderer", "request_body_schema", "executable"], "post-P drift inventory drift")
    promotion = authority.get("promotion_gates", {})
    require(promotion.get("fresh_official_finam_contract_before") == "8B-P" and promotion.get("branch_protection_or_equivalent_before") == "8B-P" and promotion.get("immutable_action_and_toolchain_pins_before") == "8B-P", "promotion deadline drift")
    require_true(promotion, "material_contract_drift_blocks")

    closed = authority.get("closed_surfaces", {})
    require(len(closed) == 15 and all(value is True for value in closed.values()), "closed surface opened")
    require(authority.get("acceptance_matrix_rows") == 110 and authority.get("negative_cases") == 100, "count authority drift")
    with MATRIX.open(newline="", encoding="utf-8") as stream:
        rows = list(csv.DictReader(stream))
    require([row.get("id") for row in rows] == [f"S-{number:03d}" for number in range(1, 111)], "matrix IDs/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") and row.get("status") == "pending" for row in rows), "matrix row incomplete")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", NEGATIVE.read_text(encoding="utf-8"), flags=re.MULTILINE)]
    require(numbers == list(range(1, 101)), "negative inventory must be exact 1..100")

    doc = DOC.read_text(encoding="utf-8")
    for marker in ("corrective specification/checker-only candidate", "strictly additive over the exact accepted Stage 8B-D R2 authority", "invoke_stage8b_operator_once", "compose_stage8b_effect_authority", "cannot contain account IDs, URL", "K2 cannot be minted before the exact durable arm exists", "domain `moex-stage8b-account-binding-v1`, suffix hex `00`", "message_hex = 6d6f6578", "max_notional", "exact network policy", "Durable one-use arm record", "one FINAM execution owner", "ALOR is the sole execution owner/oracle", "compose_stage8b_private_request_parts_from_stage8a2", "classify_stage8b_transport_observation_with_stage8a3", "Classifier output is candidate/diagnostic evidence", "8B-D → 8B-S → 8B-I → 8B-IT → 8B-P → 8B-XE", "P package issued before adapter", "Automatic P refresh", "Stage 8B-S R3 keeps closed"):
        require(marker in doc, f"missing contract marker: {marker}")

    if git_scope:
        require(git("branch", "--show-current") == BRANCH, "branch drift")
        subprocess.run(["git", "merge-base", "--is-ancestor", R2, "HEAD"], cwd=ROOT, check=True)
        for path in git("diff", "--name-only", R2, "--").splitlines():
            require(not path.startswith(("crates/", ".github/workflows/")), f"production/workflow delta: {path}")
            require(path not in ("Cargo.toml", "Cargo.lock"), f"Cargo delta: {path}")
            require(path.startswith(("docs/", "scripts/")) or path == "README.md", f"spec scope widened: {path}")

    print("stage8b-spec-check: PASS rows=110 negatives=100 corrective_specification=true implementation=false execution=false stage8b_i=false stage8b_it=false stage8b_p=false stage8b_xe=false stage12=false")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-git", action="store_true")
    args = parser.parse_args()
    check(git_scope=not args.no_git)


if __name__ == "__main__":
    main()
