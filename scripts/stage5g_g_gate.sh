#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

artifact_dir="${STAGE5G_G_ARTIFACT_DIR:-${TMPDIR:-/tmp}/stage5g-g-artifact.$$}"
mkdir -p "$artifact_dir"
debug_artifact="$artifact_dir/stage5g-g-lifecycle-artifact.debug.json"
release_artifact="$artifact_dir/stage5g-g-lifecycle-artifact.release.json"

cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib stage5g_g_
cargo test -p strategy-runtime-core --lib stage5gd_
cargo test -p strategy-runtime-core --lib stage5ck_partial_entry_cleanup_uses_pending_entry_attribution
cargo test -p strategy-runtime-core --lib stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once
cargo test -p strategy-runtime-core --lib stage5cm_generated_timer_batch_blocks_continuation_until_lifecycle
cargo test -p strategy-runtime-core --lib stage5g_edc_grst
cargo test --release -p strategy-runtime-core --lib stage5gd_
cargo test --release -p strategy-runtime-core --lib stage5ck_partial_entry_cleanup_uses_pending_entry_attribution
cargo test --release -p strategy-runtime-core --lib stage5cm_ready_checkpoint_can_continue_to_timer_or_bar_once
cargo test --release -p strategy-runtime-core --lib stage5cm_generated_timer_batch_blocks_continuation_until_lifecycle
cargo test --release -p strategy-runtime-core --lib stage5g_edc_grst
cargo run -q -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_g_lifecycle_artifact > "$debug_artifact"
cargo run -q --release -p strategy-runtime-core --features stage5g-artifact-fixtures --bin stage5g_g_lifecycle_artifact > "$release_artifact"
cmp "$debug_artifact" "$release_artifact"
python3 scripts/stage5g_g_matrix_check.py --artifact "$debug_artifact"
python3 scripts/stage5g_g_negative_harness.py --artifact "$debug_artifact"
cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings
shasum -a 256 "$debug_artifact"
echo "stage5g-g-gate: PASS artifact=$debug_artifact"
