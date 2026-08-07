#!/usr/bin/env bash
set -euo pipefail

python3 scripts/stage5g_f_check.py
python3 scripts/stage5g_f_negative_harness.py
python3 scripts/stage5g_f_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_f_
cargo test --release -p strategy-runtime-core --lib stage5g_f_
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r3-predecessor.XXXXXX")"
cleanup() { rm -rf "$snapshot"; }
trap cleanup EXIT
git clone --quiet --no-hardlinks . "$snapshot/predecessor"
(
  cd "$snapshot/predecessor"
  git checkout --quiet -B stage5g-lifecycle c38d2e44e083e39552ea716823e43ebae775b881
  python3 scripts/stage5g_edc_r3_check.py
  python3 scripts/stage5g_edc_r3_negative_harness.py
  python3 scripts/stage5g_edc_r3_preseal_check.py
  cargo fmt --all -- --check
  cargo test -p strategy-runtime-core --lib stage5g_edc_r3_
  cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_
)
echo "stage5g-f-gate: PASS current-negative>=80/80 predecessor-r3-bounded=PASS"
