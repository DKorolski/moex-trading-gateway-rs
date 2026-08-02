#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_d_r1a_r1_authority_check.py
python3 scripts/stage5g_d_r1a_r1_negative_harness.py
python3 scripts/stage5g_d_r1a_r1_predecessor_gate.py
python3 scripts/stage5c_api_freeze_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5gd_r1a --quiet
cargo test -p strategy-runtime-core --release stage5gd_r1a --quiet
bash scripts/stage5f_forbidden_no_rg_gate.sh
git diff --check 0f72478123c8ddf90c5368ce0cef7867257087c3

echo "stage5g-d-r1a-r1-gate: PASS"
