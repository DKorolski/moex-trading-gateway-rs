#!/usr/bin/env bash
set -euo pipefail

cargo test -p strategy-runtime-core \
  stage5g_p1_semantic::tests::p1_multi_intent_boundary_returns_only_noncontinuable_diagnostic
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::canonical_roundtrip_and_stage5c_admission_are_exact
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::active_entry_cannot_be_trimmed_and_delivery_is_exact_bound
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::journal_ahead_rejects_
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::journal_ahead_exception_rejects_dispatch_attempt_suffix
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::subprocess_kill_matrix_recovers_all_seven_prepublication_frontiers
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::subprocess_kill_after_zero_intent_s1_recovers_exact_ack_only
cargo test -p runtime-durable-service \
  stage8b_p1_semantic::tests::zero_intent_ack_recovery_rejects_
cargo test -p runtime-durable-service \
  recovery::tests::stage7b_c_b040_unbound_nonfinal_file_journal_blocks_without_effect
cargo test -p runtime-durable-service --doc

echo "PASS stage8b-p1b-semantic-negative-harness"
