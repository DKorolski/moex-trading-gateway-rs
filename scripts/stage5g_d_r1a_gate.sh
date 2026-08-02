#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_d_r1a_authority_check.py
python3 scripts/stage5g_d_r1a_negative_harness.py
python3 scripts/stage5c_api_freeze_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core stage5gd_r1a --quiet
cargo test -p strategy-runtime-core --release stage5gd_r1a --quiet
bash scripts/stage5f_forbidden_no_rg_gate.sh
git diff --check bc4cabfff42eafee48733296f121a8a6e2f42dd8

echo "stage5g-d-r1a-gate: PASS"
