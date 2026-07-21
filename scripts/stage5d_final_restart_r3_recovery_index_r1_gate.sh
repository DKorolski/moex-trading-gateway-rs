#!/usr/bin/env bash
set -euo pipefail

echo "stage5d-final-restart-r3-recovery-index-r1-r3-gate: start"
echo "rustc_version=$(rustc --version)"
echo "cargo_version=$(cargo --version)"
echo "source_commit=$(git rev-parse HEAD)"
if test -z "$(git status --short)"; then
  echo "clean_worktree_before=true"
else
  echo "clean_worktree_before=false"
fi

cargo fmt --all --check
mkdir -p reports/stage-5
focused_log="reports/stage-5/stage5d-final-restart-r3-recovery-index-r1-r3-focused-rust-evidence.log"
negative_log="reports/stage-5/stage5d-final-restart-r3-recovery-index-r1-r3-negative-harness.log"
: > "$focused_log"
: > "$negative_log"

cargo test -p strategy-runtime-core stage5d_final_r3_recovery_index_r1 -- --nocapture | tee "$focused_log"

require_marker() {
  local marker="$1"
  if ! grep -Fqx "$marker" "$focused_log"; then
    echo "missing focused Rust evidence marker: $marker" >&2
    exit 1
  fi
}

require_marker "STAGE5D_RECOVERY_R1R3 unbroken_type_state_path=true"
require_marker "STAGE5D_RECOVERY_R1R3 production_working_set_transition=true"
require_marker "STAGE5D_RECOVERY_R1R3 validated_stop_truth_roundtrip=true"
require_marker "STAGE5D_RECOVERY_R1R3 tp_duplicate_suppressed=true"
require_marker "STAGE5D_RECOVERY_R1R3 sl_duplicate_suppressed=true"
require_marker "STAGE5D_RECOVERY_R1R3 tp_terminal_no_entry_or_flip=true"
require_marker "STAGE5D_RECOVERY_R1R3 sl_terminal_no_entry_or_flip=true"
require_marker "STAGE5D_RECOVERY_R1R3 normalization_block_retained=true"
require_marker "STAGE5D_RECOVERY_R1R3 pending_terminal_no_orphan=true"
require_marker "STAGE5D_RECOVERY_R1R3 stage5c_continuation=true"
require_marker "STAGE5D_RECOVERY_R1R3 recovery_index_cases_executed=3"

cargo test -p strategy-runtime-core stage5d_final_r3_operational_state_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_current_shadow_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_positive_core -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3a_source_pending_entry_full_restart_matrix -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_resumption -- --nocapture
python3 - <<'PY'
import json
from pathlib import Path

inventory = json.loads(
    Path("docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json").read_text()
)
rows = inventory["scenario_rows"]
accepted_statuses = {
    "accepted_r3a_r1_source_produced",
    "accepted_r3_positive_core_r1b_source_produced",
    "accepted_r3_current_shadow_r1_source_produced",
    "accepted_r3_operational_state_r1_source_produced",
    "accepted_r3_recovery_index_r1_source_produced",
}
accepted = [
    row for row in rows if row["execution_status"] in accepted_statuses
]
todo = [
    row for row in rows if row["execution_status"] == "todo_source_produced"
]
recovery = [
    row
    for row in rows
    if row["execution_status"] == "accepted_r3_recovery_index_r1_source_produced"
]
case_ids = {row["case_id"] for row in recovery}
print(f"mandatory_positive_count={len(rows)}")
print(f"accepted_executable_count={len(accepted)}")
print(f"todo_source_produced_count={len(todo)}")
print(f"recovery_index_cases_executed={len(recovery)}")
print(f"known_order_index_non_empty={'positive_non_empty_known_order_index' in case_ids}")
print(f"pending_request_index_non_empty={'positive_non_empty_pending_request_index' in case_ids}")
print(f"working_protective_order_and_stop_hints_non_empty={'positive_working_protective_order_hints' in case_ids}")
print(f"stage5e_closed={inventory['closed_surfaces']['runtime_live'] is False and inventory['closed_surfaces']['broker_execution'] is False}")
PY
echo "unbroken_type_state_path=true"
echo "production_working_set_transition_executed=true"
echo "validated_stop_truth_roundtrip=true"
echo "normalization_block_capability_preserved=true"
echo "tp_duplicate_suppressed=true"
echo "sl_duplicate_suppressed=true"
echo "tp_terminal_no_entry_or_flip=true"
echo "sl_terminal_no_entry_or_flip=true"
echo "pending_terminal_no_orphan=true"
echo "stage5c_continuation=true"
echo "focused_log_sha256=$(sha256sum "$focused_log" | awk '{print $1}')"
echo "focused_log_line_count=$(wc -l < "$focused_log" | tr -d ' ')"
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py | tee "$negative_log"
negative_cases="$(awk -F= '/^cases_declared=/{print $2}' "$negative_log" | tail -1)"
if test -z "$negative_cases" || test "$negative_cases" -lt 214; then
  echo "negative_cases below r1-r3 floor: ${negative_cases:-missing}" >&2
  exit 1
fi
echo "negative_cases=$negative_cases"
python3 scripts/handoff_safety_check.py --source-tree "$(pwd)"
bash scripts/make_handoff_archive.sh
cargo test --workspace --all-targets
cargo test --workspace --doc
cargo clippy --workspace --all-targets -- -D warnings

if test -z "$(git status --short)"; then
  echo "clean_worktree_after=true"
else
  echo "clean_worktree_after=false"
fi
echo "stage5d-final-restart-r3-recovery-index-r1-r3-gate: ok"
