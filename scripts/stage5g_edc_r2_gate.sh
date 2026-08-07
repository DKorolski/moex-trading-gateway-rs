#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_edc_r2_check.py
python3 scripts/stage5g_edc_r2_negative_harness.py
python3 scripts/stage5g_edc_r2_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_edc_
cargo test --release -p strategy-runtime-core --lib stage5g_edc_
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edc-r2.XXXXXX")"
cleanup() { rm -rf "$snapshot"; }
trap cleanup EXIT
git clone --quiet --no-hardlinks . "$snapshot/predecessor"
(
  cd "$snapshot/predecessor"
  git checkout --quiet -B stage5g-lifecycle 67e13aeecd3bf0dc33e570770b0e4b90f5fec0cf
  bash scripts/stage5g_edc_r1_gate.sh
)
echo "stage5g-edc-r2-gate: PASS current-negative=98/98 aggregate-negative=522/522"
