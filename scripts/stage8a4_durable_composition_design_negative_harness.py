#!/usr/bin/env python3
"""Run Stage 8A-4 durable-composition Design R2 negative mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_design_check as checker


def json_value(path: Path, keys: tuple[str, ...], value: object):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        node = data
        for key in keys[:-1]:
            node = node[key]
        node[keys[-1]] = value
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def json_list_remove(path: Path, keys: tuple[str, ...], value: str):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        node = data
        for key in keys:
            node = node[key]
        node.remove(value)
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def main() -> None:
    a = checker.AUTHORITY
    mutations = [
        ("accepted-ref", json_value(a, ("accepted_reducer_ref",), "0" * 40), None),
        ("review-hash", json_value(a, ("accepted_reducer_review_sha256",), "0" * 64), None),
        ("forged-accepted", json_value(a, ("status",), "accepted"), None),
        ("production-rust-enabled", json_value(a, ("production_rust_changed",), True), None),
        ("diagnostic-authority", json_value(a, ("authoritative_result", "public_diagnostic_is_authority"), True), None),
        ("result-public", json_value(a, ("authoritative_result", "private"), False), None),
        ("caller-constructible", json_value(a, ("authoritative_result", "caller_constructible"), True), None),
        ("partial-identity-merge", json_value(a, ("partial_identity_policy",), "material_merge"), None),
        ("not-found-no-match", json_value(a, ("documented_not_found_proves_no_match",), True), None),
        ("unavailable-no-match", json_value(a, ("unavailable_proves_no_match",), True), None),
        ("proven-no-match", json_value(a, ("proven_no_match_available",), True), None),
        ("unknown-safety-removed", json_list_remove(a, ("account_safety_summary",), "unknown_status_orders"), None),
        ("orphan-safety-removed", json_list_remove(a, ("account_safety_summary",), "orphan_orders"), None),
        ("seal-precondition-removed", json_list_remove(a, ("pre_append_compare_and_append", "fields"), "expected_recovery_seal_fingerprint"), None),
        ("arm-provenance-removed", json_value(a, ("operator_arm_post_effect", "historical_provenance_and_scope_required"), False), None),
        ("kill-switch-revalidation-removed", json_list_remove(a, ("apply_time_revalidation",), "kill_switch_for_readiness_and_send_gate"), None),
        ("hold-advances-lifecycle", json_value(a, ("conflict_or_unknown_advances_order_lifecycle",), True), None),
        ("replay-non-idempotent", json_value(a, ("replay_is_idempotent",), False), None),
        ("publication-before-transition", json_value(a, ("transition_durable_before_any_derived_publication",), False), None),
        ("retry-rearm-resend", json_value(a, ("result_grants_retry_rearm_or_resend",), True), None),
        ("implementation-open", json_value(a, ("closed", "durable_composition_implementation"), False), None),
        ("finam-open", json_value(a, ("closed", "finam_post_delete"), False), None),
        ("runtime-live-open", json_value(a, ("closed", "runtime_live"), False), None),
        ("production-path-added", lambda root: None, checker.ALLOWED_CHANGED_PATHS | {"crates/finam-gateway/src/lib.rs"}),
        ("random-transition-key", json_value(a, ("transition_identity", "fields"), ["random_nonce"]), None),
        ("mutable-generation-in-key", json_value(a, ("transition_identity", "includes_mutable_post_append_generation"), True), None),
        ("post-append-seal-removed", json_value(a, ("post_append_recovery_seal", "required"), False), None),
        ("ack-before-covering-seal", json_value(a, ("post_append_recovery_seal", "ack_after_covering_seal_only"), False), None),
        ("append-before-seal-reruns-append", json_value(a, ("crash_recovery", "AfterTransitionAppendBeforeCoveringSeal"), "rerun_reducer_and_append_again"), None),
        ("expired-arm-blocks-reconciliation", json_value(a, ("operator_arm_post_effect", "expired_operator_arm_blocks_reconciliation_append"), True), None),
        ("stop-blocks-reconciliation", json_value(a, ("kill_switch_post_effect", "stop_requested_blocks_reconciliation_append"), True), None),
        ("not-found-downgraded", json_value(a, ("exact_lookup_disposition", "DocumentedNotFound"), "use_other_admitted_sources"), None),
        ("unavailable-downgraded", json_value(a, ("exact_lookup_disposition", "Unavailable"), "use_other_admitted_sources"), None),
        ("conflict-terminal-ack", json_value(a, ("conflict_hold_terminal_ack_allowed",), True), None),
        ("unknown-terminal-xack", json_value(a, ("still_unknown_hold_xack_allowed",), True), None),
        ("retry-transition", json_value(a, ("transition_vocabulary",), checker.TRANSITIONS[:-1] + ["RetryAllowed"]), None),
        ("decode-failure-downgraded", json_value(a, ("exact_lookup_disposition", "DecodeFailure"), "use_other_admitted_sources"), None),
        ("stale-downgraded", json_value(a, ("exact_lookup_disposition", "Stale"), "use_other_admitted_sources"), None),
    ]
    if len(mutations) != 38:
        raise SystemExit("stage8a4-durable-composition-design-negative: FAIL inventory")
    with tempfile.TemporaryDirectory(prefix="stage8a4-durable-design-negative-") as raw:
        root = Path(raw) / "repo"
        shutil.copytree(checker.ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__"))
        for name, mutate, override in mutations:
            original = (root / a).read_text()
            mutate(root)
            try:
                checker.check(root, git_scope=False, changed_paths_override=override)
            except checker.CheckFailure:
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-durable-composition-design-negative: FAIL survived={name}")
            finally:
                (root / a).write_text(original)
    print("stage8a4-durable-composition-design-negative: PASS 38/38")


if __name__ == "__main__":
    main()
