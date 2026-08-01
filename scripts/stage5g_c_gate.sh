#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

bash scripts/stage5g_b_r3_snapshot_gate.sh
python3 scripts/stage5g_c_check.py
python3 scripts/stage5g_c_negative_harness.py
cargo test -p strategy-runtime-core stage5g_order_position --no-fail-fast
cargo test -p strategy-runtime-core stage5gc_public_terminal_ack_converges_without_broker_callback
cargo test --release -p strategy-runtime-core stage5g_order_position --no-fail-fast

echo "stage5g-c-gate: PASS gop=16/16 negative=10/10 inherited-r3=green"
