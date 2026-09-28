#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1f_id_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_id_negative_harness.py
cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features stage8b_p1f_fixed_redis -- --test-threads=1
cargo test -p finam-gateway --all-features stage8b_p1f_fixed_producers -- --test-threads=1
cargo test -p finam-gateway --all-features stage8b_p1e_schedule_publisher -- --test-threads=1
cargo test -p runtime-durable-service --all-features -- --test-threads=1
cargo test -p finam-gateway -- --test-threads=1
cargo test -p runtime-durable-service --doc --all-features
cargo test -p finam-gateway --doc --all-features
cargo clippy -p runtime-durable-service -p finam-gateway --all-targets --all-features -- -D warnings
git diff --check 5c2656fbe8691da256b5380dd16ce6f6b6aa1fa8 --

echo "PASS stage8b-p1f-id-gate roles=8 operations=10 scripts=8 operational=false"
