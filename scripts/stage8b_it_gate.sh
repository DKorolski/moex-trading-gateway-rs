#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_design_check.py --no-git
python3 scripts/stage8b_spec_check.py --no-git
python3 scripts/stage8b_i_check.py
python3 scripts/stage8b_it_check.py
python3 scripts/stage8b_it_negative_harness.py
bash scripts/stage8b_it_external_compile_fail.sh
bash scripts/stage8b_it_internal_compile_fail.sh
cargo test -p finam-gateway stage8b_no_send --no-default-features -- --test-threads=1
cargo test -p finam-gateway stage8b_adapter --no-default-features -- --test-threads=1
cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
bash scripts/stage8b_i_full_regression.sh

echo "stage8b-it-gate: PASS revision=R2 rows=72 negatives=60 external_compile_fail=12 internal_compile_fail=4 canonical_full_regression=true adapter=1 post=1 delete=1 send=1 controlled_only=true broker_effect=false stage8b_p=false stage8b_xe=false stage12=false"
