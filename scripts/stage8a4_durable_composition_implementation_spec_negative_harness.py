#!/usr/bin/env python3
"""Run 57 fail-closed Stage 8A-4 implementation-spec R2 mutations."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path

import stage8a4_durable_composition_implementation_spec_check as checker


def set_value(path: Path, keys: tuple[str, ...], value: object):
    def mutate(root: Path) -> None:
        target = root / path
        data = json.loads(target.read_text())
        node = data
        for key in keys[:-1]:
            node = node[key]
        node[keys[-1]] = value
        target.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    return mutate


def remove_value(path: Path, keys: tuple[str, ...], value: str):
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
    cases = [
        # R1 inherited 1..40.
        ("design-ref", set_value(a, ("accepted_durable_design_ref",), "0" * 40), None),
        ("review-hash", set_value(a, ("accepted_durable_design_review_sha256",), "0" * 64), None),
        ("forged-accepted", set_value(a, ("status",), "accepted"), None),
        ("spec-only-disabled", set_value(a, ("spec_only",), False), None),
        ("production-rust", set_value(a, ("production_rust_changed",), True), None),
        ("v1-bytes-mutable", set_value(a, ("schema_decision", "stage6_v1_bytes_immutable"), False), None),
        ("v1-semantics-mutable", set_value(a, ("schema_decision", "stage6_v1_semantics_immutable"), False), None),
        ("digest-smuggling", set_value(a, ("schema_decision", "source_evidence_digest_smuggling_forbidden"), False), None),
        ("historical-rewrite", set_value(a, ("schema_decision", "historical_rewrite_or_migration_allowed"), True), None),
        ("mixed-replay-disabled", set_value(a, ("schema_decision", "mixed_v1_v2_replay_required"), False), None),
        ("unknown-v2-skip", set_value(a, ("schema_decision", "unknown_v2_skip_allowed"), True), None),
        ("v2-event-drift", set_value(a, ("v2_record", "event_kind_variants"), ["Marker"]), None),
        ("stable-key-field-removed", remove_value(a, ("stable_key_fields",), "transition_kind"), None),
        ("nonce-in-key", set_value(a, ("stable_key_fields",), checker.STABLE_KEY_FIELDS + ["random_nonce"]), None),
        ("lookup-evidence-removed", remove_value(a, ("v2_payload_fields",), "exact_lookup_evidence"), None),
        ("suffix-manifest-removed", remove_value(a, ("v2_payload_fields",), "deterministic_suffix_manifest"), None),
        ("query-account-removed", remove_value(a, ("exact_lookup_union", "Unavailable"), "account_id"), None),
        ("query-order-removed", remove_value(a, ("exact_lookup_union", "Unavailable"), "queried_broker_order_id"), None),
        ("response-timing-removed", remove_value(a, ("exact_lookup_union", "Unavailable"), "response_received_at"), None),
        ("transition-not-first", set_value(a, ("future_append_batch", "transition_v2_is_first_record"), False), None),
        ("append-not-durable", set_value(a, ("future_append_batch", "each_record_append_is_durable"), False), None),
        ("seal-before-suffix", set_value(a, ("future_append_batch", "covering_seal_after_complete_batch_only"), False), None),
        ("restart-no-key-lookup", set_value(a, ("future_append_batch", "restart_finds_transition_by_persisted_stable_key"), False), None),
        ("second-transition", set_value(a, ("future_append_batch", "second_transition_append_allowed"), True), None),
        ("different-payload-accepted", set_value(a, ("future_append_batch", "same_key_different_payload_or_record"), "accept"), None),
        ("seal-cas-removed", remove_value(a, ("pre_append_cas_fields",), "expected_recovery_seal_fingerprint"), None),
        ("s1-no-frontier", set_value(a, ("covering_seal_protocol", "post_batch_seal_must_cover_f1"), False), None),
        ("s1-no-reread", set_value(a, ("covering_seal_protocol", "post_batch_seal_reread_hmac_canonical_checkpoint_validation"), False), None),
        ("expired-arm-blocks", set_value(a, ("post_effect_controls", "expired_operator_arm_blocks_reconciliation_append"), True), None),
        ("stop-blocks", set_value(a, ("post_effect_controls", "stop_requested_blocks_reconciliation_append"), True), None),
        ("unreadable-kill-blocks", set_value(a, ("post_effect_controls", "stale_or_unreadable_kill_switch_blocks_reconciliation_append"), True), None),
        ("reconciliation-send", set_value(a, ("post_effect_controls", "reconciliation_can_send"), True), None),
        ("place-working-unresolved", set_value(a, ("endpoint_disposition_matrix", "place", "ExactWorking"), "unresolved"), None),
        ("place-rejected-completed", set_value(a, ("endpoint_disposition_matrix", "place", "ExactTerminalRejected"), "request_finalized_completed"), None),
        ("cancel-working-finalized", set_value(a, ("endpoint_disposition_matrix", "cancel", "ExactWorking"), "request_finalized_completed"), None),
        ("cancel-filled-nonexecution", set_value(a, ("endpoint_disposition_matrix", "cancel", "ExactTerminalFilled"), "already_terminal_non_execution"), None),
        ("hold-ack-enabled", set_value(a, ("canonical_ack", "hold_ack_or_xack_allowed"), True), None),
        ("ack-without-seal", set_value(a, ("canonical_ack", "terminal_ack_requires_covering_seal"), False), None),
        ("i1-writer-open", set_value(a, ("closed", "durable_writer_apply_in_i1"), False), None),
        ("production-path", lambda root: None, checker.ALLOWED_CHANGED_PATHS | {"crates/finam-gateway/src/lib.rs"}),
        # R2 additions 41..57.
        ("v2-lifecycle-sequence-omitted", remove_value(a, ("v2_record", "outer_fields"), "lifecycle_sequence"), None),
        ("v2-previous-record-id-omitted", remove_value(a, ("v2_record", "outer_fields"), "previous_record_id"), None),
        ("v2-durable-identity-omitted", remove_value(a, ("v2_record", "outer_fields"), "durable_request_identity"), None),
        ("v2-mutates-v1-event-kind", set_value(a, ("v2_record", "event_kind_type_is_separate_from_v1"), False), None),
        ("unknown-record-version-skipped", set_value(a, ("framed_journal_dispatch", "unknown_version_action"), "skip"), None),
        ("failed-v2-falls-back-v1", set_value(a, ("framed_journal_dispatch", "v2_decode_failure_fallback_allowed"), True), None),
        ("mixed-replay-ignores-v2-frontier", set_value(a, ("mixed_replay", "v2_advances_last_sequence_and_record_id"), False), None),
        ("v2-directly-applies-suffix", set_value(a, ("mixed_replay", "v2_itself_applies_v1_suffix_semantics"), True), None),
        ("pending-batch-loses-key", remove_value(a, ("mixed_replay", "pending_batch_fields"), "stable_transition_key_sha256"), None),
        ("manifest-payload-hash-only", remove_value(a, ("suffix_manifest_entry_fields",), "canonical_record_sha256"), None),
        ("changed-source-evidence-accepted", set_value(a, ("suffix_manifest_policy", "binds_previous_causal_identity_payload_source_and_sequence"), False), None),
        ("changed-causal-previous-accepted", set_value(a, ("suffix_manifest_policy", "same_payload_different_full_record"), "accept"), None),
        ("invent-order-id", set_value(a, ("fact_projection", "missing_broker_order_id_is_never_fabricated"), False), None),
        ("invent-trade-order-id", set_value(a, ("fact_projection", "v1_broker_trade_observed_requires_real_compatible_broker_order_id"), False), None),
        ("drop-client-linked-trade", set_value(a, ("fact_projection", "client_linked_trade_without_broker_id_retained_in_v2"), False), None),
        ("succeeded-loses-observation", remove_value(a, ("exact_lookup_union", "Succeeded"), "exact_order_observation_v2"), None),
        ("v1-golden-mutable", set_value(a, ("schema_decision", "canonical_golden_and_restart_compatibility_required"), False), None),
    ]
    if len(cases) != 57:
        raise SystemExit("stage8a4-durable-composition-implementation-spec-negative: FAIL inventory")
    with tempfile.TemporaryDirectory(prefix="stage8a4-implementation-spec-negative-") as raw:
        root = Path(raw) / "repo"
        shutil.copytree(checker.ROOT, root, ignore=shutil.ignore_patterns(".git", "target", "tmp", "reports", "__pycache__"))
        for name, mutate, override in cases:
            original = (root / a).read_text()
            mutate(root)
            try:
                checker.check(root, git_scope=False, changed_paths_override=override)
            except checker.CheckFailure:
                print(f"PASS {name}")
            else:
                raise SystemExit(f"stage8a4-durable-composition-implementation-spec-negative: FAIL survived={name}")
            finally:
                (root / a).write_text(original)
    print("stage8a4-durable-composition-implementation-spec-negative: PASS 57/57")


if __name__ == "__main__":
    main()
