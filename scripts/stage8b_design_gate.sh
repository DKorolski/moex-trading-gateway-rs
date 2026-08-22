#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_design_check.py
python3 scripts/stage8b_design_negative_harness.py
python3 scripts/stage8b_design_closed_surface_check.py
python3 scripts/stage8a5_check.py --no-git
python3 -m py_compile \
  scripts/stage8b_design_check.py \
  scripts/stage8b_design_negative_harness.py \
  scripts/stage8b_design_closed_surface_check.py \
  scripts/stage8b_design_handoff_safety_check.py \
  scripts/make_stage8b_design_handoff.py
bash -n scripts/stage8b_design_gate.sh
cargo fmt --all -- --check
git diff --check

echo "stage8b-design-gate: PASS rows=70 negatives=50 design_only=true implementation=false execution=false finam=false redis=false dispatch=false live=false stage8b_s=false stage12=false"
