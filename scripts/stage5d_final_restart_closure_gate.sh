#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all --check
cargo test -p strategy-runtime-core stage5d_final -- --nocapture
cargo test -p strategy-runtime-core stage5d_b2bc1r3 --lib
cargo test -p strategy-runtime-core stage5d_b2bd1 --lib
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py
