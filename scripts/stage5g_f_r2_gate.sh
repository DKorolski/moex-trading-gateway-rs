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

submitted_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r1-submitted-a28cedd.XXXXXX")"
predecessor_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r3-predecessor.XXXXXX")"
cleanup() {
  rm -rf "$submitted_snapshot" "$predecessor_snapshot"
}
trap cleanup EXIT

git clone --quiet --no-hardlinks . "$submitted_snapshot/submitted"
(
  cd "$submitted_snapshot/submitted"
  git checkout --quiet -B stage5g-lifecycle a28cedd984d41bd2db4aeb7fd8c125c62ded4b28
  python3 scripts/stage5g_f_check.py
  python3 scripts/stage5g_f_negative_harness.py
  python3 scripts/stage5g_f_preseal_check.py
  cargo test -p strategy-runtime-core --lib stage5g_f_
  cargo test --release -p strategy-runtime-core --lib stage5g_f_
)

git clone --quiet --no-hardlinks . "$predecessor_snapshot/predecessor"
(
  cd "$predecessor_snapshot/predecessor"
  git checkout --quiet -B stage5g-lifecycle c38d2e44e083e39552ea716823e43ebae775b881
  python3 scripts/stage5g_edc_r3_check.py
  python3 scripts/stage5g_edc_r3_negative_harness.py
  python3 scripts/stage5g_edc_r3_preseal_check.py
  cargo fmt --all -- --check
  cargo test -p strategy-runtime-core --lib stage5g_edc_r3_
  cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_
)

echo "stage5g-f-r2-gate: PASS current-negative>=180/180 submitted-a28cedd=PASS predecessor-r3-bounded=PASS"
