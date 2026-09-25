#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
export RUST_MIN_STACK=33554432

PYTHONPATH=scripts python3 scripts/stage8b_p1f_ib_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_ib_negative_harness.py
cargo fmt --all -- --check
cargo test -p runtime-durable-service --lib --all-features -- --test-threads=1
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p runtime-durable-service --all-targets --all-features -- -D warnings
git diff --check 9be356b04a38e627337ed148ccc9fbdaebae8d4a --

if [[ "$(uname -s)" == "Linux" && "$(id -u)" == "0" ]]; then
  scripts/stage8b_p1f_multi_uid_custody_harness.sh
  linux_custody="executed"
else
  echo "INFO stage8b-p1f multi-UID execution deferred to root Linux evidence runner"
  linux_custody="separate-evidence-required"
fi

echo "PASS stage8b-p1f-ib-gate boundary=P1F-Ib scenarios=20 negatives=16 real_process_tests=8 linux_custody=$linux_custody operational=false"
