#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_c_r2cb_authority_check.py
python3 scripts/stage5g_c_r2cb_negative_harness.py
python3 scripts/stage5g_c_r2ca_r3_predecessor_gate.py
python3 scripts/stage5g_c_r2ca_r3_authority_check.py
python3 scripts/stage5g_c_r2ca_r3_snapshot_gate.py
python3 scripts/stage5g_c_r2ca_r3_authority_negative_harness.py
python3 scripts/stage5g_c_r2ca_r3_semantic_negative_harness.py
cargo test -p broker-finam stage5g_r2cb_finam_full_snapshot_fixture --quiet
cargo test -p strategy-runtime-core stage5g_order_position --quiet
cargo test -p strategy-runtime-core stage5g_r2ca_r3_tests --quiet
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-c-r2cb-gate: PASS"
