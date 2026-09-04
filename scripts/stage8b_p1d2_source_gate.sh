#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1d2_source_check.py
python3 scripts/stage8b_p1d2_source_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d2_source_check.py \
  scripts/stage8b_p1d2_source_negative_harness.py \
  scripts/make_stage8b_p1d2_source_handoff.py \
  scripts/stage8b_p1d2_source_handoff_safety_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib --all-features
cargo test -p runtime-durable-service --lib --all-features
cargo test -p strategy-runtime-core --doc --all-features
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1d2-source-gate rows=44 negatives=26 crash=6 pair_crash=true schedule=source-authority db0=false finam=false live=false"
