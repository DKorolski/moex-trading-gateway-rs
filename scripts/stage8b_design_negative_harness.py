#!/usr/bin/env python3
"""Exact fail-closed mutation harness for Stage 8B-D R2."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/stage8b_design_check.py"

MUTATIONS = [
    ("accepted-stage8a5-ref", "bf58b47fdef8af774a4107455dfcc6204e594283", "0" * 40),
    ("accepted-gov-ci-ref", "13f659f368cbb36a2d38c2b0b88efa376f0b690c", "1" * 40),
    ("design-base-ref", "7bc9fdab190e011111b15ebdf2f35ff2263a8e34", "2" * 40),
    ("retained-r1-ref", "b3358ba2268da3db4eb8352c097495ebb85575d7", "3" * 40),
    ("matrix-count", '"acceptance_rows": 70', '"acceptance_rows": 69'),
    ("negative-count", '"negative_cases": 50', '"negative_cases": 49'),
    ("open-implementation", '"next_after_acceptance": "Stage 8B-S implementation specification only"', '"next_after_acceptance": "Stage 8B-S implementation"'),
    ("remove-phase", '    "8B-I no-send implementation and rehearsal acceptance",\n', ""),
    ("multi-command", '"exactly_one_command": true', '"exactly_one_command": false'),
    ("market-open", '"market_order_allowed": false', '"market_order_allowed": true'),
    ("build-manifest-optional", '"execution_qualified_manifest_required": true', '"execution_qualified_manifest_required": false'),
    ("source-archive-unbound", '"source_commit_and_archive_sha256_required": true', '"source_commit_and_archive_sha256_required": false'),
    ("cargo-lock-unbound", '"cargo_lock_sha256_required": true', '"cargo_lock_sha256_required": false'),
    ("rustc-unbound", '"rustc_vv_and_commit_required": true', '"rustc_vv_and_commit_required": false'),
    ("metadata-graph-unbound", '"cargo_metadata_graph_sha256_required": true', '"cargo_metadata_graph_sha256_required": false'),
    ("feature-set-incomplete", '"complete_feature_set_required": true', '"complete_feature_set_required": false'),
    ("legacy-cli-enabled", '"legacy_actual_send_feature_broker_cli": false', '"legacy_actual_send_feature_broker_cli": true'),
    ("legacy-gateway-enabled", '"legacy_actual_send_feature_finam_gateway": false', '"legacy_actual_send_feature_finam_gateway": true'),
    ("unknown-feature-authorized", '"missing_or_unknown_feature_authorizable": false', '"missing_or_unknown_feature_authorizable": true'),
    ("alternate-transport", '"alternate_real_transport_path_allowed": false', '"alternate_real_transport_path_allowed": true'),
    ("mutable-toolchain", '"toolchain_immutable_version_required_before_protected_evidence": true', '"toolchain_immutable_version_required_before_protected_evidence": false'),
    ("plain-account-sha", '"hmac_algorithm": "HMAC-SHA256"', '"hmac_algorithm": "SHA256"'),
    ("raw-account-export", '"raw_account_id_in_git_or_handoff_allowed": false', '"raw_account_id_in_git_or_handoff_allowed": true'),
    ("operator-key-export", '"operator_key_in_git_or_handoff_allowed": false', '"operator_key_in_git_or_handoff_allowed": true'),
    ("weak-operator-key", '"minimum_operator_key_bits": 256', '"minimum_operator_key_bits": 128'),
    ("account-domain-drift", '"domain_separator": "moex-stage8b-account-binding-v1\\\\0"', '"domain_separator": "account"'),
    ("account-normalization", '"normalization_allowed": false', '"normalization_allowed": true'),
    ("account-fallback", '"fallback_to_plain_digest_allowed": false', '"fallback_to_plain_digest_allowed": true'),
    ("cloneable-arm", '"clone_serialize_default_allowed": false', '"clone_serialize_default_allowed": true'),
    ("restart-arm", '"reconstructible_after_restart": false', '"reconstructible_after_restart": true'),
    ("preflight-build-drift", '"build_feature_api_contract_match_required": true', '"build_feature_api_contract_match_required": false'),
    ("caller-snapshot", '"caller_supplied_snapshot_allowed": false', '"caller_supplied_snapshot_allowed": true'),
    ("dual-owner", '"single_broker_ownership_required": true', '"single_broker_ownership_required": false'),
    ("ambiguity-open", '"zero_ambiguity_required": true', '"zero_ambiguity_required": false'),
    ("send-before-attempt", '"transport_may_run_before_attempt_commit": false', '"transport_may_run_before_attempt_commit": true'),
    ("redis-authority", '"redis_is_not_execution_authority": true', '"redis_is_not_execution_authority": false'),
    ("automatic-retry", '"same_request_automatic_retry_after_transport_boundary": false', '"same_request_automatic_retry_after_transport_boundary": true'),
    ("timeout-definitely-no-send", "timeout, disconnect, partial write, response loss", "timeout is definitely not sent; disconnect, partial write, response loss"),
    ("empty-proves-flat", "Empty, missing, stale or account-wide row\ncounts do not prove absence or flat", "Empty account-wide rows prove absence and flat"),
    ("truth-rewrites-identity", "Broker truth cannot rewrite durable identity", "Broker truth may rewrite durable identity"),
    ("remove-closure-state", '      "ResidualPosition",\n', ""),
    ("residual-is-safe", '"accepted_state": "Stage8BClosedSafe"', '"accepted_state": "ResidualPosition"'),
    ("baseline-not-required", '"target_position_equals_approved_baseline_required": true', '"target_position_equals_approved_baseline_required": false'),
    ("automatic-residual-resolution", '"automatic_residual_resolution_allowed": false', '"automatic_residual_resolution_allowed": true'),
    ("new-arm-while-blocked", '"new_arm_while_blocked_allowed": false', '"new_arm_while_blocked_allowed": true'),
    ("two-stage11-sessions", '"complete_active_sessions_required": 3', '"complete_active_sessions_required": 2'),
    ("recovery-replaces-session", '"recovery_qualification_is_separate": true', '"recovery_qualification_is_separate": false'),
    ("stage11-divergence", '"zero_unexplained_blocking_divergences_required": true', '"zero_unexplained_blocking_divergences_required": false'),
    ("silent-action-rewrite", '"silent_action_or_quantity_rewrite_allowed": false', '"silent_action_or_quantity_rewrite_allowed": true'),
    ("open-stage8b-execution", '"stage8b_execution": true', '"stage8b_execution": false'),
]


def mutate(tree: Path, old: str, new: str) -> None:
    authority = tree / "docs/stage-8/stage8b-design-authority.json"
    candidates = [authority] + [
        path
        for path in sorted((tree / "docs/stage-8").glob("*8B*"))
        if path != authority
    ]
    for path in candidates:
        text = path.read_text(encoding="utf-8")
        if old in text:
            path.write_text(text.replace(old, new, 1), encoding="utf-8")
            return
    raise RuntimeError(f"mutation source missing: {old}")


def main() -> None:
    if len(MUTATIONS) != 50:
        raise SystemExit("stage8b-design-negative: FAIL inventory is not exact 50")
    with tempfile.TemporaryDirectory(prefix="stage8b-design-negative-") as raw:
        base = Path(raw) / "base"
        shutil.copytree(ROOT / "docs", base / "docs")
        shutil.copytree(ROOT / "scripts", base / "scripts")
        for name, old, new in MUTATIONS:
            case = Path(raw) / name
            shutil.copytree(base, case)
            mutate(case, old, new)
            env = os.environ.copy()
            env["STAGE8B_ROOT"] = str(case)
            result = subprocess.run(
                ["python3", str(CHECKER), "--no-git"],
                cwd=ROOT,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                raise SystemExit(f"stage8b-design-negative: FAIL {name}")
            print(f"PASS {name}")
    print("stage8b-design-negative: PASS cases=50/50")


if __name__ == "__main__":
    main()
