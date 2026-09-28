#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

scripts/stage8b_p1e_i1a_r1_design_gate.sh
python3 scripts/stage8b_p1e_i1a_r2_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_r2_semantic_model.py
python3 scripts/stage8b_p1e_i1a_r2_negative_harness.py

echo "stage8b-p1e-i1a-r2-design-gate: ok"
