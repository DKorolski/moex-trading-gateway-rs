#!/usr/bin/env bash
set -euo pipefail

artifact_dir="${TMPDIR:-/tmp}/stage5g-f-r6-artifact.$$"
mkdir -p "$artifact_dir"
cleanup() {
  rm -rf "$artifact_dir" "$r5_snapshot" "$predecessor_snapshot"
}
r5_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r6-submitted-1f8d7f3.XXXXXX")"
predecessor_snapshot="$(mktemp -d "${TMPDIR:-/tmp}/stage5g-f-r6-predecessor.XXXXXX")"
trap cleanup EXIT

# Accepted R1/R2/R3/R4 lineage is verified transitively by the submitted R5 gate below.
# a28cedd984d41bd2db4aeb7fd8c125c62ded4b28
# 7dde2ac181c7a5d3a3312bfb463e384281062a8a
# 34ecc9595bdb83639415ddde1b3975b88ac2faa4
# 430bae6cd02f67844623f9d1b2112b1faedcc40a

python3 scripts/stage5g_f_check.py
python3 scripts/stage5g_f_negative_harness.py
python3 scripts/stage5g_f_preseal_check.py
cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_f_
cargo test --release -p strategy-runtime-core --lib stage5g_f_
cargo run -q -p strategy-runtime-core --bin stage5g_f_gprt_artifact > "$artifact_dir/stage5g-f-gprt-artifact.debug.json"
cargo run -q --release -p strategy-runtime-core --bin stage5g_f_gprt_artifact > "$artifact_dir/stage5g-f-gprt-artifact.release.json"
cmp "$artifact_dir/stage5g-f-gprt-artifact.debug.json" "$artifact_dir/stage5g-f-gprt-artifact.release.json"
shasum -a 256 "$artifact_dir/stage5g-f-gprt-artifact.debug.json"
cargo test -p strategy-runtime-core --lib
cargo test -p strategy-runtime-core --doc
cargo test --workspace --doc
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

git clone --quiet --no-hardlinks . "$r5_snapshot/submitted-r5"
(
  cd "$r5_snapshot/submitted-r5"
  git checkout --quiet -B stage5g-lifecycle 1f8d7f3d14aa9cd2cb0f522679cf66787d5dd8a8
  bash scripts/stage5g_f_r5_gate.sh
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

echo "stage5g-f-r6-gate: PASS current-negative>=430/430 submitted-1f8d7f3=PASS predecessor-r3-bounded=PASS gprt-artifact-debug-release-parallel=PASS"
