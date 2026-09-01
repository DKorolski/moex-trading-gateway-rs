#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 -m py_compile \
  scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_check.py \
  scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_negative_harness.py \
  scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_terminal_oracle.py \
  scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight.py \
  scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_check.py
python3 scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_negative_harness.py
python3 scripts/stage8b_p_r2b_generation2_full_transaction_native_r0_host_preflight_negative_harness.py
git diff --check

echo "stage8b-generation2-full-transaction-native-r0-gate: PASS native_execution=false authorization=NOT_ISSUED"
