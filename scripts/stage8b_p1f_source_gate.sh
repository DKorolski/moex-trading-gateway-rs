#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1f_source_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_source_negative_harness.py
cargo fmt --all -- --check
cargo test -p runtime-durable-service --lib --all-features -- --test-threads=1
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p runtime-durable-service --all-targets --all-features -- -D warnings
git diff --check 5d81b8e212300858246237a227a95d115dd67c2d --

if [[ "$(uname -s)" == "Linux" && "$(id -u)" == "0" ]]; then
  scripts/stage8b_p1f_multi_uid_custody_harness.sh
  multi_uid="executed"
else
  echo "INFO stage8b-p1f-multi-uid execution deferred to root Linux evidence runner"
  multi_uid="separate-evidence-required"
fi

echo "PASS stage8b-p1f-source-gate boundary=P1F-Ia guardian=true inherited=true negatives=19 multi_uid=$multi_uid operational=false"
