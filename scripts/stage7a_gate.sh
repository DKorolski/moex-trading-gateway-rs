#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
artifact_dir="${STAGE7A_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage7a-artifact.$$}"
mkdir -p "$artifact_dir"

command -v redis-server >/dev/null
cargo fmt --all -- --check
echo "fmt: PASS" | tee "$artifact_dir/fmt.txt"
python3 scripts/stage7a_check.py | tee "$artifact_dir/stage7a-check.txt"
python3 scripts/stage7a_negative_harness.py | tee "$artifact_dir/negative.txt"
python3 scripts/stage7a_closed_surface_check.py | tee "$artifact_dir/closed-surface.txt"

cargo test -p runtime-command-bridge --no-fail-fast 2>&1 | tee "$artifact_dir/bridge-debug.txt"
cargo test --release -p runtime-command-bridge --no-fail-fast 2>&1 | tee "$artifact_dir/bridge-release.txt"
cargo test -p strategy-runtime-core stage7a_ --no-fail-fast 2>&1 | tee "$artifact_dir/core-debug.txt"
cargo test --release -p strategy-runtime-core stage7a_ --no-fail-fast 2>&1 | tee "$artifact_dir/core-release.txt"
cargo test -p strategy-runtime-core gack07_duplicate_requires_prior_outcome_and_exact_duplicate_is_noop --no-fail-fast 2>&1 | tee "$artifact_dir/stage5g-ack-oracle-a.txt"
cargo test -p strategy-runtime-core duplicate_ack_terminal_twice_and_expired_lifecycle_block --no-fail-fast 2>&1 | tee "$artifact_dir/stage5g-ack-oracle-b.txt"
cat "$artifact_dir/stage5g-ack-oracle-a.txt" "$artifact_dir/stage5g-ack-oracle-b.txt" > "$artifact_dir/stage5g-ack-oracle.txt"
cargo test -p strategy-runtime-core --lib stage6 --no-fail-fast 2>&1 | tee "$artifact_dir/stage6-regression.txt"
cargo test --workspace --all-targets 2>&1 | tee "$artifact_dir/workspace-tests.txt"
cargo test --workspace --doc 2>&1 | tee "$artifact_dir/workspace-docs.txt"
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | tee "$artifact_dir/clippy.txt"

if [[ "${STAGE7A_SKIP_PRESEAL:-0}" != "1" ]]; then
  python3 scripts/stage7a_preseal_check.py | tee "$artifact_dir/preseal.txt"
  python3 scripts/stage7a_r1_acceptance_report.py \
    --artifact-dir "$artifact_dir" \
    --output "$artifact_dir/stage7a-r1-acceptance.json" \
    | tee "$artifact_dir/stage7a-r1-acceptance.txt"
else
  echo "stage7a-r1-acceptance: DEFERRED until clean committed preseal"
fi
rustc --version | tee "$artifact_dir/toolchain.txt"
cargo --version | tee -a "$artifact_dir/toolchain.txt"
echo "stage7a-gate: PASS artifact_dir=$artifact_dir"
