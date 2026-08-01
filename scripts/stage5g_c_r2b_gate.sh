#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_c_r2b_snapshot_gate.py
python3 scripts/stage5g_c_r2b_snapshot_negative_harness.py
bash scripts/stage5g_c_r2a_authority_gate.sh
python3 scripts/stage5g_c_r2b_semantic_negative_harness.py

echo "stage5g-c-r2b-gate: PASS"
