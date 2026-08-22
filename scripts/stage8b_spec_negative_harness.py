#!/usr/bin/env python3
"""Exact fail-closed mutation harness for Stage 8B-S."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_spec_check.py"
MUTATIONS = [
    ("r2-candidate", "f296d0be782b8aa550a20e27600ba16826214349", "0" * 40),
    ("r2-merge", "50ed5382fdbe2d62ed253d65a312f951e2a267ff", "1" * 40),
    ("r2-tree", "f40e2e5f40d7e3ed1dd5f5a252832734265094df", "2" * 40),
    ("r2-handoff", "ac351d9c03c98d59e90affeb423dbb7fff2cd3722b3d601889c53ae90c6cc06b", "3" * 64),
    ("r2-review", "ba624781b59741aae1c59acbf430f897c7c591ac78aecc9e0a0463883ffacaa0", "4" * 64),
    ("stage8a5-ref", "bf58b47fdef8af774a4107455dfcc6204e594283", "5" * 40),
    ("gov-ref", "7bc9fdab190e011111b15ebdf2f35ff2263a8e34", "6" * 40),
    ("phase-order", '"8B-I", "8B-IT", "8B-P"', '"8B-I", "8B-P", "8B-IT"'),
    ("composition-crate", '"crate": "finam-gateway"', '"crate": "broker-cli"'),
    ("public-composition", '"visibility": "pub(crate)"', '"visibility": "pub"'),
    ("drop-stage8a", '"consumes_stage8a1_current_capability": true', '"consumes_stage8a1_current_capability": false'),
    ("drop-stage7b", '"consumes_stage7b_durable_authority": true', '"consumes_stage7b_durable_authority": false'),
    ("parallel-transport", '"parallel_transport_forbidden": true', '"parallel_transport_forbidden": false'),
    ("runtime-dependency", '"runtime_dependency_forbidden": true', '"runtime_dependency_forbidden": false'),
    ("raw-public-output", '"returns_redacted_diagnostic_only": true', '"returns_redacted_diagnostic_only": false'),
    ("remove-linear-type", '    "Stage8bExactTransportPermit",\n', ""),
    ("cloneable-authority", '"forbidden_traits": ["Clone", ', '"forbidden_traits": ['),
    ("serializable-authority", ', "Serialize", "Deserialize"]', ', "Deserialize"]'),
    ("noncausal-build", '"build_from_extracted_accepted_archive": true', '"build_from_extracted_accepted_archive": false'),
    ("archive-modes-unverified", '"archive_member_and_mode_verification": true', '"archive_member_and_mode_verification": false'),
    ("tree-unverified", '"pre_and_post_build_tree_verification": true', '"pre_and_post_build_tree_verification": false'),
    ("mutable-fetch", '"offline_build_after_dependency_preparation": true', '"offline_build_after_dependency_preparation": false'),
    ("manifests-unbound", '"cargo_lock_and_all_manifests_bound": true', '"cargo_lock_and_all_manifests_bound": false'),
    ("toolchain-unbound", '"rustc_llvm_version", ', ''),
    ("local-path-metadata", '"canonical_metadata_projection_excludes_local_paths": true', '"canonical_metadata_projection_excludes_local_paths": false'),
    ("declared-features-only", '"resolved_feature_graph_required": true', '"resolved_feature_graph_required": false'),
    ("legacy-cli", '"broker_cli_m3j16_actual_one_shot": false', '"broker_cli_m3j16_actual_one_shot": true'),
    ("legacy-gateway", '"finam_gateway_m3j16_actual_one_shot": false', '"finam_gateway_m3j16_actual_one_shot": true'),
    ("unknown-feature", '"unknown_feature_state_authorizable": false', '"unknown_feature_state_authorizable": true'),
    ("plain-account", '"account_binding": "HMAC-SHA256"', '"account_binding": "SHA256"'),
    ("weak-key", '"minimum_key_bits": 256', '"minimum_key_bits": 128'),
    ("normalize-account", '"exact_utf8_no_normalization": true', '"exact_utf8_no_normalization": false'),
    ("timing-leak", '"constant_time_verification": true', '"constant_time_verification": false'),
    ("plain-fallback", '"plain_digest_fallback": false', '"plain_digest_fallback": true'),
    ("endpoint-no-binding", '"keyed_account_binding", ', ""),
    ("rendered-path-digest", '"rendered_path_sha256_publishable": false', '"rendered_path_sha256_publishable": true'),
    ("raw-account", '"raw_account_export": false', '"raw_account_export": true'),
    ("secret-export", '"secret_key_export": false', '"secret_key_export": true'),
    ("two-effects", '"max_effects": 1', '"max_effects": 2'),
    ("market", '"place_order_type": "LIMIT"', '"place_order_type": "MARKET"'),
    ("gtc", '"place_tif": "DAY"', '"place_tif": "GTC"'),
    ("two-lots", '"max_lots": 1', '"max_lots": 2'),
    ("instrument", '"instrument": "IMOEXF@RTSX"', '"instrument": "RTS@RTSX"'),
    ("cancel-other-lifecycle", '"cancel_same_durable_lifecycle": true', '"cancel_same_durable_lifecycle": false'),
    ("cancel-terminal", '"cancel_requires_currently_working": true', '"cancel_requires_currently_working": false'),
    ("silent-rewrite", '"silent_rewrite_forbidden": true', '"silent_rewrite_forbidden": false'),
    ("remove-kill-boundary", '"K4_immediately_before_transport_write", ', ""),
    ("remove-fresh-source", '"account_orders", ', ""),
    ("unfrozen-freshness", '"freshness_budgets_frozen_before": "8B-P"', '"freshness_budgets_frozen_before": "8B-X"'),
    ("historical-ready", '"historical_ack_implies_current_readiness": false', '"historical_ack_implies_current_readiness": true'),
    ("remove-crash-window", '"ResponseNoDurableOutcome", ', ""),
    ("response-resend", '"response_no_durable_outcome": "broker_truth_only_never_resend"', '"response_no_durable_outcome": "retry"'),
    ("publication-resend", '"durable_outcome_no_publication": "settlement_publication_only_never_resend"', '"durable_outcome_no_publication": "resend"'),
    ("automatic-retry", '"automatic_retry": false', '"automatic_retry": true'),
    ("truth-rewrites", '"broker_truth_may_rewrite_identity": false', '"broker_truth_may_rewrite_identity": true'),
    ("remove-closure", '"ResidualPosition", ', ""),
    ("two-sessions", '"minimum_complete_active_sessions": 3', '"minimum_complete_active_sessions": 2'),
    ("empty-session", '"no_activity_session_sufficient": false', '"no_activity_session_sufficient": true'),
    ("no-replay", '"deterministic_replay_for_unobserved_reachable_paths": true', '"deterministic_replay_for_unobserved_reachable_paths": false'),
    ("open-network", '"network_send": true', '"network_send": false'),
    ("remove-public-facade", '"name": "invoke_stage8b_operator_once"', '"name": "removed_facade"'),
    ("facade-raw-transport", '"raw_account_url_method_header_body_token_client_transport_allowed": false', '"raw_account_url_method_header_body_token_client_transport_allowed": true'),
    ("root-cross-crate", '"cross_crate_accessible": false', '"cross_crate_accessible": true'),
    ("facade-authority-output", '"capability_or_arm_input_output_allowed": false', '"capability_or_arm_input_output_allowed": true'),
    ("domain-text-drift", '"domain_ascii": "moex-stage8b-account-binding-v1"', '"domain_ascii": "moex-stage8b-account-binding-v2"'),
    ("literal-domain-suffix", '"domain_suffix_hex": "00"', '"domain_suffix_hex": "5c7530303030"'),
    ("removed-domain-suffix", '"domain_suffix_hex": "00"', '"domain_suffix_hex": ""'),
    ("little-endian-length", '"length_encoding": "u32be"', '"length_encoding": "u32le"'),
    ("golden-key-drift", '"key_hex": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"', '"key_hex": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"'),
    ("golden-account-drift", '"account_utf8_hex": "4143435f544553545f30303031"', '"account_utf8_hex": "4143435f544553545f30303032"'),
    ("golden-message-drift", '"message_hex": "6d6f65782d737461676538622d6163636f756e742d62696e64696e672d7631000000000d4143435f544553545f30303031"', '"message_hex": "00"'),
    ("golden-digest-drift", '"expected_hmac_sha256": "60106309bd530bd0cec76c3fa78fa4b7004ef34e44447fb7cd78fdda87444435"', '"expected_hmac_sha256": "' + '0' * 64 + '"'),
    ("r2-authority-drift", '"authority_sha256": "83e85722fcf41e6abdd215569c4337f6c83994baeafbae47c5ad80bb9e935d09"', '"authority_sha256": "' + '7' * 64 + '"'),
    ("s-overrides-r2", '"s_fields_may_weaken_or_override_stage8b_d": false', '"s_fields_may_weaken_or_override_stage8b_d": true'),
    ("max-notional-optional", '"required_in_accepted_run_spec": true', '"required_in_accepted_run_spec": false'),
    ("max-notional-no-transport-recheck", '"checked_immediately_before_transport": true', '"checked_immediately_before_transport": false'),
    ("finam-host-drift", '"exact_host": "api.finam.ru"', '"exact_host": "example.invalid"'),
    ("tls-optional", '"tls_required": true', '"tls_required": false'),
    ("redirects-open", '"redirects_allowed": false', '"redirects_allowed": true'),
    ("proxy-open", '"proxy_allowed": false', '"proxy_allowed": true'),
    ("transport-retry-open", '"automatic_transport_retry_allowed": false', '"automatic_transport_retry_allowed": true'),
    ("second-arm-open", '"second_arm_same_request_allowed": false', '"second_arm_same_request_allowed": true'),
    ("restart-arm-open", '"restart_reconstructs_arm_or_send_authority": false', '"restart_reconstructs_arm_or_send_authority": true'),
    ("preflight-owner-missing", '"single_finam_execution_owner_required": true', '"single_finam_execution_owner_required": false'),
    ("toolchain-field-reduced", '"rustc_commit_date", ', ''),
    ("stage11-alor-owner-weakened", '"alor_sole_execution_owner_oracle": true', '"alor_sole_execution_owner_oracle": false'),
    ("k2-before-arm", '"K1_fresh_control_before_arm_issuance", "DurableArmIssuedForExactRun"', '"K2_exact_arm_preflight_owns_arm", "DurableArmIssuedForExactRun"'),
    ("second-serializer-open", '"independent_serializer_allowed": false', '"independent_serializer_allowed": true'),
    ("third-classifier-open", '"stage8a3_model": "A_reuse_accepted_classifier"', '"stage8a3_model": "C_new_classifier"'),
    ("adapter-not-before-p", '"qualification_independently_accepted_before_exact_8b_p": true', '"qualification_independently_accepted_before_exact_8b_p": false'),
    ("p-build-not-bound", '"p_build_sha_equals_accepted_adapter_build_sha": true', '"p_build_sha_equals_accepted_adapter_build_sha": false'),
    ("p-source-not-bound", '"p_source_equals_accepted_adapter_source": true', '"p_source_equals_accepted_adapter_source": false'),
    ("p-executable-not-bound", '"p_executable_equals_accepted_adapter_executable": true', '"p_executable_equals_accepted_adapter_executable": false'),
    ("p-renderer-body-not-bound", '"p_endpoint_renderer_and_body_schema_equal_accepted_adapter": true', '"p_endpoint_renderer_and_body_schema_equal_accepted_adapter": false'),
    ("post-p-drift-retains-authority", '"post_p_drift_invalidates_p": true', '"post_p_drift_invalidates_p": false'),
    ("prequalification-p-accepted", '"p_issued_before_adapter_qualification_allowed": false', '"p_issued_before_adapter_qualification_allowed": true'),
    ("xe-different-build", '"xe_requires_exact_p_bound_build": true', '"xe_requires_exact_p_bound_build": false'),
    ("p-authority-carry-over", '"automatic_p_refresh_or_authority_carry_over_allowed": false', '"automatic_p_refresh_or_authority_carry_over_allowed": true'),
    ("skip-requalification", '"material_drift_requires_adapter_requalification_where_relevant": true', '"material_drift_requires_adapter_requalification_where_relevant": false'),
    ("skip-fresh-p-after-drift", '"material_drift_requires_fresh_contract_preflight_and_new_p": true', '"material_drift_requires_fresh_contract_preflight_and_new_p": false'),
]


def mutate(tree: Path, old: str, new: str) -> None:
    candidates = [tree / "docs/stage-8/stage8b-spec-authority.json"] + sorted((tree / "docs/stage-8").glob("*STAGE8B_SPEC*")) + [tree / "docs/stage-8/STAGE8B_IMPLEMENTATION_SPEC_2026-08-22.md"]
    for path in candidates:
        text = path.read_text(encoding="utf-8")
        if old in text:
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            return
    raise RuntimeError(f"mutation source missing: {old}")


def main() -> None:
    if len(MUTATIONS) != 100:
        raise SystemExit(f"stage8b-spec-negative: FAIL inventory={len(MUTATIONS)} expected=100")
    with tempfile.TemporaryDirectory(prefix="stage8b-spec-negative-") as raw:
        base = Path(raw) / "base"
        shutil.copytree(ROOT / "docs", base / "docs")
        shutil.copytree(ROOT / "scripts", base / "scripts")
        for name, old, new in MUTATIONS:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutate(case, old, new)
            env = os.environ.copy()
            env["STAGE8B_SPEC_ROOT"] = str(case)
            result = subprocess.run(["python3", str(CHECKER), "--no-git"], cwd=ROOT, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode == 0:
                raise SystemExit(f"stage8b-spec-negative: FAIL {name}")
            print(f"PASS {name}")
    print("stage8b-spec-negative: PASS cases=100/100")


if __name__ == "__main__":
    main()
