#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/stage8b_design_check.py --no-git
python3 scripts/stage8b_spec_check.py --no-git
python3 scripts/stage8b_i_check.py
python3 scripts/stage8b_it_check.py
python3 scripts/stage8b_it_negative_harness.py
bash scripts/stage8b_it_external_compile_fail.sh
cargo test -p finam-gateway stage8b_no_send --no-default-features -- --test-threads=1
cargo test -p finam-gateway stage8b_adapter --no-default-features -- --test-threads=1
cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check

echo "stage8b-it-gate: PASS rows=60 negatives=48 compile_fail=12 adapter=1 post=1 delete=1 send=1 controlled_only=true broker_effect=false stage8b_p=false stage8b_xe=false stage12=false"
