#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_eb_r1_check.py
python3 scripts/stage5g_eb_r1_negative_harness.py
python3 scripts/stage5g_eb_r1_predecessor_gate.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5ge_b_r1 --quiet
cargo test -p strategy-runtime-core stage5ge_b_ --quiet
cargo test -p strategy-runtime-core --release stage5ge_b_r1 --quiet
cargo test -p strategy-runtime-core --release stage5ge_b_ --quiet
cargo test -p strategy-runtime-core --doc stage5g_timer --quiet
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-eb-r1-gate: PASS"
