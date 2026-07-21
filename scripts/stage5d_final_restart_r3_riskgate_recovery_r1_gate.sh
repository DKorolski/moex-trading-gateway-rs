#!/usr/bin/env bash
set -euo pipefail

echo "stage5d-final-restart-r3-riskgate-recovery-r1-gate: start"
echo "rustc_version=$(rustc --version)"
echo "cargo_version=$(cargo --version)"
echo "source_commit=$(git rev-parse HEAD)"

mkdir -p reports/stage-5
focused_log="reports/stage-5/stage5d-final-restart-r3-riskgate-recovery-r1-focused-rust-evidence.log"
negative_log="reports/stage-5/stage5d-final-restart-r3-riskgate-recovery-r1-negative-harness.log"
: > "$focused_log"
: > "$negative_log"

cargo test -p strategy-runtime-core stage5d_final_r3_riskgate_recovery_r1_source_produced_matrix -- --nocapture | tee "$focused_log"

require_marker() {
  local marker="$1"
  if ! grep -Fqx "$marker" "$focused_log"; then
    echo "missing focused Rust evidence marker: $marker" >&2
    exit 1
  fi
}

require_marker "STAGE5D_RISKREC source_rows_exact=true"
require_marker "STAGE5D_RISKREC production_recovery_actions=true"
require_marker "STAGE5D_RISKREC durable_store_matrix=true"
require_marker "STAGE5D_RISKREC checkpoint_restart_matrix=true"
require_marker "STAGE5D_RISKREC final_checkpoint_committed=true"
require_marker "STAGE5D_RISKREC single_pending_finalization=true"
require_marker "STAGE5D_RISKREC multi_row_ordered=true"
require_marker "STAGE5D_RISKREC complete_plan_noop=true"
require_marker "STAGE5D_RISKREC callback_exactly_once=true"
require_marker "STAGE5D_RISKREC idempotent_replay=true"
require_marker "STAGE5D_RISKREC golden_values_exact=true"
require_marker "STAGE5D_RISKREC stage5c_continuation=true"
require_marker "STAGE5D_RISKREC stage5e_closed=true"

python3 - <<'PY'
import json
from pathlib import Path

inventory = json.loads(Path("docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json").read_text())
rows = inventory["scenario_rows"]
accepted_statuses = {
    "accepted_r3a_r1_source_produced",
    "accepted_r3_positive_core_r1b_source_produced",
    "accepted_r3_current_shadow_r1_source_produced",
    "accepted_r3_operational_state_r1_source_produced",
    "accepted_r3_recovery_index_r1_source_produced",
    "accepted_r3_riskgate_recovery_r1_r1_source_produced",
}
accepted = [row for row in rows if row["execution_status"] in accepted_statuses]
todo = [row for row in rows if row["execution_status"] == "todo_source_produced"]
riskrec = [row for row in rows if row["execution_status"] == "accepted_r3_riskgate_recovery_r1_r1_source_produced"]
print(f"mandatory_positive_count={len(rows)}")
print(f"accepted_executable_count={len(accepted)}")
print(f"todo_source_produced_count={len(todo)}")
print(f"riskgate_recovery_cases_executed={len(riskrec)}")
if len(rows) != 21 or len(accepted) != 21 or len(todo) != 0 or len(riskrec) != 3:
    raise SystemExit("Stage 5D r3 riskgate recovery inventory is not 21/0")
PY
echo "accepted_executable_count=21"
echo "todo_source_produced_count=0"

for golden in \
  tests/fixtures/stage5/stage5d_riskrec_single_pending_golden.json \
  tests/fixtures/stage5/stage5d_riskrec_ordered_multi_row_golden.json \
  tests/fixtures/stage5/stage5d_riskrec_complete_noop_golden.json
do
  echo "golden_sha256 $(sha256sum "$golden" | awk '{print $1}') $golden"
done

python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py | tee "$negative_log"
negative_cases="$(awk -F= '/^cases_declared=/{print $2}' "$negative_log" | tail -1)"
if test -z "$negative_cases" || test "$negative_cases" -le 214; then
  echo "negative_cases did not increase beyond 214: ${negative_cases:-missing}" >&2
  exit 1
fi
echo "negative_cases=$negative_cases"
echo "focused_log_sha256=$(sha256sum "$focused_log" | awk '{print $1}')"
echo "focused_log_line_count=$(wc -l < "$focused_log" | tr -d ' ')"
echo "stage5d-final-restart-r3-riskgate-recovery-r1-gate: ok"
