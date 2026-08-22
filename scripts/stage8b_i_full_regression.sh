#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

bash scripts/current_tree_ci_gate.sh
bash scripts/test_m4_3x_evidence_no_redis.sh
cargo fmt --all --check
cargo test --workspace --all-targets -- --test-threads=1
cargo test --workspace --release --all-targets -- --test-threads=1
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

redis_url="${FINAM_GATEWAY_REDIS_URL:-redis://127.0.0.1:6379/}"
if ! redis-cli -u "$redis_url" ping | grep -qx PONG; then
  echo "stage8b-i-full-regression: FAIL Redis unavailable at configured isolated smoke URL" >&2
  exit 1
fi
scripts/redis_shadow_smoke.sh
scripts/runtime_bridge_dry_smoke.sh

echo "stage8b-i-full-regression: PASS canonical_ci=true debug=true release=true doctest=true clippy_all_features=true redis_shadow=true runtime_bridge=true"
