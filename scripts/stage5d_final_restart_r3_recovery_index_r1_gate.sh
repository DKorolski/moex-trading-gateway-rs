#!/usr/bin/env bash
set -euo pipefail

echo "stage5d-final-restart-r3-recovery-index-r1-r1-gate: start"
echo "rustc_version=$(rustc --version)"
echo "cargo_version=$(cargo --version)"
echo "source_commit=$(git rev-parse HEAD)"
if test -z "$(git status --short)"; then
  echo "clean_worktree_before=true"
else
  echo "clean_worktree_before=false"
fi

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
print(f"working_protective_hints_non_empty={'positive_working_protective_order_hints' in case_ids}")
print(f"post_restored_duplicate_suppression={all(row.get('duplicate_suppression_after_restore') is True for row in recovery)}")
print(f"stage5c_continuation={all(row.get('stage5c_continuation_executed') is True for row in recovery)}")
print(f"stage5e_closed={inventory['closed_surfaces']['runtime_live'] is False and inventory['closed_surfaces']['broker_execution'] is False}")
PY

cargo fmt --all --check
cargo test -p strategy-runtime-core stage5d_final_r3_recovery_index_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_operational_state_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_current_shadow_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_positive_core -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3a_source_pending_entry_full_restart_matrix -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_resumption -- --nocapture
python3 scripts/stage5c_api_freeze_check.py
python3 scripts/stage5d_additive_freeze_check.py
bash scripts/forbidden_surface_scan.sh
python3 scripts/stage5d_additive_freeze_negative_harness.py
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
echo "stage5d-final-restart-r3-recovery-index-r1-r1-gate: ok"
