#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"

python3 scripts/stage8b_p1e_i1_foundation_r1_check.py
python3 scripts/stage8b_p1e_i1_foundation_r1_negative_harness.py
cargo fmt --check
cargo test -p runtime-durable-service stage8b_p1_supervisor --lib
cargo clippy -p runtime-durable-service --lib --tests -- -D warnings

echo "stage8b-p1e-i1-foundation-r1-gate: ok"
