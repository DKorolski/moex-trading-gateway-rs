#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

python3 scripts/stage8b_p1e_i1a_design_check.py
python3 scripts/stage8b_p1e_i1a_design_negative_harness.py

echo "stage8b-p1e-i1a-design-gate: ok"
