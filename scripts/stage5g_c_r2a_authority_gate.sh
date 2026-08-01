#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_c_r2a_authority_check.py
python3 scripts/stage5g_c_r2a_authority_negative_harness.py
python3 scripts/stage5c_api_freeze_check.py

echo "stage5g-c-r2a-authority-gate: PASS"
