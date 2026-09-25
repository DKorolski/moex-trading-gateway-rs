#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1f_ic_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_ic_negative_harness.py
cargo fmt --all -- --check
cargo test -p finam-gateway --all-features stage8b_p1f_fixed_producers -- --test-threads=1
cargo test -p finam-gateway --all-features stage8b_p1e_schedule_publisher -- --test-threads=1
cargo test -p runtime-durable-service --all-features \
  stage8b_p1f_guardian::tests::o2_materialization_finalizes_only_source_hash_and_replays_exactly \
  -- --test-threads=1
cargo test -p finam-gateway -- --test-threads=1
cargo test -p finam-gateway --doc --all-features
cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
git diff --check 7c481bc60699b514b016e8dffe62eb9ca462a100 --

echo "PASS stage8b-p1f-ic-gate boundary=P1F-Ic scenarios=24 negatives=24 targeted_tests=4 operational=false"
