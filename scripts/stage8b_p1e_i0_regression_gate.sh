#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: stage8b_p1e_i0_regression_gate.sh ACCEPTED_R9_COMMIT" >&2
  exit 64
fi

accepted_r9="$1"
python3 scripts/stage8b_p1e_i0_scope_check.py "$accepted_r9"
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i0_p1d4_regression_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
python3 -m py_compile \
  scripts/stage8b_p1e_i0_scope_check.py \
  scripts/stage8b_p1e_i0_p1d4_regression_check.py \
  scripts/stage8b_p1d4_crash_evidence_check.py \
  scripts/stage8b_p1d4_crash_evidence_negative_harness.py \
  scripts/stage8b_p1d4_source_check.py \
  scripts/stage8b_p1d4_source_negative_harness.py

cargo fmt --all -- --check
cargo test -p strategy-runtime-core --lib --all-features
RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features

evidence_output="$(mktemp -d "${TMPDIR:-/tmp}/stage8b-p1e-i0-evidence.XXXXXX")"
trap 'rm -rf "$evidence_output"' EXIT
STAGE8B_P1D4_EVIDENCE_OUTPUT="$evidence_output" \
STAGE8B_P1D4_EVIDENCE_SOURCE_REF="$(git rev-parse HEAD)" \
STAGE8B_P1D4_EVIDENCE_SOURCE_TREE="$(git rev-parse 'HEAD^{tree}')" \
RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --lib --all-features \
    stage8b_p1_semantic::redis::tests::p1d4_exhaustive_crash_replay_evidence_two_clean_runs \
    -- --ignored --exact
python3 scripts/stage8b_p1d4_crash_evidence_check.py "$evidence_output"
python3 scripts/stage8b_p1d4_crash_evidence_negative_harness.py "$evidence_output"

filters=(
  p1d2_market_feedback_commits_ack_then_truth_then_xacks_source
  p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers
  p1d3_read_only_successor_observation_retains_original_source_until_xack
  p1d3_actual_host_optional_tcid_completes_recovered_cancel_and_xacks_last
  p1d3_optional_tcid_target_first_restart_continues_without_duplicate_effects
  p1d3_subprocess_sigkill_brackets_s_cancel_recovered
)
for filter in "${filters[@]}"; do
  RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features "$filter" -- --exact
done

cargo test -p strategy-runtime-core --doc --all-features
cargo test -p runtime-durable-service --doc --all-features
cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

echo "PASS stage8b-p1e-i0-regression-gate scope=current-r9-delta historical_scope_gate=false p1d4_content=true p1d4_sigkill=105x2 p1d2_p1d3_filters=6 doctests=true clippy=true"
