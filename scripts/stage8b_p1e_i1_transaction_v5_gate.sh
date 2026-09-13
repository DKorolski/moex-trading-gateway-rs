#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1e_i1_transaction_v5_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py
python3 -m py_compile \
  scripts/make_stage8b_p1e_i1_transaction_v5_handoff.py \
  scripts/stage8b_p1e_i1_transaction_v5_check.py \
  scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py \
  scripts/stage8b_p1e_i1_transaction_v5_handoff_safety_check.py

cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage8b_p1e_v2_ -- --nocapture
cargo test -p runtime-durable-service \
  stage8b_p1e_first_boot_transaction::tests::derived_digest_framing_matches_all_accepted_golden_vectors \
  -- --exact --nocapture
cargo test -p runtime-durable-service \
  stage8b_p1e_first_boot_source::tests::composition_exports_restores_then_creates_the_single_durable_root \
  -- --exact --nocapture
cargo test -p runtime-durable-service \
  stage8b_p1e_first_boot_source::tests::every_v5_crash_hook_has_one_exact_fail_closed_classification \
  -- --exact --nocapture

cargo test -p strategy-runtime-core --lib
RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib -- --test-threads=1
cargo test -p strategy-runtime-core --doc
cargo test -p runtime-durable-service --doc
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1e-i1-transaction-v5-gate classifications=15 crash_hooks=10 post_seal_recovery=4 provenance_v2=true redis=false finam=false dispatch=false live=false"
