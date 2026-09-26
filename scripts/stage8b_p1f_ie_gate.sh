#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1f_ie_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_ie_negative_harness.py
bash scripts/stage8b_p1f_ie_linked_local_composition.sh
cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features -- --test-threads=1
cargo test -p finam-gateway -- --test-threads=1
cargo test -p runtime-durable-service --doc --all-features
cargo test -p finam-gateway --doc --all-features
cargo clippy -p runtime-durable-service -p finam-gateway --all-targets --all-features -- -D warnings
git diff --check 512db6e6e652a2b0a15be7b6dcb72b96e231950d --

echo "PASS stage8b-p1f-ie-gate accepted_sources=4 linked_steps=9 rows=20 operational=false"
