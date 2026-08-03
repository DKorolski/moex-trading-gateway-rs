#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_e_check.py
python3 scripts/stage5g_e_negative_harness.py
python3 scripts/stage5g_e_predecessor_gate.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5ge_a --quiet
cargo test -p strategy-runtime-core stage5g_timer --quiet
cargo test -p strategy-runtime-core --release stage5ge_a --quiet
cargo test -p strategy-runtime-core --release stage5g_timer --quiet
cargo test -p strategy-runtime-core --doc stage5g_timer --quiet
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-e-gate: PASS"
