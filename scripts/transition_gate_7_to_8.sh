#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

python3 scripts/transition_gate_7_to_8_check.py
python3 scripts/transition_gate_7_to_8_negative_harness.py
python3 -m json.tool docs/stage-8/transition-gate-7-to-8-descriptor.json >/dev/null
python3 -m json.tool docs/stage-8/finam-rest-order-contract-snapshot-2026-08-14.json >/dev/null
python3 -m json.tool docs/stage-8/finam-rest-order-contract-evidence-2026-08-14.json >/dev/null
cargo fmt --all -- --check
# The inherited Stage 7B suite contains real Redis/SIGKILL barrier witnesses.
# Serialize test binaries so two infrastructure witnesses cannot compete for a
# timing barrier during immutable handoff generation.
cargo test --workspace --all-targets -- --test-threads=1
cargo test --workspace --doc -- --test-threads=1
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "transition-gate-7-to-8: PASS r1 rows=66 negatives=32 contract=current stage8-implementation=closed"
