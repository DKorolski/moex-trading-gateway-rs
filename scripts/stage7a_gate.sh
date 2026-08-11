#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7A_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7a-artifact.$$}"
mkdir -p "$artifact_dir"

command -v redis-server >/dev/null
cargo fmt --all -- --check
python3 scripts/stage7a_check.py
python3 scripts/stage7a_negative_harness.py | tee "$artifact_dir/stage7a-negative.txt"
python3 scripts/stage7a_closed_surface_check.py

cargo test -p runtime-command-bridge --no-fail-fast
cargo test --release -p runtime-command-bridge --no-fail-fast
cargo test -p strategy-runtime-core stage7a_ --no-fail-fast
cargo test --release -p strategy-runtime-core stage7a_ --no-fail-fast
cargo test -p strategy-runtime-core --lib stage6 --no-fail-fast
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets --all-features -- -D warnings

if [[ "${STAGE7A_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage7a_preseal_check.py
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7a-gate: PASS artifact_dir=$artifact_dir"
