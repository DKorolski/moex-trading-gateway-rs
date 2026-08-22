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
    ("phase-order", '"8B-I", "8B-P"', '"8B-P", "8B-I"'),
    ("composition-crate", '"crate": "finam-gateway"', '"crate": "broker-cli"'),
    ("public-composition", '"visibility": "pub(crate)"', '"visibility": "pub"'),
    ("drop-stage8a", '"consumes_stage8a1_current_capability": true', '"consumes_stage8a1_current_capability": false'),
    ("drop-stage7b", '"consumes_stage7b_durable_authority": true', '"consumes_stage7b_durable_authority": false'),
    ("parallel-transport", '"parallel_transport_forbidden": true', '"parallel_transport_forbidden": false'),
    ("runtime-dependency", '"runtime_dependency_forbidden": true', '"runtime_dependency_forbidden": false'),
    ("raw-public-output", '"public_output": "redacted_diagnostic_only"', '"public_output": "raw_request_parts"'),
    ("remove-linear-type", '    "Stage8bExactTransportPermit",\n', ""),
    ("cloneable-authority", '"forbidden_traits": ["Clone", ', '"forbidden_traits": ['),
    ("serializable-authority", ', "Serialize", "Deserialize"]', ', "Deserialize"]'),
    ("noncausal-build", '"build_from_extracted_accepted_archive": true', '"build_from_extracted_accepted_archive": false'),
    ("archive-modes-unverified", '"archive_member_and_mode_verification": true', '"archive_member_and_mode_verification": false'),
    ("tree-unverified", '"pre_and_post_build_tree_verification": true', '"pre_and_post_build_tree_verification": false'),
    ("mutable-fetch", '"offline_build_after_dependency_preparation": true', '"offline_build_after_dependency_preparation": false'),
    ("manifests-unbound", '"cargo_lock_and_all_manifests_bound": true', '"cargo_lock_and_all_manifests_bound": false'),
    ("toolchain-unbound", '"toolchain_target_profile_binary_bound": true', '"toolchain_target_profile_binary_bound": false'),
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
    ("endpoint-no-binding", '      "keyed_account_binding",\n', ""),
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
    ("remove-kill-boundary", '    "ImmediatelyBeforeTransportWrite",\n', ""),
    ("remove-fresh-source", '    "account_orders",\n', ""),
    ("unfrozen-freshness", '"freshness_budgets_frozen_before": "8B-P"', '"freshness_budgets_frozen_before": "8B-X"'),
    ("historical-ready", '"historical_ack_implies_current_readiness": false', '"historical_ack_implies_current_readiness": true'),
    ("remove-crash-window", '    "ResponseNoDurableOutcome",\n', ""),
    ("response-resend", '"response_no_durable_outcome": "broker_truth_only_never_resend"', '"response_no_durable_outcome": "retry"'),
    ("publication-resend", '"durable_outcome_no_publication": "settlement_publication_only_never_resend"', '"durable_outcome_no_publication": "resend"'),
    ("automatic-retry", '"automatic_retry": false', '"automatic_retry": true'),
    ("truth-rewrites", '"broker_truth_may_rewrite_identity": false', '"broker_truth_may_rewrite_identity": true'),
    ("remove-closure", '    "ResidualPosition",\n', ""),
    ("two-sessions", '"minimum_complete_active_sessions": 3', '"minimum_complete_active_sessions": 2'),
    ("empty-session", '"no_activity_session_sufficient": false', '"no_activity_session_sufficient": true'),
    ("no-replay", '"deterministic_replay_for_unobserved_reachable_paths": true', '"deterministic_replay_for_unobserved_reachable_paths": false'),
    ("open-network", '"network_send": true', '"network_send": false'),
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
    if len(MUTATIONS) != 60:
        raise SystemExit(f"stage8b-spec-negative: FAIL inventory={len(MUTATIONS)} expected=60")
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
    print("stage8b-spec-negative: PASS cases=60/60")


if __name__ == "__main__":
    main()
