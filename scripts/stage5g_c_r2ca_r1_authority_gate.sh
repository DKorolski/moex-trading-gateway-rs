#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_c_r2ca_r1_authority_check.py
python3 scripts/stage5g_c_r2ca_r1_snapshot_gate.py
python3 scripts/stage5g_c_r2ca_r1_authority_negative_harness.py
python3 scripts/stage5g_c_r2ca_r1_semantic_negative_harness.py
cargo test -p strategy-runtime-core stage5g_r2ca
cargo test -p broker-core trading_window_closed_ack_preserves_confirmed_and_deferred_semantics
cargo test -p strategy-runtime-core production_public_submitted_then_recovered_resolves_stage5c_once
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-c-r2ca-r1-authority-gate: PASS"
