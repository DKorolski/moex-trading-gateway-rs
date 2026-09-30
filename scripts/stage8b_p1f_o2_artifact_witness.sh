#!/usr/bin/env bash
set -euo pipefail

cargo test -p broker-finam o2_readonly::tests:: -- --nocapture
cargo test -p finam-gateway stage8b_p1f_o2_materializer::tests:: -- --nocapture
cargo test --locked -p finam-gateway --bin stage8b-p1f-o2-materializer -- --nocapture
cargo test --locked -p runtime-durable-service --lib \
  stage8b_p1f_guardian::tests::o2_selector_ -- --nocapture
cargo test -p runtime-durable-service stage8b_p1f_o2_systemd::tests:: -- --nocapture

for test_name in \
  pending_claim_and_terminal_transactions_resume_without_new_authority \
  execution_owner_is_unique_and_stopping_resume_has_no_new_grace \
  stopping_monotonic_bound_rejects_frozen_and_backward_wall_clock \
  retained_pre_spawn_signal_prevents_child_start \
  signal_supervision_loss_is_nonzero_and_leaves_no_child \
  pending_stopping_frontier_recovers_through_ib_without_child
do
  cargo test -p runtime-durable-service \
    "stage8b_p1f_guardian::tests::${test_name}" -- --exact --nocapture
done

bash scripts/stage8b_p1f_ie_composition_witness.sh
echo "PASS stage8b-p1f-o2-artifact-witness execution=false"
