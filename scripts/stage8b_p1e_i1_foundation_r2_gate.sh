#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

bash scripts/stage8b_p1e_i1_foundation_r1_gate.sh
python3 scripts/stage8b_p1e_i1_foundation_r2_check.py
python3 scripts/stage8b_p1e_i1_foundation_r2_negative_harness.py

echo "stage8b-p1e-i1-foundation-r2-gate: ok"
