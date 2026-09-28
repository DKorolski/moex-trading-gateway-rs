#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_telemetry_composition_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_telemetry_composition_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_negative_harness.py
cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features telemetry_ -- --test-threads=1
cargo test -p runtime-durable-service --all-features production_process_publishes_ready_drain_and_stop_telemetry -- --test-threads=1
cargo test -p runtime-durable-service --all-features production_heartbeat_turns_draining_while_ready_poll_response_is_withheld -- --test-threads=1
cargo test -p runtime-durable-service --all-features retained_signed_market_ack_exposes_exact_terminal_snapshot_and_pel -- --test-threads=1
cargo clippy -p runtime-durable-service --all-targets --all-features -- -D warnings
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings
git diff --check HEAD --

echo "PASS stage8b-p1e-i1-telemetry-composition-gate"
