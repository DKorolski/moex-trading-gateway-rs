#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_design_check.py --no-git
python3 scripts/stage8b_design_negative_harness.py
python3 scripts/stage8b_spec_check.py
python3 scripts/stage8b_spec_negative_harness.py
python3 scripts/stage8b_spec_closed_surface_check.py
python3 scripts/stage8a5_check.py --no-git
python3 -m py_compile scripts/stage8b_spec_check.py scripts/stage8b_spec_negative_harness.py scripts/stage8b_spec_closed_surface_check.py scripts/stage8b_spec_handoff_safety_check.py scripts/make_stage8b_spec_handoff.py
bash -n scripts/stage8b_spec_gate.sh
cargo fmt --all -- --check
git diff --check

echo "stage8b-spec-gate: PASS rows=100 negatives=90 corrective_specification=true implementation=false execution=false finam=false redis=false dispatch=false live=false stage8b_i=false stage8b_p=false stage8b_xt=false stage8b_xe=false stage12=false"
