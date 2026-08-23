#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_design_check.py --no-git
python3 scripts/stage8b_design_negative_harness.py
python3 scripts/stage8b_spec_check.py --no-git
python3 scripts/stage8b_spec_negative_harness.py
python3 scripts/stage8b_i_check.py
python3 scripts/stage8b_i_negative_harness.py
python3 scripts/stage8b_i_closed_surface_check.py
bash scripts/stage8b_i_external_compile_fail.sh
cargo test -p finam-gateway stage8b_no_send --no-default-features
cargo test -p broker-cli --test stage8b_i_no_send_facade --no-default-features
cargo clippy -p finam-gateway -p broker-cli --all-targets --no-default-features -- -D warnings
cargo fmt --all -- --check
git diff --check
bash scripts/stage8b_i_full_regression.sh

echo "stage8b-i-gate: PASS revision=R3 rows=104 negatives=82 compile_fail=20 canonical_regression=true no_send=true adapter=false finam=false redis=false dispatch=false live=false real_orders=false stage8b_it=false stage8b_p=false stage8b_xe=false stage12=false"
