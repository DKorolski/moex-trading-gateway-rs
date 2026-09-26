#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

run_runtime() {
  bash scripts/stage8b_p1f_ie_exact_test.sh runtime-durable-service "$1"
}

run_finam() {
  bash scripts/stage8b_p1f_ie_exact_test.sh finam-gateway "$1"
}

run_runtime stage8b_p1f_guardian::tests::o2_materialization_finalizes_only_source_hash_and_replays_exactly
run_runtime stage8b_p1f_guardian::tests::local_supervision_starts_after_admission_and_stops_on_sigterm
run_finam stage8b_p1f_fixed_producers::tests::fixed_o3_o4_schedule_uses_one_retained_sequence_and_revision
run_finam stage8b_p1f_fixed_producers::tests::id_linked_real_redis_response_loss_restarts_prepared_without_duplicate
run_runtime stage8b_p1f_fixed_redis::tests::resource_probe_uses_real_bounded_redis_reads_and_hash_only_audit
run_runtime stage8b_p1_semantic::redis::tests::p1c_journal_ahead_reclaims_real_pel_before_reconstructing_s1
run_runtime stage8b_p1e_process::tests::p1f_id_process_supervision_retains_failed_attach_audit_after_early_owner_return
run_runtime stage8b_p1e_process::tests::p1f_id_process_supervision_retains_audit_after_owner_abort
run_runtime stage8b_p1e_process::tests::production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly

echo "PASS stage8b-p1f-ie-aggregate-regression-suite steps=9 exact_selected=9 operational=false"
