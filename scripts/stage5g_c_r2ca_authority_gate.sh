#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_c_r2ca_authority_check.py
python3 scripts/stage5g_c_r2ca_snapshot_gate.py
python3 scripts/stage5g_c_r2ca_authority_negative_harness.py
cargo test -p strategy-runtime-core stage5g_r2ca

echo "stage5g-c-r2ca-authority-gate: PASS"
