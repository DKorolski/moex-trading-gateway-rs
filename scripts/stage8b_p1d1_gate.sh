#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage8b_p1d1_check.py
python3 scripts/stage8b_p1d1_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1d1_check.py \
  scripts/stage8b_p1d1_negative_harness.py \
  scripts/make_stage8b_p1d1_handoff.py \
  scripts/stage8b_p1d1_handoff_safety_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage8b_p1d1 --lib
cargo test -p runtime-durable-service canonical_roundtrip_and_stage5c_admission_are_exact --lib
cargo test -p runtime-durable-service p1c_command_response_loss_republishes_exactly_once_and_retains_m10 --lib
cargo test -p runtime-durable-service journal_ahead_exception_rejects_dispatch_attempt_suffix --lib
cargo test -p strategy-runtime-core --lib
cargo test -p runtime-durable-service --lib
cargo test -p strategy-runtime-core --doc
cargo test -p runtime-durable-service --doc
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1d1-gate rows=30 negatives=18 market=true db0=false finam=false ack=false xack=false"
