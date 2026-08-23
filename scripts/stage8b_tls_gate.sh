#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
bash scripts/stage8b_tls_predecessor_replay.sh
python3 scripts/stage8b_tls_qualification_check.py
python3 scripts/stage8b_tls_negative_harness.py
python3 scripts/stage8b_tls_graph_evidence.py
cargo test -p finam-gateway stage8b_no_send::tests::it_tls --no-default-features -- --test-threads=1
cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
git diff --check
bash scripts/stage8b_i_full_regression.sh

echo "stage8b-tls-gate: PASS revision=R1 rows=50 negatives=40 focused_tls_tests=5 h2=true rustls=true native_tls=false canonical_full_regression=true finam=false broker_effect=false stage8b_p=false stage8b_xe=false stage12=false"
