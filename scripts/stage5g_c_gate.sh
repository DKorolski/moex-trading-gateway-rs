#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

bash scripts/stage5g_c_predecessor_snapshot_gate.sh
python3 scripts/stage5g_c_check.py
python3 scripts/stage5g_c_negative_harness.py
cargo test -p strategy-runtime-core stage5g_order_position --no-fail-fast
cargo test -p strategy-runtime-core stage5gc_r1_public --no-fail-fast
cargo test --release -p strategy-runtime-core stage5g_order_position --no-fail-fast
cargo test --release -p strategy-runtime-core stage5gc_r1_public --no-fail-fast

echo "stage5g-c-r1-gate: PASS focused=23/23 public=5/5 negative=16/16 predecessor=green"
