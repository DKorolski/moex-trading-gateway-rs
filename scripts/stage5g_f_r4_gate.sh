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

# Accepted Stage 5G-f R1 lineage anchor:
# a28cedd984d41bd2db4aeb7fd8c125c62ded4b28
r3_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r3-submitted-7dde2ac.XXXXXX")"
r2_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r2-submitted-34ecc95.XXXXXX")"
predecessor_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r4-predecessor.XXXXXX")"
cleanup() {
  rm -rf "$r3_snapshot" "$r2_snapshot" "$predecessor_snapshot"
}
trap cleanup EXIT

git clone --quiet --no-hardlinks . "$r3_snapshot/submitted-r3"
(
  cd "$r3_snapshot/submitted-r3"
  git checkout --quiet -B stage5g-lifecycle 7dde2ac181c7a5d3a3312bfb463e384281062a8a
  bash scripts/stage5g_f_r3_gate.sh
)

git clone --quiet --no-hardlinks . "$r2_snapshot/submitted-r2"
(
  cd "$r2_snapshot/submitted-r2"
  git checkout --quiet -B stage5g-lifecycle 34ecc9595bdb83639415ddde1b3975b88ac2faa4
  bash scripts/stage5g_f_r2_gate.sh
)

git clone --quiet --no-hardlinks . "$predecessor_snapshot/predecessor-edc-r3"
(
  cd "$predecessor_snapshot/predecessor-edc-r3"
  git checkout --quiet -B stage5g-lifecycle c38d2e44e083e39552ea716823e43ebae775b881
  python3 scripts/stage5g_edc_r3_check.py
  python3 scripts/stage5g_edc_r3_negative_harness.py
  python3 scripts/stage5g_edc_r3_preseal_check.py
  cargo fmt --all -- --check
  cargo test -p strategy-runtime-core --lib stage5g_edc_r3_
  cargo test --release -p strategy-runtime-core --lib stage5g_edc_r3_
)

echo "stage5g-f-r4-gate: PASS current-negative>=330/330 submitted-7dde2ac=PASS submitted-34ecc95=PASS accepted-a28cedd-lineage=PASS predecessor-r3-bounded=PASS"
