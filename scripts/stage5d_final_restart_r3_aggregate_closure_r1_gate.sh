#!/usr/bin/env bash
set -euo pipefail

echo "stage5d-final-restart-r3-aggregate-closure-r1-gate: start"
echo "rustc_version=$(rustc --version)"
echo "cargo_version=$(cargo --version)"
echo "source_commit=$(git rev-parse HEAD)"

mkdir -p reports/stage-5

positive_log="reports/stage-5/stage5d-final-restart-r3-aggregate-positive.log"
negative_log="reports/stage-5/stage5d-final-restart-r3-aggregate-negative-harness.log"
package_negative_log="reports/stage-5/stage5d-final-restart-r3-aggregate-package-negative-matrix.log"
self_test_log="reports/stage-5/stage5d-final-restart-r3-aggregate-checker-self-test.log"
golden_log="reports/stage-5/stage5d-final-restart-r3-aggregate-golden-fixture-drift.log"
stage5c_log="reports/stage-5/stage5d-final-restart-r3-aggregate-stage5c-freeze.log"
stage5d_log="reports/stage-5/stage5d-final-restart-r3-aggregate-stage5d-freeze.log"
forbidden_log="reports/stage-5/stage5d-final-restart-r3-aggregate-forbidden-surface.log"
no_redis_log="reports/stage-5/stage5d-final-restart-r3-aggregate-no-redis.log"

: > "$positive_log"
: > "$negative_log"
: > "$package_negative_log"
: > "$self_test_log"
: > "$golden_log"
: > "$stage5c_log"
: > "$stage5d_log"
: > "$forbidden_log"
: > "$no_redis_log"

python3 scripts/stage5d_final_restart_r3_aggregate_self_test.py | tee "$self_test_log"

run_positive_group() {
  local group="$1"
  local test_name="$2"
  echo "AGGREGATE_POSITIVE_GROUP_START ${group} ${test_name}" | tee -a "$positive_log"
  cargo test -p strategy-runtime-core "$test_name" -- --nocapture | tee -a "$positive_log"
  echo "AGGREGATE_POSITIVE_GROUP_OK ${group} ${test_name}" | tee -a "$positive_log"
}

run_positive_group "r3a_pending_entry" "stage5d_final_r3a_source_pending_entry_full_restart_matrix"
run_positive_group "positive_core" "stage5d_final_r3_positive_core_source_produced_full_restart_matrix"
run_positive_group "current_shadow" "stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix"
run_positive_group "operational_state" "stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix"
run_positive_group "recovery_index" "stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix"
run_positive_group "riskgate_recovery" "stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix"

python3 - <<'PY' | tee -a "$positive_log"
import json
from pathlib import Path

inventory = json.loads(Path("docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json").read_text())
rows = inventory["scenario_rows"]
accepted = [row for row in rows if str(row["execution_status"]).startswith("accepted_")]
todo = [row for row in rows if row["execution_status"] == "todo_source_produced"]
expected_groups = {
    "stage5d_final_r3a_source_pending_entry_full_restart_matrix",
    "stage5d_final_r3_positive_core_source_produced_full_restart_matrix",
    "stage5d_final_r3_current_shadow_r1_source_produced_full_restart_matrix",
    "stage5d_final_r3_operational_state_r1_source_produced_full_restart_matrix",
    "stage5d_final_r3_recovery_index_r1_source_produced_full_restart_matrix",
    "stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix",
}
owning_tests = {row["owning_test"] for row in rows}
print(f"mandatory_positive_count={len(rows)}")
print(f"accepted_executable_count={len(accepted)}")
print(f"todo_source_produced_count={len(todo)}")
print(f"positive_cases_executed={len(accepted)}")
print("positive_cases_failed=0")
if len(rows) != 21 or len(accepted) != 21 or len(todo) != 0:
    raise SystemExit("aggregate positive inventory is not 21/21")
if owning_tests != expected_groups:
    raise SystemExit("aggregate positive owning-test group mismatch")
PY

cargo test -p strategy-runtime-core stage5d_final_r2_package_negative_matrix_fails_closed -- --nocapture | tee "$package_negative_log"
cargo test -p strategy-runtime-core stage5d_final_r3_riskgate_recovery_r1r3_forged_receipts_fail_closed -- --nocapture | tee -a "$package_negative_log"

python3 scripts/stage5c_api_freeze_check.py | tee "$stage5c_log"
python3 scripts/stage5d_additive_freeze_check.py | tee "$stage5d_log"
bash scripts/forbidden_surface_scan.sh | tee "$forbidden_log"
bash scripts/test_m4_3x_evidence_no_redis.sh | tee "$no_redis_log"
python3 scripts/stage5d_additive_freeze_check.py | tee "$golden_log"
python3 scripts/stage5d_additive_freeze_negative_harness.py | tee "$negative_log"

negative_cases="$(awk -F= '/^cases_declared=/{print $2}' "$negative_log" | tail -1)"
if test -z "$negative_cases" || test "$negative_cases" -lt 303; then
  echo "aggregate negative_cases expected >=303, got ${negative_cases:-missing}" >&2
  exit 1
fi

python3 scripts/stage5d_final_restart_r3_aggregate_evidence.py
echo "negative_cases=$negative_cases"
echo "positive_log_sha256=$(sha256sum "$positive_log" | awk '{print $1}')"
echo "positive_log_line_count=$(wc -l < "$positive_log" | tr -d ' ')"
echo "negative_log_sha256=$(sha256sum "$negative_log" | awk '{print $1}')"
echo "negative_log_line_count=$(wc -l < "$negative_log" | tr -d ' ')"
echo "stage5d-final-restart-r3-aggregate-closure-r1-gate: ok"
