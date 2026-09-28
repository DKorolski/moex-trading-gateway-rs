#!/usr/bin/env python3
"""Targeted mutation harness for the Stage 8B-P1-d4 source contract."""

from __future__ import annotations

import copy

import stage8b_p1d4_source_check as check


def replace(content: dict[str, str], key: str, old: str, new: str) -> dict[str, str]:
    mutated = copy.deepcopy(content)
    if old not in mutated[key]:
        raise RuntimeError(f"mutation source missing: {key}: {old}")
    mutated[key] = mutated[key].replace(old, new)
    return mutated


def main() -> None:
    base = check.load_content()
    cases = [
        ("reservation-domain", "domain", "command-publication-reservation.v1", "command-publication-reservation.v2"),
        ("binding-domain", "domain", "command-publication-binding.v1", "command-publication-binding.v2"),
        ("composition-domain", "domain", "generated-market-composition.v1", "generated-market-composition.v2"),
        ("tri-state", "domain", "pub enum Stage8bP1d4GeneratedMarketPackageState", "enum RemovedPackageState"),
        ("successor-check", "domain", "immediate_successor(predecessor)? != reserved", "predecessor != reserved"),
        ("write-generation-bytes", "domain", "self.prepublication_package_generation.to_be_bytes()", "0_u64.to_be_bytes()"),
        ("seal-generation-bytes", "domain", "self.prepublication_seal_generation.to_be_bytes()", "0_u64.to_be_bytes()"),
        ("high-u64-domain-test", "domain", "seal_generation_above_json_safe_integer_remains_distinct", "removed_high_u64_test"),
        ("p1d4-stage5g-slot", "stage5g", "p1d4_generated_market:", "removed_generated_market:"),
        ("p1d4-source-variant", "stage5g", "Stage5gCleanRestartSource::P1d4", "Stage5gCleanRestartSource::P1d3"),
        ("auth-probe", "stage6", "authenticate_stage8b_p1d4_restart_package_state", "removed_auth_probe"),
        ("reservation-transition", "stage6", "apply_stage8b_p1d4_publication_reservation", "removed_reservation_transition"),
        ("ack-transition", "stage6", "apply_stage8b_p1d4_ack_transition", "removed_ack_transition"),
        ("truth-transition", "stage6", "apply_stage8b_p1d4_truth_transition", "removed_truth_transition"),
        ("journal-ahead-classifier", "stage6", "classify_stage8b_p1d4_generated_market_journal_ahead_candidate", "removed_classifier"),
        ("prepublication-owner", "service", "Stage8bP1d4GeneratedMarketPrepublicationOwner", "RemovedPrepublicationOwner"),
        ("dispatch-owner", "service", "Stage8bP1d4GeneratedMarketDispatchPendingOwner", "RemovedDispatchOwner"),
        ("order-owner", "service", "Stage8bP1d4GeneratedMarketOrderPendingOwner", "RemovedOrderOwner"),
        ("prefinalization-owner", "service", "Stage8bP1d4GeneratedMarketPreFinalizationPendingOwner", "RemovedPreFinalizationOwner"),
        ("preack-owner", "service", "Stage8bP1d4GeneratedMarketPreAckPendingOwner", "RemovedPreAckOwner"),
        ("ack-owner", "service", "Stage8bP1d4GeneratedMarketAckCommittedOwner", "RemovedAckOwner"),
        ("truth-owner", "service", "Stage8bP1d4GeneratedMarketTruthCommittedOwner", "RemovedTruthOwner"),
        ("package-first-routing", "service", "if let Some(package_state) = p1d4_package_state", "if false && p1d4_package_state.is_some()"),
        ("durable-equivalence", "service", "P1d3PreAckPending(Box::new(pending))", "P1d3CancelContinuationPending(Box::new(pending))"),
        ("publication-reserved-id", "redis", "local reserved_id = ARGV[6]", "local reserved_id = '*'"),
        ("publication-predecessor", "redis", "if last_generated_id(command_stream) ~= predecessor_id", "if false"),
        ("publication-explicit-xadd", "redis", "redis.call('XADD', command_stream, reserved_id, 'payload', envelope_payload)", "redis.call('XADD', command_stream, '*', 'payload', envelope_payload)"),
        ("publication-marker", "redis", "redis.call('SET', marker_key, marker_payload)", "-- marker removed"),
        ("marker-write-string", "redis", "prepublication_seal_generation: String", "prepublication_seal_generation: u64"),
        ("marker-read-string", "redis", "binding.prepublication_seal_generation().to_string()", "binding.prepublication_seal_generation()"),
        ("wg-test", "redis", "p1d4_generated_market_write_and_seal_generations_are_independent", "removed_wg_test"),
        ("wg-nonadjacent", "redis", "assert_eq!(binding.prepublication_seal_generation(), 100)", "assert_eq!(binding.prepublication_seal_generation(), 42)"),
        ("high-u64-redis-test", "redis", "p1d4_publication_marker_preserves_high_u64_generation_exactly", "removed_high_u64_redis_test"),
        ("high-u64-value", "redis", "9_007_199_254_740_993", "9_007_199_254_740_992"),
        ("base-matrix", "redis", "p1d4_exact_registry_cells_sigkill_with_duplicate_and_conflict_variants", "removed_base_matrix"),
        ("gm-matrix", "redis", "p1d4_generated_market_registry_extends_base_to_105_with_variants", "removed_gm_matrix"),
        ("base-registry-v5", "redis", "stage8b-p1d4-scenario-frontier-matrix-v5.csv", "stage8b-p1d4-scenario-frontier-matrix-v3.csv"),
        ("gm-registry-v3", "redis", "stage8b-p1d4-generated-market-crash-submatrix-v3.csv", "stage8b-p1d4-generated-market-crash-submatrix-v1.csv"),
        ("duplicate-proof", "redis", "byte-identical duplicate restart drifted", "duplicate proof removed"),
        ("exhaustive-evidence-test", "redis", "p1d4_exhaustive_crash_replay_evidence_two_clean_runs", "removed_exhaustive_evidence_test"),
        ("second-clean-run", "redis", "p1d4_collect_evidence_run(2)", "p1d4_collect_evidence_run(1)"),
        ("gm-pair-marker-path", "redis", "STAGE8B_P1_TEST_SEQUENCE_PAIR_MARKER", "REMOVED_SEQUENCE_PAIR_MARKER"),
        ("gm-pair-exact-equality", "redis", "assert_eq!(continuation.sequence_after, Some(pair)", "assert_eq!(pair.1, pair.0 + 1"),
        ("gm07-no-pair", "redis", 'cell.frontier_id == "GM07"', 'cell.frontier_id == "GM99"'),
        ("evidence-exact-registry", "crash_evidence_checker", "registry field drift", "registry field unchecked"),
        ("evidence-sigkill", "crash_evidence_checker", 'process["exit_signal"] == "signal:9"', 'bool(process["exit_signal"])'),
        ("evidence-semantic-digest", "crash_evidence_checker", "semantic_digest(run)", "str(run)"),
        ("crash-marker-v1-service", "service", "moex.stage8b.p1d4.crash-marker.v1", "moex.stage8b.p1d4.crash-marker.v2"),
        ("crash-marker-v1-core", "stage6", "moex.stage8b.p1d4.crash-marker.v1", "moex.stage8b.p1d4.crash-marker.v2"),
        ("f16-witness-domain", "service", "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v1", "moex.stage8b.p1d4.pre-kill-xack-reply-witness.v2"),
        ("f16-witness-digest-domain", "redis", "moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v1", "moex.stage8b.p1d4.pre-kill-xack-reply-witness.digest.v2"),
        ("f16-witness-create-once", "service", ".p1d4-xack-witness-", ".reusable-xack-witness-"),
        ("f16-witness-final-create-once", "service", ".open(&witness)", ".open(&temporary)"),
        ("f16-witness-reread", "service", "let reread = std::fs::read(&witness)", "let reread = bytes.as_bytes().to_vec()"),
        ("marker-hash-reconstruction", "crash_evidence_checker", 'hashlib.sha256(raw_marker).hexdigest() == filesystem["raw_marker_sha256"]', "True"),
        ("witness-independent-validation", "crash_evidence_checker", "validate_marker_and_witness", "removed_marker_and_witness_validation"),
        ("closed-db0", "evidence", '"operational_redis_db0": false', '"operational_redis_db0": true'),
        ("closed-finam", "evidence", '"finam_post_delete": false', '"finam_post_delete": true'),
        ("closed-live", "evidence", '"runtime_live": false', '"runtime_live": true'),
        ("next-stage", "evidence", '"next_stage_authorized": false', '"next_stage_authorized": true'),
    ]
    passed = 0
    for name, key, old, new in cases:
        mutated = replace(base, key, old, new)
        try:
            check.validate_content(mutated)
        except check.CheckFailure:
            passed += 1
            continue
        raise SystemExit(f"FAIL stage8b-p1d4-source-negative-harness accepted mutation: {name}")
    print(f"PASS stage8b-p1d4-source-negative-harness {passed}/{len(cases)}")


if __name__ == "__main__":
    main()
