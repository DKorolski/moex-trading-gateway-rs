#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: stage8b_p1e_i0_regression_gate.sh ACCEPTED_R10_COMMIT NEW_ABSOLUTE_RETAINED_OUTPUT" >&2
  exit 64
fi

accepted_r10="$1"
retained_output="$2"
repo_root="$(git rev-parse --show-toplevel)"

if [[ "$retained_output" != /* ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL retained output must be absolute" >&2
  exit 64
fi
if [[ "$retained_output" == "$repo_root" || "$retained_output" == "$repo_root/"* ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL retained output must be outside repository" >&2
  exit 64
fi
if [[ -e "$retained_output" ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL retained output must be fresh" >&2
  exit 64
fi
retained_parent="$(dirname "$retained_output")"
if [[ ! -d "$retained_parent" ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL retained output parent is absent" >&2
  exit 64
fi

source_ref="$(git rev-parse HEAD)"
source_tree="$(git rev-parse 'HEAD^{tree}')"
clean_before=false
if [[ -z "$(git status --porcelain --untracked-files=all)" ]]; then
  clean_before=true
fi

temporary="$(mktemp -d "$retained_parent/.stage8b-p1e-i0-evidence.XXXXXX")"
gate_log="$temporary/gate.log"
finalized=false

finalize_failure() {
  local exit_code="$1"
  local clean_flag=()
  if [[ "$clean_before" == true ]]; then
    clean_flag=(--clean-before)
  fi
  if [[ "$finalized" == false && -d "$temporary" && ! -e "$retained_output" ]]; then
    python3 scripts/stage8b_p1e_i0_retained_evidence.py finalize \
      --temporary "$temporary" \
      --output "$retained_output" \
      --status FAIL \
      --exit-code "$exit_code" \
      --accepted-r10 "$accepted_r10" \
      --source-ref "$source_ref" \
      --source-tree "$source_tree" \
      "${clean_flag[@]}"
  fi
}

on_exit() {
  local exit_code="$?"
  if [[ "$exit_code" -ne 0 ]]; then
    finalize_failure "$exit_code" || true
  fi
}
trap on_exit EXIT

run_logged() {
  "$@" 2>&1 | tee -a "$gate_log"
}

run_exact_test() {
  local index="$1"
  local test_name="$2"
  local list_file="$temporary/exact-test-${index}-list.txt"
  local run_file="$temporary/exact-test-${index}-run.txt"
  RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features \
    "$test_name" -- --list --exact 2>&1 | tee -a "$gate_log" | tee "$list_file"
  local selected
  selected="$(grep -Ec ': test$' "$list_file" || true)"
  if [[ "$selected" -ne 1 ]] || ! grep -Fqx "$test_name: test" "$list_file"; then
    echo "stage8b-p1e-i0-regression-gate: FAIL exact test selection index=$index selected=$selected name=$test_name" | tee -a "$gate_log"
    return 1
  fi
  RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features \
    "$test_name" -- --exact 2>&1 | tee -a "$gate_log" | tee "$run_file"
  if ! grep -Eq 'test result: ok\. 1 passed; 0 failed;' "$run_file"; then
    echo "stage8b-p1e-i0-regression-gate: FAIL exact test execution index=$index name=$test_name" | tee -a "$gate_log"
    return 1
  fi
  echo "PASS exact-regression-test index=$index selected=1 passed=1 name=$test_name" | tee -a "$gate_log"
}

run_logged python3 scripts/stage8b_p1e_i0_scope_check.py "$accepted_r10"
run_logged env PYTHONPATH=scripts python3 scripts/stage8b_p1e_i0_p1d4_regression_check.py
run_logged env PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
run_logged python3 -m py_compile \
  scripts/stage8b_p1e_i0_scope_check.py \
  scripts/stage8b_p1e_i0_retained_evidence.py \
  scripts/stage8b_p1e_i0_p1d4_regression_check.py \
  scripts/stage8b_p1d4_crash_evidence_check.py \
  scripts/stage8b_p1d4_crash_evidence_negative_harness.py \
  scripts/stage8b_p1d4_source_check.py \
  scripts/stage8b_p1d4_source_negative_harness.py

run_logged cargo fmt --all -- --check
run_logged cargo test -p strategy-runtime-core --lib --all-features
run_logged env RUST_MIN_STACK=33554432 cargo test -p runtime-durable-service --lib --all-features

mkdir "$temporary/crash-evidence"
run_logged env \
  STAGE8B_P1D4_EVIDENCE_OUTPUT="$temporary/crash-evidence" \
  STAGE8B_P1D4_EVIDENCE_SOURCE_REF="$source_ref" \
  STAGE8B_P1D4_EVIDENCE_SOURCE_TREE="$source_tree" \
  RUST_MIN_STACK=33554432 \
  cargo test -p runtime-durable-service --lib --all-features \
    stage8b_p1_semantic::redis::tests::p1d4_exhaustive_crash_replay_evidence_two_clean_runs \
    -- --ignored --exact
run_logged python3 scripts/stage8b_p1d4_crash_evidence_check.py "$temporary/crash-evidence"
run_logged python3 scripts/stage8b_p1d4_crash_evidence_negative_harness.py "$temporary/crash-evidence"

exact_tests=(
  stage8b_p1_semantic::redis::tests::p1d2_market_feedback_commits_ack_then_truth_then_xacks_source
  stage8b_p1_semantic::redis::tests::p1d2_subprocess_kill_matrix_recovers_all_six_durable_frontiers
  stage8b_p1_semantic::redis::tests::p1d3_read_only_successor_observation_retains_original_source_until_xack
  stage8b_p1_semantic::redis::tests::p1d3_actual_host_optional_tcid_completes_recovered_cancel_and_xacks_last
  stage8b_p1_semantic::redis::tests::p1d3_optional_tcid_target_first_restart_continues_without_duplicate_effects
  stage8b_p1_semantic::redis::tests::p1d3_subprocess_sigkill_brackets_s_cancel_recovered
)
for index in "${!exact_tests[@]}"; do
  run_exact_test "$((index + 1))" "${exact_tests[$index]}"
done

run_logged cargo test -p strategy-runtime-core --doc --all-features
run_logged cargo test -p runtime-durable-service --doc --all-features
run_logged cargo clippy -p strategy-runtime-core -p runtime-durable-service \
  --all-targets --all-features -- -D warnings

if [[ "$(git rev-parse HEAD)" != "$source_ref" || "$(git rev-parse 'HEAD^{tree}')" != "$source_tree" ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL source ref/tree changed during run" | tee -a "$gate_log"
  exit 1
fi
if [[ -n "$(git status --porcelain --untracked-files=all)" ]]; then
  echo "stage8b-p1e-i0-regression-gate: FAIL source became dirty during run" | tee -a "$gate_log"
  exit 1
fi

echo "PASS stage8b-p1e-i0-regression-gate scope=current-r10-delta historical_scope_gate=false p1d4_content=true p1d4_sigkill=105x2 p1d2_p1d3_exact=6 retained=true clean_immutable_source=true doctests=true clippy=true" | tee -a "$gate_log"
python3 scripts/stage8b_p1e_i0_retained_evidence.py finalize \
  --temporary "$temporary" \
  --output "$retained_output" \
  --status PASS \
  --exit-code 0 \
  --accepted-r10 "$accepted_r10" \
  --source-ref "$source_ref" \
  --source-tree "$source_tree" \
  --clean-before
finalized=true
python3 scripts/stage8b_p1e_i0_retained_evidence.py check "$retained_output" --require-pass
