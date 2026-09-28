#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1e_i1_pre_seal_recovery_correction_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_pre_seal_recovery_correction_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i1_pre_seal_recovery_correction_check.py \
  scripts/stage8b_p1e_i1_pre_seal_recovery_correction_negative_harness.py

cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features \
  pre_seal_administrative_recovery_remains_available_after_long_downtime -- --test-threads=1
cargo test -p runtime-durable-service --all-features \
  administrative_recovery_rejects_wrong_selector_and_marker_auth_without_mutation -- --test-threads=1
cargo test -p runtime-durable-service --all-features \
  historical_continuation_is_marker_bound_at_300_301_and_long_downtime -- --test-threads=1
cargo test -p runtime-durable-service --all-features \
  historical_continuation_rejects_changed_bundle_without_mutation -- --test-threads=1

bash scripts/stage8b_p1e_i1_pre_seal_recovery_gate.sh

echo "PASS stage8b-p1e-i1-pre-seal-recovery-correction-gate fresh_admission_max_age=300 historical_exact=true admin_without_f00=true redis=false finam=false dispatch=false live=false"
