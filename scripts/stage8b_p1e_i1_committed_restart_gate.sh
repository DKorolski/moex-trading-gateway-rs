#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --all -- --check
cargo test -p runtime-durable-service --all-features committed_
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings
cargo clippy -p runtime-durable-service --all-targets --all-features -- -D warnings
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
git diff --check HEAD --

echo "PASS stage8b-p1e-i1-committed-restart-gate"
