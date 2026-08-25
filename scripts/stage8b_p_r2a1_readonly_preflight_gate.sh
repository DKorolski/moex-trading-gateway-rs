#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bash scripts/stage8b_p_r1b_identity_gate.sh
python3 scripts/stage8b_p_r2a1_readonly_preflight_check.py
python3 scripts/stage8b_p_r2a1_readonly_preflight_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r2a1-network-topology-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a1-query-policy-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2a1-current-source-authority.json >/dev/null
cargo fmt --manifest-path tools/stage8b-readonly-preflight/Cargo.toml -- --check
cargo test --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
cargo clippy --manifest-path tools/stage8b-readonly-preflight/Cargo.toml --all-targets -- -D warnings
cargo build --release --manifest-path tools/stage8b-readonly-preflight/Cargo.toml
git diff --check

echo "stage8b-p-r2a1-gate: PASS sources=17 mock_only=true real_http=false arm=false attempt=false effect_transport=false order_post_delete=false authorization=NOT_ISSUED"
