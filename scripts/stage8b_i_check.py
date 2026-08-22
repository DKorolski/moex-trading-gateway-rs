#!/usr/bin/env python3
"""Fail-closed source/authority checker for Stage 8B-I no-send implementation."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
from pathlib import Path


S_CANDIDATE = "afecc2584593570b62cbe7f00ee81f64d4b9b26b"
S_MERGE = "d1581962666aa82b993854d0642e67bd66624032"
S_TREE = "f9cfdd2c53f3659c6610fb282ae9a024fd2c56d6"
S_AUTHORITY_SHA256 = "7650e529498dbc5adfccd646878d43909c062cedc49c957bbf17c60f53d0ca1a"
A2_SHA256 = "1026a24962bf45de8653c80ba095f892af35523da58f4fa4fccad706fb023653"
A3_SHA256 = "f34c9fef5e219dad15b0a00ce1eaf63311ec9f77d1997e422b977e5c8ffe47b3"


def fail(message: str) -> None:
    raise SystemExit(f"stage8b-i-check: FAIL {message}")


def require(value: bool, message: str) -> None:
    if not value:
        fail(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.root.resolve()
    paths = {
        "authority": root / "docs/stage-8/stage8b-i-authority.json",
        "contract": root / "docs/stage-8/STAGE8B_I_IMPLEMENTATION_2026-08-22.md",
        "matrix": root / "docs/stage-8/STAGE8B_I_ACCEPTANCE_MATRIX_2026-08-22.csv",
        "negative": root / "docs/stage-8/STAGE8B_I_NEGATIVE_INVENTORY_2026-08-22.md",
        "s_authority": root / "docs/stage-8/stage8b-spec-authority.json",
        "module": root / "crates/finam-gateway/src/stage8b_no_send.rs",
        "gateway_lib": root / "crates/finam-gateway/src/lib.rs",
        "gateway_cargo": root / "crates/finam-gateway/Cargo.toml",
        "cargo_lock": root / "Cargo.lock",
        "cli_lib": root / "crates/broker-cli/src/lib.rs",
        "cli_test": root / "crates/broker-cli/tests/stage8b_i_no_send_facade.rs",
        "a2": root / "crates/finam-gateway/src/stage8a1_execution_capability/stage8a2_builder_composition.rs",
        "a1": root / "crates/finam-gateway/src/stage8a1_execution_capability.rs",
        "a3": root / "crates/finam-gateway/src/stage8a3_endpoint_classifier.rs",
        "compile": root / "scripts/stage8b_i_external_compile_fail.sh",
        "closed": root / "scripts/stage8b_i_closed_surface_check.py",
        "gate": root / "scripts/stage8b_i_gate.sh",
        "full_regression": root / "scripts/stage8b_i_full_regression.sh",
        "handoff_safety": root / "scripts/stage8b_i_handoff_safety_check.py",
        "handoff_maker": root / "scripts/make_stage8b_i_handoff.py",
    }
    for label, path in paths.items():
        require(path.is_file(), f"missing {label}: {path}")
    text = {
        label: path.read_text(encoding="utf-8")
        for label, path in paths.items()
        if label not in {"matrix"}
    }
    authority = json.loads(text["authority"])
    require(authority.get("schema_version") == 2, "schema drift")
    require(authority.get("stage") == "8B-I-R2", "stage drift")
    require(authority.get("status") == "corrective_no_send_implementation_candidate", "status drift")
    require(authority.get("branch") == "stage8b-i-r2", "branch drift")
    require(authority.get("rejected_stage8b_i_ref") == "a52fbcae5340d632ce8b983eda6ecb4b8dedabce", "rejected I ref drift")
    require(authority.get("stage8b_i_review_sha256") == "3f7b04caa6b402ab96432560c5ef5f48c7a0e77bbbc87c466c85054f15216399", "I review digest drift")
    require(authority.get("accepted_stage8b_s_r3_candidate") == S_CANDIDATE, "S candidate drift")
    require(authority.get("accepted_stage8b_s_r3_merge") == S_MERGE, "S merge drift")
    require(authority.get("accepted_stage8b_s_r3_tree") == S_TREE, "S tree drift")
    require(authority.get("accepted_stage8b_s_authority_sha256") == S_AUTHORITY_SHA256, "S authority pin drift")
    require(sha256(paths["s_authority"]) == S_AUTHORITY_SHA256, "S authority content drift")
    require(authority.get("accepted_stage8a2_ref") == "16180ac4f8eab761b3b055c1f5515f62cd94bfb9", "A2 ref drift")
    require(authority.get("accepted_stage8a2_source_sha256") == A2_SHA256 and sha256(paths["a2"]) == A2_SHA256, "A2 source drift")
    require(authority.get("accepted_stage8a3_ref") == "012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d", "A3 ref drift")
    require(authority.get("accepted_stage8a3_source_sha256") == A3_SHA256 and sha256(paths["a3"]) == A3_SHA256, "A3 source drift")

    required_true = (
        "single_public_facade", "single_private_root", "positive_broker_cli_integration",
        "authority_types_crate_private", "authority_fields_private",
        "authority_clone_copy_debug_serde_forbidden", "existing_stage8a2_builders_only",
        "accepted_stage8a3_model_a_only", "hmac_sha256_exact_domain",
        "hmac_constant_time_verify", "secret_zeroization", "absolute_paths_required",
        "symlink_components_rejected", "single_link_regular_files_required",
        "nofollow_descriptor_open", "descriptor_path_identity_recheck", "path_swap_negative",
        "manifest_openat_child", "bounded_evidence_reads", "durable_arm_o_excl",
        "durable_arm_file_and_directory_fsync", "cross_process_single_winner_test",
        "impossible_replay_sequence_rejected",
        "stage8a2_builder_after_exact_permit_only", "permit_consumed_by_builder_bridge",
        "local_no_network_boundary_single_use", "durable_request_consumed_and_bound",
        "k2_fresh_sources_typed_and_bound", "canonical_lower_arm_identity",
        "authenticated_complete_arm_binding", "all_closure_classes_persist_exactly",
        "closure_payload_corruption_rejected", "execution_build_provenance_verified",
        "resolved_legacy_send_features_false", "unknown_feature_state_rejected",
        "endpoint_identity_exactly_bound", "canonical_full_regression_required",
        "k2_max_one_budget_enforced", "k3_covering_seal_bound",
        "k4_exact_attempt_rechecked", "k5_reconciliation_bound",
        "publication_preserves_exact_closure", "terminal_closure_receipt_typed",
    )
    for key in required_true:
        require(authority.get(key) is True, f"required property weakened: {key}")
    required_false = (
        "automatic_retry_or_resend", "public_facade_constructs_authority",
        "real_adapter_present", "finam_post_delete_enabled", "network_send_enabled",
        "redis_execution_enabled", "ack_readiness_publication_enabled",
        "broker_dispatch_enabled", "runtime_live_enabled", "real_orders_enabled",
        "stage8b_it_enabled", "stage8b_p_enabled", "stage8b_xe_enabled", "stage12_enabled",
    )
    for key in required_false:
        require(authority.get(key) is False, f"closed surface opened: {key}")
    require(authority.get("external_compile_fail_positive_cases") == 1, "compile positive count drift")
    require(authority.get("external_compile_fail_negative_cases") == 18, "compile negative count drift")
    require(authority.get("hmac_domain_ascii") == "moex-stage8b-account-binding-v1", "HMAC domain drift")
    require(authority.get("hmac_suffix_hex") == "00", "HMAC suffix drift")
    require(authority.get("hmac_length_encoding") == "u32be", "HMAC length drift")
    require(authority.get("hmac_minimum_key_bytes") == 32, "HMAC key size drift")
    require(authority.get("hmac_golden_digest") == "60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435", "HMAC golden drift")
    require(authority.get("kill_boundary_count") == 5, "kill-boundary count drift")
    require(authority.get("crash_window_count") == 6, "crash-window count drift")
    require(authority.get("durable_restart_prefix_count") == 6, "restart-prefix count drift")
    require(authority.get("closure_class_count") == 5, "closure count drift")
    require(authority.get("acceptance_rows") == 92 and authority.get("negative_cases") == 70, "authority count drift")

    with paths["matrix"].open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    require([row.get("id") for row in rows] == [f"I-{index:03d}" for index in range(1, 93)], "matrix ID/count drift")
    require(all(row.get("area") and row.get("requirement") and row.get("evidence") and row.get("status") == "pending" for row in rows), "matrix row incomplete")
    numbers = [int(value) for value in re.findall(r"^(\d+)\.", text["negative"], re.MULTILINE)]
    require(numbers == list(range(1, 71)), "negative inventory must be exact 1..70")

    module = text["module"]
    require(len(re.findall(r"(?m)^pub fn invoke_stage8b_operator_once\(", module)) == 1, "public facade count drift")
    require(len(re.findall(r"(?m)^pub\(crate\) fn compose_stage8b_effect_authority\(", module)) == 1, "private root count drift")
    require(len(re.findall(r"(?m)^fn compose_stage8b_private_request_parts_from_stage8a2\(", module)) == 1, "builder bridge count drift")
    require(len(re.findall(r"(?m)^fn classify_stage8b_transport_observation_with_stage8a3\(", module)) == 1, "classifier bridge count drift")
    require(len(re.findall(r"(?m)^fn commit_stage8b_sealed_attempt\(", module)) == 1, "sealed-attempt transition drift")
    require(len(re.findall(r"(?m)^fn authorize_stage8b_exact_transport_permit\(", module)) == 1, "exact-permit transition drift")
    require(len(re.findall(r"(?m)^fn invoke_stage8b_local_no_network_boundary\(", module)) == 1, "local no-network boundary drift")
    require(len(re.findall(r"(?m)^fn reconcile_stage8b_possible_effect\(", module)) == 1, "K5 reconciliation transition drift")
    require(len(re.findall(r"(?m)^fn publish_stage8b_durable_closure\(", module)) == 1, "closure publication transition drift")
    for marker in (
        "permit.capability.compose_stage8a2_no_send(&mut sink)", "context.classify(observation)",
        "commit_stage8b_sealed_attempt", "authorize_stage8b_exact_transport_permit",
        "invoke_stage8b_local_no_network_boundary", ".into_stage8b_binding_sha256()",
        "Stage8bK2FreshSources", "single_finam_owner", "ambiguity_count != 0",
        "unresolved_lifecycle_count != 0", "readiness_fresh", "schedule_open_and_fresh",
        "broker_truth_fresh", "max_one_budget_remaining != 1",
        "Stage8bK3CoveringSealApproved", "Stage8bK4ControlApproved",
        "Stage8bK5ReconciliationApproved", "reconcile_stage8b_possible_effect",
        "publish_stage8b_durable_closure", "verify_execution_qualified_build", "canonical_metadata_sha256",
        "resolved_feature_graph_sha256", "unknown_feature_count != 0",
        "compose_endpoint_identity", "PlaceOrderV1", "CancelOrderV1",
        "Stage8bCanonicalBindingDigest", "STAGE8B-I-R2-ARM-V1",
        "Stage8bIssuedArmRecord", "Stage8bAuthenticatedOperatorArm",
        "verify_rehearsal_arm_record", "validate_authenticated_arm_for_k2",
        "authenticated_record_sha256", "verified_at_unix_ms",
        "STAGE8B-I-R2-ARM-CONSUMED-V1", "Stage8bArmIssueError::AlreadyConsumed",
        "stage8b-i-r2-durable-arm-consumed-v1",
        "Hmac::<Sha256>::new_from_slice", "b\"moex-stage8b-account-binding-v1\"",
        "message.push(0)", "length.to_be_bytes()", "mac.verify_slice(expected)",
        "Zeroizing<Vec<u8>>", "libc::O_NOFOLLOW", "libc::O_EXCL", "libc::openat(",
        "path_before.nlink() != 1", "EvidenceIdentityDrift", "file.sync_all()",
        "directory.sync_all()", "two_processes_cannot_issue_two_arms",
        "durable_rehearsal_reopens_every_crash_boundary_without_resend",
        "durable_rehearsal_rejects_impossible_or_corrupt_sequence",
        "every_closure_class_survives_pre_and_post_publication_restart",
        "corrupt_unknown_or_mismatched_closure_payload_fails_closed",
        "execution_build_verifier_binds_source_features_metadata_toolchain_and_binary",
        "endpoint_identity_binds_method_template_account_and_renderer",
        "arm_binding_changes_for_each_exact_durable_run_and_k2_component",
        "k2_accepts_only_fresh_authenticated_arm_capability",
        "package_path_swap_after_open_is_rejected", "manifest_child_symlink_is_rejected_by_openat",
    ):
        require(marker in module, f"implementation marker missing: {marker}")
    require(
        "stage8b_binding_changes_with_exact_durable_request_authority" in text["a1"],
        "durable-request cross-binding fixture missing",
    )
    for forbidden in (
        "reqwest::", "redis::", ".post(", ".delete(", ".request(", ".send(",
        "xadd(", "xack(", "TcpStream", "automatic_retry", "resend_authority",
    ):
        require(forbidden not in module, f"forbidden no-send surface: {forbidden}")

    root_match = re.search(
        r"pub\(crate\) fn compose_stage8b_effect_authority\((?P<args>.*?)\) -> Result<Stage8bFreshPreflightApproved.*?\n\}",
        module,
        re.S,
    )
    require(root_match is not None, "private root body missing")
    root_body = root_match.group(0)
    for required in ("capability", "durable", "build", "account", "contract", "run", "control", "arm", "k2_sources"):
        require(re.search(rf"(?m)^\s{{4}}{required}:\s", root_body) is not None, f"K2 root input missing: {required}")
    require("compose_stage8b_private_request_parts_from_stage8a2" not in root_body, "builder invoked before permit")
    require(
        "validate_authenticated_arm_for_k2(&arm, &expected_arm_binding, &k2_sources)?;" in root_body,
        "authenticated arm validation missing from K2 root",
    )
    for required in (
        "if !k2_sources.single_finam_owner", "k2_sources.ambiguity_count != 0",
        "k2_sources.unresolved_lifecycle_count != 0", "!k2_sources.readiness_fresh",
        "!k2_sources.schedule_open_and_fresh", "!k2_sources.broker_truth_fresh",
        "k2_sources.max_one_budget_remaining != 1",
        "k2_sources.observed_at_unix_ms == 0",
    ):
        require(required in root_body, f"K2 fail-closed predicate missing: {required}")
    preflight_match = re.search(r"pub\(crate\) struct Stage8bFreshPreflightApproved\s*\{(?P<body>.*?)\n\}", module, re.S)
    require(preflight_match is not None, "fresh preflight type missing")
    require("capability: Stage8a1CurrentlyAuthorizedCapability" in preflight_match.group("body"), "fresh preflight lost exact continuation")
    require("request_parts" not in preflight_match.group("body"), "fresh preflight contains pre-permit request parts")
    require("struct Stage8bApprovedRequestParts {" in module and "pub struct Stage8bApprovedRequestParts" not in module, "private request witness escaped")
    require("DurableOutcomeRecorded(Stage8bClosureClassification)" in module, "durable closure payload removed")
    require("PublicationRecorded(Stage8bClosureClassification)" in module, "publication closure payload removed")
    require("(b'a'..=b'f').contains(&byte)" in module, "canonical lowercase digest check weakened")
    bridge_match = re.search(
        r"fn compose_stage8b_private_request_parts_from_stage8a2\(\s*permit: Stage8bExactTransportPermit,.*?\n\}",
        module,
        re.S,
    )
    require(bridge_match is not None, "builder bridge does not consume exact permit")
    require("pub(crate) fn into_stage8b_binding_sha256(" in text["a1"], "durable Stage8A1 binding bridge missing")
    require("self.validate()?;" in text["a1"] and "stage8b-durable-request-binding-v1" in text["a1"], "durable authority not validated/bound")

    transition_contracts = {
        "K3": (
            r"fn commit_stage8b_sealed_attempt\(.*?\n\}",
            ("k3: Stage8bK3CoveringSealApproved", "append(Stage8bRehearsalRecord::AttemptCommitted)",
             "k3.seal_sha256.as_bytes()", "k3.control_sha256.as_bytes()"),
        ),
        "K4": (
            r"fn authorize_stage8b_exact_transport_permit\(.*?\n\}",
            ("k4: Stage8bK4ControlApproved", "k4.rechecked_attempt_sha256 != sealed.attempt_sha256",
             "k4.control_sha256.as_bytes()"),
        ),
        "K5": (
            r"fn reconcile_stage8b_possible_effect\(.*?\n\}",
            ("k5: Stage8bK5ReconciliationApproved", "Stage8bRehearsalRecord::ResponseObserved",
             "Stage8bRehearsalRecord::DurableOutcomeRecorded(k5.closure)",
             "k5.broker_truth_sha256.as_bytes()", "k5.control_sha256.as_bytes()",
             "k5.closure.code().as_bytes()"),
        ),
        "publication": (
            r"fn publish_stage8b_durable_closure\(.*?\n\}",
            ("Stage8bDurableClosureOwner", "Stage8bRehearsalRecord::PublicationRecorded(owner.closure)",
             "owner.closure_sha256.as_bytes()", "owner.closure.code().as_bytes()"),
        ),
    }
    for label, (pattern, required_markers) in transition_contracts.items():
        match = re.search(pattern, module, re.S)
        require(match is not None, f"{label} transition body missing")
        for marker in required_markers:
            require(marker in match.group(0), f"{label} transition binding missing: {marker}")

    endpoint_match = re.search(
        r"fn compose_endpoint_identity\(.*?\n\}", module, re.S
    )
    require(endpoint_match is not None, "endpoint identity verifier missing")
    endpoint_body = endpoint_match.group(0)
    for required in ("method", "route", "account.binding_sha256.as_bytes()", "endpoint_renderer_sha256.as_bytes()"):
        require(required in endpoint_body, f"endpoint identity component missing: {required}")
    build_match = re.search(
        r"fn verify_execution_qualified_build\(.*?\n\}", module, re.S
    )
    require(build_match is not None, "build verifier missing")
    build_body = build_match.group(0)
    for required in (
        '"broker-cli/m3j16-actual-one-shot".to_string(), false',
        '"finam-gateway/m3j16-actual-one-shot".to_string(), false',
        "unknown_feature_count != 0", "source_tree_before_sha256 != evidence.source_tree_after_sha256",
        "canonical_metadata_sha256.as_bytes()", "binary_sha256.as_bytes()",
        "endpoint_renderer_sha256.as_bytes()", "body_schema_sha256.as_bytes()",
    ):
        require(required in build_body, f"build provenance component missing: {required}")
    recover_match = re.search(r"fn recover\(root: &Path\).*?\n    \}\n\}", module, re.S)
    require(recover_match is not None, "durable recovery implementation missing")
    recover_body = recover_match.group(0)
    for required in (
        'strip_prefix("D:")', 'strip_prefix("U:")', "Stage8bClosureClassification::parse",
        "if durable == publication", "Ok(durable)", "body.ends_with(b\"\\n\")",
    ):
        require(required in recover_body, f"closure recovery invariant missing: {required}")

    authority_types = (
        "Stage8bExecutionQualifiedBuild", "Stage8bKeyedAccountBinding",
        "Stage8bFreshContractAuthority", "Stage8bAcceptedRunSpec", "Stage8bK1ControlApproved",
        "Stage8bAuthenticatedOperatorArm", "Stage8bK2FreshSources", "Stage8bK3CoveringSealApproved",
        "Stage8bK4ControlApproved", "Stage8bK5ReconciliationApproved",
        "Stage8bFreshPreflightApproved", "Stage8bSealedAttemptCommitted",
        "Stage8bExactTransportPermit", "Stage8bPossibleEffectOwner",
        "Stage8bDurableClosureOwner", "Stage8bClosureReceipt",
    )
    for name in authority_types:
        match = re.search(rf"pub\(crate\) struct {name}\s*\{{(?P<body>.*?)\n\}}", module, re.S)
        require(match is not None, f"authority type missing/private drift: {name}")
        require("pub " not in match.group("body"), f"authority field public: {name}")
        prefix = module[max(0, match.start() - 100):match.start()]
        require("#[derive" not in prefix, f"authority type gained derive: {name}")

    gateway_lib = text["gateway_lib"]
    require(gateway_lib.count("mod stage8b_no_send;") == 1, "Stage8B module ownership drift")
    for marker in (
        "invoke_stage8b_operator_once", "Stage8bOperatorDiagnostic",
        "Stage8bOperatorFacadeError", "Stage8bOperatorInvocationRequest",
    ):
        require(marker in gateway_lib, f"public facade export missing: {marker}")
    for forbidden in authority_types:
        require(forbidden not in gateway_lib, f"private authority re-exported: {forbidden}")

    cargo = text["gateway_cargo"]
    require("hmac.workspace = true" in cargo and "zeroize.workspace = true" in cargo, "HMAC/zeroize dependency missing")
    require('name = "hmac"' in text["cargo_lock"] and 'name = "zeroize"' in text["cargo_lock"], "locked privacy dependency missing")
    require("invoke_stage8b_no_send_from_cli" in text["cli_lib"], "broker-cli facade missing")
    require("broker_cli_reaches_only_the_public_redacted_no_send_facade" in text["cli_test"], "positive integration fixture missing")
    compile_script = text["compile"]
    require("stage8b-i-external-compile-fail: PASS positive=1 negative=18" in compile_script, "compile-fail count marker drift")
    for marker in (
        "check_fail private_module", "check_fail private_root", "check_fail private_build",
        "check_fail private_binding", "check_fail private_arm", "check_fail private_permit",
        "check_fail request_literal", "check_fail request_clone", "check_fail raw_path_getter",
        "check_fail authority_conversion", "check_fail arm_issuer", "check_fail classifier_bridge",
        "check_fail private_k2_sources", "check_fail sealed_transition",
        "check_fail permit_transition", "check_fail builder_before_permit",
        "check_fail raw_request_witness", "check_fail local_boundary",
    ):
        require(marker in compile_script, f"compile-fail witness missing: {marker}")

    for marker in (
        "stage8b-i-closed-surface: PASS",
        "ALLOWED_PRODUCTION",
        "workflow delta forbidden",
    ):
        require(marker in text["closed"], f"closed-surface marker missing: {marker}")
    for marker in (
        "python3 scripts/stage8b_i_check.py",
        "python3 scripts/stage8b_i_negative_harness.py",
        "python3 scripts/stage8b_i_closed_surface_check.py",
        "bash scripts/stage8b_i_external_compile_fail.sh",
        "stage8b-i-gate: PASS revision=R2 rows=92 negatives=70 compile_fail=18 canonical_regression=true",
    ):
        require(marker in text["gate"], f"aggregate-gate marker missing: {marker}")
    require("stage8b-i-handoff-safety: PASS" in text["handoff_safety"], "handoff safety marker missing")
    require("stage8b-i-handoff: PASS" in text["handoff_maker"], "handoff maker marker missing")
    for marker in (
        'GATE_LOG = ROOT / "reports/stage8b-i-r2-gate.log"',
        "current-tree-ci-gate: PASS source_ref={full_ref}",
        "stage8b-i-full-regression: PASS canonical_ci=true",
        "stale or incomplete exact-commit gate log",
    ):
        require(marker in text["handoff_maker"], f"exact-commit handoff gate binding missing: {marker}")
    for marker in (
        "bash scripts/current_tree_ci_gate.sh", "bash scripts/test_m4_3x_evidence_no_redis.sh",
        "cargo test --workspace --all-targets -- --test-threads=1",
        "cargo test --workspace --release --all-targets -- --test-threads=1",
        "cargo test --workspace --doc",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "scripts/redis_shadow_smoke.sh", "scripts/runtime_bridge_dry_smoke.sh",
        "stage8b-i-full-regression: PASS canonical_ci=true",
    ):
        require(marker in text["full_regression"], f"full regression marker missing: {marker}")

    contract = text["contract"]
    for marker in (
        "Stage 8B-I R2", "corrective no-send type-state", "invoke_stage8b_operator_once",
        "compose_stage8b_effect_authority", "O_CREAT|O_EXCL|O_NOFOLLOW",
        "six prefixes", "Stage 8B-I does not authorize 8B-IT", "canonical current-tree gate",
    ):
        require(marker in contract, f"contract marker missing: {marker}")

    print("stage8b-i-check: PASS revision=R2 rows=92 negatives=70 facade=1 root=1 compile_fail=18 permit_ordered=true durable_k2_k3_k4_k5_bound=true closure_exact=true build_endpoint_bound=true arm_bound=true no_send=true finam=false redis=false dispatch=false live=false real_orders=false stage8b_it=false stage8b_p=false stage8b_xe=false stage12=false")


if __name__ == "__main__":
    main()
