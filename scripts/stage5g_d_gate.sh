#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_d_check.py
python3 scripts/stage5g_d_negative_harness.py
python3 scripts/stage5g_d_predecessor_gate.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5g_timer --quiet
cargo test -p strategy-runtime-core stage5gd_ --quiet
cargo test -p strategy-runtime-core --release stage5g_timer --quiet
cargo test -p strategy-runtime-core --release stage5gd_ --quiet
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-d-gate: PASS"
