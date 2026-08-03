#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_ec_check.py
python3 scripts/stage5g_ec_negative_harness.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5ge_c_ --quiet
cargo test -p strategy-runtime-core --release stage5ge_c_ --quiet
cargo test -p strategy-runtime-core --doc --quiet
python3 scripts/stage5c_api_freeze_check.py
bash scripts/stage5f_forbidden_no_rg_gate.sh

echo "stage5g-ec-gate: PASS"
