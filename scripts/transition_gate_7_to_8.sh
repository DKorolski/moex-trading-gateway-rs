#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

python3 scripts/transition_gate_7_to_8_check.py
python3 scripts/transition_gate_7_to_8_negative_harness.py
python3 -m json.tool docs/stage-8/transition-gate-7-to-8-descriptor.json >/dev/null
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "transition-gate-7-to-8: PASS rows=45 negatives=20 stage8-implementation=closed"
