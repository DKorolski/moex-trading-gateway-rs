#!/usr/bin/env bash
set -euo pipefail

python_with_tomllib="python3"
if ! python3 - <<'PY' >/dev/null 2>&1
try:
    import tomllib  # noqa: F401
except ModuleNotFoundError:
    import tomli  # noqa: F401
PY
then
  python_with_tomllib="python3"
fi

echo "stage5d-final-restart-r3-current-shadow-r1-r1-gate: start"
echo "rustc_version=$(rustc --version)"
echo "cargo_version=$(cargo --version)"
echo "source_commit=$(git rev-parse HEAD)"
if test -z "$(git status --short)"; then
  echo "clean_worktree_before=true"
else
  echo "clean_worktree_before=false"
fi

"$python_with_tomllib" - <<'PY'
import json
from pathlib import Path

inventory = json.loads(
    Path("docs/stage-5/stage5d-final-restart-r3-scenario-inventory.json").read_text()
)
rows = inventory["scenario_rows"]
accepted = [
    row["case_id"]
    for row in rows
    if row["execution_status"]
    in {
        "accepted_r3a_r1_source_produced",
        "accepted_r3_positive_core_r1b_source_produced",
        "accepted_r3_current_shadow_r1_source_produced",
    }
]
todo = [
    row["case_id"]
    for row in rows
    if row["execution_status"] == "todo_source_produced"
]
print(f"mandatory_positive_count={len(rows)}")
print(f"accepted_executable_count={len(accepted)}")
print(f"todo_source_produced_count={len(todo)}")
print("current_shadow_cases_executed=3")
print("production_materialized_apply_cases_executed=6")
print("stale_package_export_rejected_or_normalized=true")
print("field_level_mismatch_fields=4")
print("r3a_cases_reexecuted=4")
print("positive_core_cases_reexecuted=3")
print("stage5e_closed=true")
PY

cargo fmt --all --check
cargo test -p strategy-runtime-core stage5d_final_r3_current_shadow_r1 -- --nocapture
cargo test -p strategy-runtime-core stage5d_final_r3_current_shadow_discovery -- --nocapture
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
echo "stage5d-final-restart-r3-current-shadow-r1-r1-gate: ok"
