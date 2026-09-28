#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1f_o2_check.py
python3 scripts/stage8b_p1f_o2_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1f_o2_check.py \
  scripts/stage8b_p1f_o2_negative_harness.py
cargo test -p runtime-durable-service \
  stage8b_p1f_guardian::tests::genesis_activation_and_exact_same_active_continuation \
  -- --exact
cargo test -p runtime-durable-service \
  stage8b_p1f_guardian::tests::o2_materialization_finalizes_only_source_hash_and_replays_exactly \
  -- --exact
cargo test -p runtime-durable-service \
  stage8b_p1e_first_boot_source::tests::exact_source_bundle_is_accepted \
  -- --exact
echo "PASS stage8b-p1f-o2-gate execution=false"
