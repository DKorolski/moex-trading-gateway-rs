#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all -- --check
python3 scripts/stage8b_p1c_check.py
python3 scripts/stage8b_p1c_negative_harness.py
cargo test -p runtime-durable-service stage8b_p1_semantic::redis::tests --no-fail-fast
bash scripts/stage8b_p1b_semantic_negative_harness.sh
cargo test -p strategy-runtime-core --lib
cargo test -p runtime-durable-service --lib
cargo test -p runtime-durable-service --doc
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1c-gate"
