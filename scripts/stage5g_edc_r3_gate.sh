#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_edc_r3_check.py
python3 scripts/stage5g_edc_r3_negative_harness.py
python3 scripts/stage5g_edc_r3_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_edc_r3_
cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_
cargo test -p strategy-runtime-core --lib stage5g_edc_
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-edc-r3.XXXXXX")"
cleanup() { rm -rf "$snapshot"; }
trap cleanup EXIT
git clone --quiet --no-hardlinks . "$snapshot/predecessor"
(
  cd "$snapshot/predecessor"
  git checkout --quiet -B stage5g-lifecycle 95901eb9bf19e103e9acb82fb9726708f356b4cd
  bash scripts/stage5g_edc_r2_gate.sh
)
echo "stage5g-edc-r3-gate: PASS current-r3-new=23/23 current-head-negative=121/121 aggregate-negative=545/545"
