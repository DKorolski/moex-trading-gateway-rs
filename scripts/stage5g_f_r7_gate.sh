#!/usr/bin/env bash
set -euo pipefail

# The detached R6 gate retains the accepted Stage 5G-f R1 lineage at
# a28cedd984d41bd2db4aeb7fd8c125c62ded4b28.

artifact_dir="${TMPDIR:-/tmp}/stage5g-f-r7-artifact.$$"
mkdir -p "$artifact_dir"
r6_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r7-submitted-79c5443.XXXXXX")"
predecessor_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r7-predecessor.XXXXXX")"
cleanup() {
  rm -rf "$artifact_dir" "$r6_snapshot" "$predecessor_snapshot"
}
trap cleanup EXIT

python3 scripts/stage5g_f_check.py
python3 scripts/stage5g_f_negative_harness.py
python3 scripts/stage5g_f_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_f_ --no-default-features
cargo test --release -p strategy-runtime-core --lib stage5g_f_ --no-default-features
cargo run -q -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_f_gprt_artifact > "$artifact_dir/stage5g-f-gprt-artifact.debug.json"
cargo run -q --release -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_f_gprt_artifact > "$artifact_dir/stage5g-f-gprt-artifact.release.json"
cmp "$artifact_dir/stage5g-f-gprt-artifact.debug.json" "$artifact_dir/stage5g-f-gprt-artifact.release.json"
shasum -a 256 "$artifact_dir/stage5g-f-gprt-artifact.debug.json"
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

git clone --quiet --no-hardlinks . "$r6_snapshot/submitted-r6"
(
  cd "$r6_snapshot/submitted-r6"
  git checkout --quiet -B stage5g-lifecycle 79c544352a0a5f8c0fc61da33c314a17df5d0e3b
  bash scripts/stage5g_f_r6_gate.sh
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

echo "stage5g-f-r7-gate: PASS current-negative>=460/460 submitted-79c5443=PASS predecessor-r3-bounded=PASS production-authority-gprt-v3-debug-release-parallel=PASS"
