#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --all -- --check
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::os_process_ -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::production_run_signals_cover_admission_attach_and_s06_without_effects -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::production_run_cancels_server_processed_redis_attach_and_s06_requests -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::common_supervisor_maps_noncooperative_owner_grace_expiry_to_72 -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::production_v5_bootstrap_runs_continuous_market_lifecycle_and_readmits_exactly -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1e_process::tests::process_wrapper_ -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  ordinary_run_admission_ -- --test-threads=1
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --all-features --lib \
  stage8b_p1_supervisor::tests:: -- --test-threads=1
cargo clippy -p runtime-durable-service --all-targets --all-features -- -D warnings
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_transaction_v5_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
git diff --check HEAD --

echo "PASS stage8b-p1e-i1-process-supervision-gate"
