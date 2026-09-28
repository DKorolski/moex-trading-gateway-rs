#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

scripts/stage8b_p1e_i1_foundation_r2_gate.sh
scripts/stage8b_p1e_i1a_design_gate.sh
python3 scripts/stage8b_p1e_i1a_r1_design_check.py
python3 scripts/stage8b_p1e_i1a_r1_semantic_model.py
python3 scripts/stage8b_p1e_i1a_r1_negative_harness.py

echo "stage8b-p1e-i1a-r1-design-gate: ok"
