#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_edc_r1_check.py
python3 scripts/stage5g_edc_r1_negative_harness.py
python3 scripts/stage5g_edc_r1_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_edc_
cargo test --release -p strategy-runtime-core --lib stage5g_edc_
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edc-r1.XXXXXX")"
cleanup() { rm -rf "$snapshot"; }
trap cleanup EXIT
git clone --quiet --no-hardlinks . "$snapshot/predecessor"
(
  cd "$snapshot/predecessor"
  git checkout --quiet -B stage5g-lifecycle 18240b26a5bea77ea71c851f72a644706a7e0b57
  bash scripts/stage5g_edc_gate.sh
)
echo "stage5g-edc-r1-gate: PASS aggregate-negative=424/424"
