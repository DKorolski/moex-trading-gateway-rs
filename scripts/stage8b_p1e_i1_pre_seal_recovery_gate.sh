#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1e_i1_pre_seal_recovery_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_pre_seal_recovery_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i1_pre_seal_recovery_check.py \
  scripts/stage8b_p1e_i1_pre_seal_recovery_negative_harness.py

cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features \
  stage8b_p1e_first_boot_source::tests::pre_seal_ -- --test-threads=1

bash scripts/stage8b_p1e_i1_transaction_v5_gate.sh

echo "PASS stage8b-p1e-i1-pre-seal-recovery-gate actions=7 continuable=4 quarantine_frontiers=2 response_loss_hooks=4 generation_guard=true redis=false finam=false dispatch=false live=false"
