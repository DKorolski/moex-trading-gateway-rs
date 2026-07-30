#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive_dir="$repo_root/reports/handoff"
mkdir -p "$archive_dir"

if [[ -n "$(git -C "$repo_root" status --porcelain --untracked-files=all)" ]]; then
  echo "Refusing to build review handoff: source tree is dirty." >&2
  git -C "$repo_root" status --short >&2
  exit 1
fi

source_commit="$(git -C "$repo_root" rev-parse --short=7 HEAD)"
source_ref="$(git -C "$repo_root" rev-parse HEAD)"
archive_name="moex-trading-project-${source_commit}.zip"
archive_path="$archive_dir/$archive_name"
sha_path="$archive_path.sha256"
commit_marker="$repo_root/handoff-commit.txt"
handoff_manifest="$repo_root/handoff-manifest.json"
stage5e_gate_result="$repo_root/handoff-stage5e-gate-result.json"
stage5f_gate_result="$repo_root/handoff-stage5f-gate-result.json"
source_tree_manifest="$repo_root/handoff-source-tree-manifest.json"
stage5e_gate_stdout_log="$repo_root/handoff-stage5e-gate-stdout.txt"
stage5e_gate_stderr_log="$repo_root/handoff-stage5e-gate-stderr.txt"
stage5f_gate_stdout_log="$repo_root/handoff-stage5f-gate-stdout.txt"
stage5f_gate_stderr_log="$repo_root/handoff-stage5f-gate-stderr.txt"
cargo_gate_result="$repo_root/handoff-cargo-gate-result.json"
cargo_gate_stdout_log="$repo_root/handoff-cargo-gate-stdout.txt"
cargo_gate_stderr_log="$repo_root/handoff-cargo-gate-stderr.txt"
provenance_negative_result="$repo_root/handoff-provenance-negative-result.json"
provenance_negative_stdout_log="$repo_root/handoff-provenance-negative-stdout.txt"
provenance_negative_stderr_log="$repo_root/handoff-provenance-negative-stderr.txt"
stage5d_negative_result="$repo_root/handoff-stage5d-negative-result.json"
stage5d_negative_stdout_log="$repo_root/handoff-stage5d-negative-stdout.txt"
stage5d_negative_stderr_log="$repo_root/handoff-stage5d-negative-stderr.txt"
stage5f_negative_result="$repo_root/handoff-stage5f-negative-result.json"
stage5f_negative_stdout_log="$repo_root/handoff-stage5f-negative-stdout.txt"
stage5f_negative_stderr_log="$repo_root/handoff-stage5f-negative-stderr.txt"
stage5f_ci_negative_result="$repo_root/handoff-stage5f-ci-negative-result.json"
stage5f_ci_negative_stdout_log="$repo_root/handoff-stage5f-ci-negative-stdout.txt"
stage5f_ci_negative_stderr_log="$repo_root/handoff-stage5f-ci-negative-stderr.txt"
forbidden_negative_result="$repo_root/handoff-forbidden-negative-result.json"
forbidden_negative_stdout_log="$repo_root/handoff-forbidden-negative-stdout.txt"
forbidden_negative_stderr_log="$repo_root/handoff-forbidden-negative-stderr.txt"
completed=0

cleanup() {
  local status=$?
  rm -f "$commit_marker" "$handoff_manifest" "$stage5e_gate_result" "$stage5f_gate_result" \
    "$source_tree_manifest" "$stage5e_gate_stdout_log" "$stage5e_gate_stderr_log" \
    "$stage5f_gate_stdout_log" "$stage5f_gate_stderr_log" "$cargo_gate_result" \
    "$cargo_gate_stdout_log" "$cargo_gate_stderr_log" "$provenance_negative_result" \
    "$provenance_negative_stdout_log" "$provenance_negative_stderr_log" \
    "$stage5d_negative_result" "$stage5d_negative_stdout_log" \
    "$stage5d_negative_stderr_log" "$stage5f_negative_result" \
    "$stage5f_negative_stdout_log" "$stage5f_negative_stderr_log" \
    "$stage5f_ci_negative_result" "$stage5f_ci_negative_stdout_log" \
    "$stage5f_ci_negative_stderr_log" \
    "$forbidden_negative_result" \
    "$forbidden_negative_stdout_log" "$forbidden_negative_stderr_log"
  if [[ "$completed" -ne 1 ]]; then
    rm -f "$archive_path" "$sha_path"
  fi
  exit "$status"
}
trap cleanup EXIT

rm -f "$archive_path" "$sha_path"
python3 "$repo_root/scripts/handoff_safety_check.py" --source-tree "$repo_root"

cargo_gate_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
set +e
(
  set -euo pipefail
  cd "$repo_root"
  cargo fmt --check
  cargo test --workspace --all-targets
  cargo test --workspace --doc
  cargo clippy --workspace --all-targets -- -D warnings
) >"$cargo_gate_stdout_log" 2>"$cargo_gate_stderr_log"
cargo_gate_exit_code="$?"
set -e
cargo_gate_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if [[ "$cargo_gate_exit_code" -ne 0 ]]; then
  cat "$cargo_gate_stdout_log"
  cat "$cargo_gate_stderr_log" >&2
  echo "Cargo gate failed before handoff packaging." >&2
  exit "$cargo_gate_exit_code"
fi
SOURCE_REF="$source_ref" CARGO_VERSION="$(cargo --version)" \
CARGO_GATE_STARTED_AT_UTC="$cargo_gate_started_at_utc" \
CARGO_GATE_FINISHED_AT_UTC="$cargo_gate_finished_at_utc" \
CARGO_GATE_EXIT_CODE="$cargo_gate_exit_code" \
CARGO_GATE_STDOUT_SHA256="$(shasum -a 256 "$cargo_gate_stdout_log" | awk '{print $1}')" \
CARGO_GATE_STDERR_SHA256="$(shasum -a 256 "$cargo_gate_stderr_log" | awk '{print $1}')" \
python3 - "$cargo_gate_result" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "source_ref": os.environ["SOURCE_REF"],
    "cargo_version": os.environ["CARGO_VERSION"],
    "commands": [
        ["cargo", "fmt", "--check"],
        ["cargo", "test", "--workspace", "--all-targets"],
        ["cargo", "test", "--workspace", "--doc"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    ],
    "exit_code": int(os.environ["CARGO_GATE_EXIT_CODE"]),
    "started_at_utc": os.environ["CARGO_GATE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["CARGO_GATE_FINISHED_AT_UTC"],
    "stdout_member": "handoff-cargo-gate-stdout.txt",
    "stderr_member": "handoff-cargo-gate-stderr.txt",
    "stdout_sha256": os.environ["CARGO_GATE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["CARGO_GATE_STDERR_SHA256"],
}, indent=2, sort_keys=True) + "\n")
PY

printf '%s\n' \
  "source_commit=$source_commit" \
  "source_ref=$source_ref" \
  "archive_name=$archive_name" >"$commit_marker"

stage5c_checker_sha256="$(shasum -a 256 "$repo_root/scripts/stage5c_api_freeze_check.py" | awk '{print $1}')"
stage5d_checker_sha256="$(shasum -a 256 "$repo_root/scripts/stage5d_additive_freeze_check.py" | awk '{print $1}')"
stage5d_manifest_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/stage-5d-additive-freeze-manifest.json" | awk '{print $1}')"
review_stage="$(python3 - "$repo_root/docs/stage-5/stage-5d-additive-freeze-manifest.json" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["stage"])
PY
)"
stage5e_checker_sha256=""
stage5e_inventory_sha256=""
stage5e_plan_sha256=""
stage5e_active_descriptor_sha256=""
stage5e_descriptor_registry_sha256=""
stage5e_gate_result_sha256=""
stage5e_design_scope_sha256=""
stage5f_enabled=0
stage5f_checker_sha256=""
stage5f_inventory_sha256=""
stage5f_plan_sha256=""
stage5f_active_descriptor_sha256=""
stage5f_descriptor_registry_sha256=""
stage5f_gate_result_sha256=""
stage5f_negative_result_sha256=""
stage5f_ci_negative_result_sha256=""
stage5f_ci_workflow_sha256=""
stage5f_b3f_snapshot_provenance_wrapper_sha256=""
stage5f_atomic_hybrid_semantics_gate_sha256=""
stage5f_ci_snapshot_inheritance_check_sha256=""
stage5f_atomic_hybrid_semantics_negative_harness_sha256=""
stage5f_ci_snapshot_inheritance_negative_harness_sha256=""
stage5f_design_scope_sha256=""
source_tree_manifest_sha256=""
current_review_stage="$review_stage"
design_baseline_ref=""
design_changed_paths_json="[]"
design_head_tree=""

# Stage 5E-B3F is an immutable closure descriptor. Once Stage 5F is active the
# builder must not run B3F against the newer tree: its checker intentionally
# seals the exact accepted B3F diff. The Stage 5F gate executes that closure
# from the accepted snapshot, then records its own separate descriptor/gate.
if [[ -f "$repo_root/docs/stage-5/stage5f-active-descriptor.json" ]]; then
  stage5f_enabled=1
  stage5f_descriptor_json="$(python3 "$repo_root/scripts/stage5f_descriptor.py" --root "$repo_root")"
  stage5f_inventory_rel="$(STAGE5F_DESCRIPTOR_JSON="$stage5f_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5F_DESCRIPTOR_JSON"])["inventory"])')"
  stage5f_plan_rel="$(STAGE5F_DESCRIPTOR_JSON="$stage5f_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5F_DESCRIPTOR_JSON"])["plan"])')"
  stage5f_checker_rel="$(STAGE5F_DESCRIPTOR_JSON="$stage5f_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5F_DESCRIPTOR_JSON"])["checker"])')"
  stage5f_checker_sha256="$(shasum -a 256 "$repo_root/$stage5f_checker_rel" | awk '{print $1}')"
  stage5f_inventory_sha256="$(shasum -a 256 "$repo_root/$stage5f_inventory_rel" | awk '{print $1}')"
  stage5f_plan_sha256="$(shasum -a 256 "$repo_root/$stage5f_plan_rel" | awk '{print $1}')"
  stage5f_active_descriptor_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/stage5f-active-descriptor.json" | awk '{print $1}')"
  stage5f_descriptor_registry_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_descriptor.py" | awk '{print $1}')"
  stage5f_baseline_ref="$(python3 - "$repo_root/$stage5f_inventory_rel" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["baseline_ref"])
PY
)"
  stage5f_head_tree="$(git -C "$repo_root" rev-parse HEAD^{tree})"
  stage5f_changed_paths_json="$(git -C "$repo_root" diff --name-only "$stage5f_baseline_ref" -- | python3 -c 'import json,sys; print(json.dumps([line.strip() for line in sys.stdin if line.strip()], separators=(",", ":"), sort_keys=True))')"
  stage5f_changed_paths_sha256="$(printf '%s' "$stage5f_changed_paths_json" | shasum -a 256 | awk '{print $1}')"
  stage5f_design_scope_sha256="$(BASELINE_REF="$stage5f_baseline_ref" HEAD_TREE="$stage5f_head_tree" CHANGED_PATHS_JSON="$stage5f_changed_paths_json" CHANGED_PATHS_SHA256="$stage5f_changed_paths_sha256" SOURCE_REF="$source_ref" python3 - <<'PY'
import hashlib
import json
import os

payload = {
    "baseline_ref": os.environ["BASELINE_REF"],
    "changed_paths": json.loads(os.environ["CHANGED_PATHS_JSON"]),
    "changed_paths_sha256": os.environ["CHANGED_PATHS_SHA256"],
    "head_tree": os.environ["HEAD_TREE"],
    "source_ref": os.environ["SOURCE_REF"],
}
print(hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest())
PY
)"
  design_baseline_ref="$stage5f_baseline_ref"
  design_changed_paths_json="$stage5f_changed_paths_json"
  design_head_tree="$stage5f_head_tree"
  current_review_stage="$(python3 - "$repo_root/$stage5f_inventory_rel" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["stage"])
PY
)"

  stage5f_b3f_active_descriptor_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/stage5e-active-descriptor.json" | awk '{print $1}')"
  stage5f_b3f_checker_sha256="$(shasum -a 256 "$repo_root/scripts/stage5e_b3f_callback_settlement_escrow_design_check.py" | awk '{print $1}')"
  stage5f_b3f_inventory_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/stage5e-b3f-callback-settlement-escrow-design-inventory.json" | awk '{print $1}')"
  stage5f_b3f_plan_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/5e-b3f-callback-settlement-escrow-design.md" | awk '{print $1}')"
  stage5f_b3f_ui_harness_sha256="$(shasum -a 256 "$repo_root/scripts/stage5e_b3f_production_ui_harness.py" | awk '{print $1}')"
  stage5f_b3f_provenance_harness_sha256="$(shasum -a 256 "$repo_root/scripts/handoff_provenance_negative_harness.py" | awk '{print $1}')"
  stage5f_b3f_snapshot_wrapper_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_b3f_snapshot_provenance_gate.sh" | awk '{print $1}')"
  stage5f_b3f_snapshot_provenance_wrapper_sha256="$stage5f_b3f_snapshot_wrapper_sha256"
  stage5f_ci_workflow_sha256="$(shasum -a 256 "$repo_root/.github/workflows/ci.yml" | awk '{print $1}')"
  stage5f_ci_snapshot_checker_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_ci_snapshot_inheritance_check.py" | awk '{print $1}')"
  stage5f_ci_snapshot_negative_harness_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_ci_snapshot_inheritance_negative_harness.py" | awk '{print $1}')"
  stage5f_atomic_hybrid_semantics_gate_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_atomic_hybrid_semantics_gate.sh" | awk '{print $1}')"
  stage5f_ci_snapshot_inheritance_check_sha256="$stage5f_ci_snapshot_checker_sha256"
  stage5f_atomic_hybrid_semantics_negative_harness_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_atomic_hybrid_semantics_negative_harness.py" | awk '{print $1}')"
  stage5f_ci_snapshot_inheritance_negative_harness_sha256="$stage5f_ci_snapshot_negative_harness_sha256"

  stage5f_gate_stdout="$(mktemp "$archive_dir/stage5f-gate-stdout.XXXXXX")"
  stage5f_gate_stderr="$(mktemp "$archive_dir/stage5f-gate-stderr.XXXXXX")"
  stage5f_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  set +e
  (
    cd "$repo_root"
    bash scripts/stage5f_atomic_hybrid_semantics_gate.sh
  ) >"$stage5f_gate_stdout" 2>"$stage5f_gate_stderr"
  stage5f_exit_code="$?"
  set -e
  stage5f_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  if [[ "$stage5f_exit_code" -ne 0 ]]; then
    cat "$stage5f_gate_stdout"
    cat "$stage5f_gate_stderr" >&2
    rm -f "$stage5f_gate_stdout" "$stage5f_gate_stderr"
    echo "Stage 5F gate failed before handoff packaging." >&2
    exit "$stage5f_exit_code"
  fi
  cp "$stage5f_gate_stdout" "$stage5f_gate_stdout_log"
  cp "$stage5f_gate_stderr" "$stage5f_gate_stderr_log"
  STAGE5F_STARTED_AT_UTC="$stage5f_started_at_utc" \
  STAGE5F_FINISHED_AT_UTC="$stage5f_finished_at_utc" \
  STAGE5F_EXIT_CODE="$stage5f_exit_code" \
  STAGE5F_STDOUT_SHA256="$(shasum -a 256 "$stage5f_gate_stdout" | awk '{print $1}')" \
  STAGE5F_STDERR_SHA256="$(shasum -a 256 "$stage5f_gate_stderr" | awk '{print $1}')" \
  STAGE5F_STDOUT_LINE_COUNT="$(wc -l <"$stage5f_gate_stdout" | tr -d ' ')" \
  STAGE5F_STDERR_LINE_COUNT="$(wc -l <"$stage5f_gate_stderr" | tr -d ' ')" \
  SOURCE_REF="$source_ref" \
  STAGE5C_CHECKER_SHA256="$stage5c_checker_sha256" \
  STAGE5D_CHECKER_SHA256="$stage5d_checker_sha256" \
  STAGE5D_MANIFEST_SHA256="$stage5d_manifest_sha256" \
  B3F_ACTIVE_DESCRIPTOR_SHA256="$stage5f_b3f_active_descriptor_sha256" \
  B3F_CHECKER_SHA256="$stage5f_b3f_checker_sha256" \
  B3F_INVENTORY_SHA256="$stage5f_b3f_inventory_sha256" \
  B3F_PLAN_SHA256="$stage5f_b3f_plan_sha256" \
  B3F_UI_HARNESS_SHA256="$stage5f_b3f_ui_harness_sha256" \
  B3F_PROVENANCE_HARNESS_SHA256="$stage5f_b3f_provenance_harness_sha256" \
  B3F_SNAPSHOT_WRAPPER_SHA256="$stage5f_b3f_snapshot_wrapper_sha256" \
  STAGE5F_ATOMIC_GATE_SHA256="$stage5f_atomic_hybrid_semantics_gate_sha256" \
  STAGE5F_CI_WORKFLOW_SHA256="$stage5f_ci_workflow_sha256" \
  STAGE5F_CI_SNAPSHOT_CHECKER_SHA256="$stage5f_ci_snapshot_checker_sha256" \
  STAGE5F_CI_SNAPSHOT_NEGATIVE_HARNESS_SHA256="$stage5f_ci_snapshot_negative_harness_sha256" \
  STAGE5F_ATOMIC_NEGATIVE_HARNESS_SHA256="$stage5f_atomic_hybrid_semantics_negative_harness_sha256" \
  STAGE5F_ACTIVE_DESCRIPTOR_SHA256="$stage5f_active_descriptor_sha256" \
  STAGE5F_CHECKER_SHA256="$stage5f_checker_sha256" \
  STAGE5F_DESCRIPTOR_REGISTRY_SHA256="$stage5f_descriptor_registry_sha256" \
  STAGE5F_INVENTORY_SHA256="$stage5f_inventory_sha256" \
  STAGE5F_PLAN_SHA256="$stage5f_plan_sha256" \
  DESIGN_BASELINE_REF="$stage5f_baseline_ref" \
  DESIGN_CHANGED_PATHS_JSON="$stage5f_changed_paths_json" \
  DESIGN_CHANGED_PATHS_SHA256="$stage5f_changed_paths_sha256" \
  DESIGN_HEAD_TREE="$stage5f_head_tree" \
  python3 - "$stage5f_gate_result" <<'PY'
import json
import os
import sys
from pathlib import Path

result = {
    "schema_version": 1,
    "gate_id": "stage5f_atomic_hybrid_semantics",
    "command": ["bash", "scripts/stage5f_atomic_hybrid_semantics_gate.sh"],
    "source_ref": os.environ["SOURCE_REF"],
    "accepted_stage5e_b3f_source_ref": "e14654f7129aa61011931306140a3bfefe2fcfbc",
    "started_at_utc": os.environ["STAGE5F_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["STAGE5F_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["STAGE5F_EXIT_CODE"]),
    "stdout_sha256": os.environ["STAGE5F_STDOUT_SHA256"],
    "stderr_sha256": os.environ["STAGE5F_STDERR_SHA256"],
    "stdout_member": "handoff-stage5f-gate-stdout.txt",
    "stderr_member": "handoff-stage5f-gate-stderr.txt",
    "stdout_line_count": int(os.environ["STAGE5F_STDOUT_LINE_COUNT"]),
    "stderr_line_count": int(os.environ["STAGE5F_STDERR_LINE_COUNT"]),
    "input_sha256": {
        "stage5c_checker": os.environ["STAGE5C_CHECKER_SHA256"],
        "stage5d_checker": os.environ["STAGE5D_CHECKER_SHA256"],
        "stage5d_manifest": os.environ["STAGE5D_MANIFEST_SHA256"],
        "stage5e_b3f_active_descriptor": os.environ["B3F_ACTIVE_DESCRIPTOR_SHA256"],
        "stage5e_b3f_checker": os.environ["B3F_CHECKER_SHA256"],
        "stage5e_b3f_inventory": os.environ["B3F_INVENTORY_SHA256"],
        "stage5e_b3f_plan": os.environ["B3F_PLAN_SHA256"],
        "stage5e_b3f_production_ui_harness": os.environ["B3F_UI_HARNESS_SHA256"],
        "stage5e_b3f_provenance_negative_harness": os.environ["B3F_PROVENANCE_HARNESS_SHA256"],
        "stage5f_b3f_snapshot_provenance_gate": os.environ["B3F_SNAPSHOT_WRAPPER_SHA256"],
        "stage5f_atomic_hybrid_semantics_gate": os.environ["STAGE5F_ATOMIC_GATE_SHA256"],
        "stage5f_atomic_hybrid_semantics_negative_harness": os.environ["STAGE5F_ATOMIC_NEGATIVE_HARNESS_SHA256"],
        "stage5f_ci_workflow": os.environ["STAGE5F_CI_WORKFLOW_SHA256"],
        "stage5f_ci_snapshot_inheritance_check": os.environ["STAGE5F_CI_SNAPSHOT_CHECKER_SHA256"],
        "stage5f_ci_snapshot_inheritance_negative_harness": os.environ["STAGE5F_CI_SNAPSHOT_NEGATIVE_HARNESS_SHA256"],
        "stage5f_active_descriptor": os.environ["STAGE5F_ACTIVE_DESCRIPTOR_SHA256"],
        "stage5f_checker": os.environ["STAGE5F_CHECKER_SHA256"],
        "stage5f_descriptor_registry": os.environ["STAGE5F_DESCRIPTOR_REGISTRY_SHA256"],
        "stage5f_inventory": os.environ["STAGE5F_INVENTORY_SHA256"],
        "stage5f_plan": os.environ["STAGE5F_PLAN_SHA256"],
    },
    "design_scope": {
        "baseline_ref": os.environ["DESIGN_BASELINE_REF"],
        "changed_paths": json.loads(os.environ["DESIGN_CHANGED_PATHS_JSON"]),
        "changed_paths_sha256": os.environ["DESIGN_CHANGED_PATHS_SHA256"],
        "head_tree": os.environ["DESIGN_HEAD_TREE"],
        "source_ref": os.environ["SOURCE_REF"],
    },
}
Path(sys.argv[1]).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
PY
  rm -f "$stage5f_gate_stdout" "$stage5f_gate_stderr"
else
stage5e_descriptor_json="$(python3 "$repo_root/scripts/stage5e_descriptor.py" --root "$repo_root")"
stage5e_inventory_rel="$(STAGE5E_DESCRIPTOR_JSON="$stage5e_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5E_DESCRIPTOR_JSON"])["inventory"])')"
stage5e_plan_rel="$(STAGE5E_DESCRIPTOR_JSON="$stage5e_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5E_DESCRIPTOR_JSON"])["plan"])')"
stage5e_checker_rel="$(STAGE5E_DESCRIPTOR_JSON="$stage5e_descriptor_json" python3 -c 'import json,os; print(json.loads(os.environ["STAGE5E_DESCRIPTOR_JSON"])["checker"])')"
if [[ -f "$repo_root/$stage5e_checker_rel" ]] && [[ -f "$repo_root/$stage5e_inventory_rel" ]]; then
  stage5e_checker_sha256="$(shasum -a 256 "$repo_root/$stage5e_checker_rel" | awk '{print $1}')"
  stage5e_inventory_sha256="$(shasum -a 256 "$repo_root/$stage5e_inventory_rel" | awk '{print $1}')"
  stage5e_plan_sha256="$(shasum -a 256 "$repo_root/$stage5e_plan_rel" | awk '{print $1}')"
  stage5e_active_descriptor_sha256="$(shasum -a 256 "$repo_root/docs/stage-5/stage5e-active-descriptor.json" | awk '{print $1}')"
  stage5e_descriptor_registry_sha256="$(shasum -a 256 "$repo_root/scripts/stage5e_descriptor.py" | awk '{print $1}')"
  stage5e_baseline_ref="$(python3 - "$repo_root/$stage5e_inventory_rel" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["baseline_ref"])
PY
)"
  stage5e_head_tree="$(git -C "$repo_root" rev-parse HEAD^{tree})"
  stage5e_changed_paths_json="$(git -C "$repo_root" diff --name-only "$stage5e_baseline_ref" -- | python3 -c 'import json,sys; print(json.dumps([line.strip() for line in sys.stdin if line.strip()], separators=(",", ":"), sort_keys=True))')"
  stage5e_changed_paths_sha256="$(printf '%s' "$stage5e_changed_paths_json" | shasum -a 256 | awk '{print $1}')"
  stage5e_design_scope_sha256="$(BASELINE_REF="$stage5e_baseline_ref" HEAD_TREE="$stage5e_head_tree" CHANGED_PATHS_JSON="$stage5e_changed_paths_json" CHANGED_PATHS_SHA256="$stage5e_changed_paths_sha256" SOURCE_REF="$source_ref" python3 - <<'PY'
import hashlib
import json
import os

payload = {
    "baseline_ref": os.environ["BASELINE_REF"],
    "changed_paths": json.loads(os.environ["CHANGED_PATHS_JSON"]),
    "changed_paths_sha256": os.environ["CHANGED_PATHS_SHA256"],
    "head_tree": os.environ["HEAD_TREE"],
    "source_ref": os.environ["SOURCE_REF"],
}
  canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
print(hashlib.sha256(canonical).hexdigest())
PY
)"
  design_baseline_ref="$stage5e_baseline_ref"
  design_changed_paths_json="$stage5e_changed_paths_json"
  design_head_tree="$stage5e_head_tree"
  current_review_stage="$(python3 - "$repo_root/$stage5e_inventory_rel" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["stage"])
PY
)"
  stage5e_stdout="$(mktemp "$archive_dir/stage5e-gate-stdout.XXXXXX")"
  stage5e_stderr="$(mktemp "$archive_dir/stage5e-gate-stderr.XXXXXX")"
  stage5e_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  set +e
  (
    cd "$repo_root"
    bash scripts/stage5e_lifecycle_event_time_gate.sh
  ) >"$stage5e_stdout" 2>"$stage5e_stderr"
  stage5e_exit_code="$?"
  set -e
  stage5e_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  if [[ "$stage5e_exit_code" -ne 0 ]]; then
    cat "$stage5e_stdout"
    cat "$stage5e_stderr" >&2
    rm -f "$stage5e_stdout" "$stage5e_stderr"
    echo "Stage 5E gate failed before handoff packaging." >&2
    exit "$stage5e_exit_code"
  fi
  cp "$stage5e_stdout" "$stage5e_gate_stdout_log"
  cp "$stage5e_stderr" "$stage5e_gate_stderr_log"
  STAGE5E_STARTED_AT_UTC="$stage5e_started_at_utc" \
  STAGE5E_FINISHED_AT_UTC="$stage5e_finished_at_utc" \
  STAGE5E_EXIT_CODE="$stage5e_exit_code" \
  STAGE5E_STDOUT_SHA256="$(shasum -a 256 "$stage5e_stdout" | awk '{print $1}')" \
  STAGE5E_STDERR_SHA256="$(shasum -a 256 "$stage5e_stderr" | awk '{print $1}')" \
  STAGE5E_STDOUT_LINE_COUNT="$(wc -l <"$stage5e_stdout" | tr -d ' ')" \
  STAGE5E_STDERR_LINE_COUNT="$(wc -l <"$stage5e_stderr" | tr -d ' ')" \
  SOURCE_REF="$source_ref" \
  STAGE5C_CHECKER_SHA256="$stage5c_checker_sha256" \
  STAGE5D_CHECKER_SHA256="$stage5d_checker_sha256" \
  STAGE5D_MANIFEST_SHA256="$stage5d_manifest_sha256" \
  STAGE5E_CHECKER_SHA256="$stage5e_checker_sha256" \
  STAGE5E_INVENTORY_SHA256="$stage5e_inventory_sha256" \
  STAGE5E_PLAN_SHA256="$stage5e_plan_sha256" \
  STAGE5E_ACTIVE_DESCRIPTOR_SHA256="$stage5e_active_descriptor_sha256" \
  STAGE5E_DESCRIPTOR_REGISTRY_SHA256="$stage5e_descriptor_registry_sha256" \
  DESIGN_BASELINE_REF="$stage5e_baseline_ref" \
  DESIGN_CHANGED_PATHS_JSON="$stage5e_changed_paths_json" \
  DESIGN_CHANGED_PATHS_SHA256="$stage5e_changed_paths_sha256" \
  DESIGN_HEAD_TREE="$stage5e_head_tree" \
  python3 - "$stage5e_gate_result" <<'PY'
import json
import os
import sys
from pathlib import Path

result = {
    "schema_version": 1,
    "gate_id": "stage5e_lifecycle_event_time",
    "command": ["bash", "scripts/stage5e_lifecycle_event_time_gate.sh"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["STAGE5E_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["STAGE5E_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["STAGE5E_EXIT_CODE"]),
    "stdout_sha256": os.environ["STAGE5E_STDOUT_SHA256"],
    "stderr_sha256": os.environ["STAGE5E_STDERR_SHA256"],
    "stdout_member": "handoff-stage5e-gate-stdout.txt",
    "stderr_member": "handoff-stage5e-gate-stderr.txt",
    "stdout_line_count": int(os.environ["STAGE5E_STDOUT_LINE_COUNT"]),
    "stderr_line_count": int(os.environ["STAGE5E_STDERR_LINE_COUNT"]),
    "input_sha256": {
        "stage5c_checker": os.environ["STAGE5C_CHECKER_SHA256"],
        "stage5d_checker": os.environ["STAGE5D_CHECKER_SHA256"],
        "stage5d_manifest": os.environ["STAGE5D_MANIFEST_SHA256"],
        "stage5e_checker": os.environ["STAGE5E_CHECKER_SHA256"],
        "stage5e_inventory": os.environ["STAGE5E_INVENTORY_SHA256"],
        "stage5e_plan": os.environ["STAGE5E_PLAN_SHA256"],
        "stage5e_active_descriptor": os.environ["STAGE5E_ACTIVE_DESCRIPTOR_SHA256"],
        "stage5e_descriptor_registry": os.environ["STAGE5E_DESCRIPTOR_REGISTRY_SHA256"],
    },
    "design_scope": {
        "baseline_ref": os.environ["DESIGN_BASELINE_REF"],
        "changed_paths": json.loads(os.environ["DESIGN_CHANGED_PATHS_JSON"]),
        "changed_paths_sha256": os.environ["DESIGN_CHANGED_PATHS_SHA256"],
        "head_tree": os.environ["DESIGN_HEAD_TREE"],
        "source_ref": os.environ["SOURCE_REF"],
    },
}
Path(sys.argv[1]).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
PY
  rm -f "$stage5e_stdout" "$stage5e_stderr"
fi
fi

if [[ -n "$(git -C "$repo_root" status --porcelain --untracked-files=no)" ]]; then
  echo "Refusing to build review handoff: tracked source tree changed after gate." >&2
  git -C "$repo_root" status --short --untracked-files=no >&2
  exit 1
fi

provenance_negative_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
provenance_tested_source_ref="$source_ref"
if [[ "$stage5f_enabled" -eq 1 ]]; then
  # The 580-case B3F harness is intentionally evaluated only through the
  # shared immutable-snapshot wrapper. CI uses this exact same runner.
  provenance_tested_source_ref="$stage5f_baseline_ref"
  set +e
  (
    set -euo pipefail
    cd "$repo_root"
    bash scripts/stage5f_b3f_snapshot_provenance_gate.sh
  ) >"$provenance_negative_stdout_log" 2>"$provenance_negative_stderr_log"
  provenance_negative_exit_code="$?"
  set -e
else
  set +e
  (
    set -euo pipefail
    cd "$repo_root"
    python3 scripts/handoff_provenance_negative_harness.py
  ) >"$provenance_negative_stdout_log" 2>"$provenance_negative_stderr_log"
  provenance_negative_exit_code="$?"
  set -e
fi
provenance_negative_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if [[ "$provenance_negative_exit_code" -ne 0 ]]; then
  cat "$provenance_negative_stdout_log"
  cat "$provenance_negative_stderr_log" >&2
  echo "Handoff provenance-negative gate failed before packaging." >&2
  exit "$provenance_negative_exit_code"
fi
SOURCE_REF="$source_ref" \
PROVENANCE_NEGATIVE_STARTED_AT_UTC="$provenance_negative_started_at_utc" \
PROVENANCE_NEGATIVE_FINISHED_AT_UTC="$provenance_negative_finished_at_utc" \
PROVENANCE_NEGATIVE_EXIT_CODE="$provenance_negative_exit_code" \
PROVENANCE_NEGATIVE_STDOUT_SHA256="$(shasum -a 256 "$provenance_negative_stdout_log" | awk '{print $1}')" \
PROVENANCE_NEGATIVE_STDERR_SHA256="$(shasum -a 256 "$provenance_negative_stderr_log" | awk '{print $1}')" \
PROVENANCE_NEGATIVE_PASSED_CASES="$(grep -c '^PASS ' "$provenance_negative_stdout_log" || true)" \
PROVENANCE_TESTED_SOURCE_REF="$provenance_tested_source_ref" \
STAGE5F_ENABLED="$stage5f_enabled" \
python3 - "$provenance_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

result = {
    "schema_version": 1,
    "gate_id": "handoff_provenance_negative",
    "command": ["python3", "scripts/handoff_provenance_negative_harness.py"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["PROVENANCE_NEGATIVE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["PROVENANCE_NEGATIVE_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["PROVENANCE_NEGATIVE_EXIT_CODE"]),
    "passed_cases": int(os.environ["PROVENANCE_NEGATIVE_PASSED_CASES"]),
    "stdout_member": "handoff-provenance-negative-stdout.txt",
    "stderr_member": "handoff-provenance-negative-stderr.txt",
    "stdout_sha256": os.environ["PROVENANCE_NEGATIVE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["PROVENANCE_NEGATIVE_STDERR_SHA256"],
}
if os.environ["STAGE5F_ENABLED"] == "1":
    result["tested_source_ref"] = os.environ["PROVENANCE_TESTED_SOURCE_REF"]
    result["command"] = ["bash", "scripts/stage5f_b3f_snapshot_provenance_gate.sh"]
Path(sys.argv[1]).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
PY

stage5d_negative_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
set +e
(
  set -euo pipefail
  cd "$repo_root"
  python3 scripts/stage5d_additive_freeze_negative_harness.py
) >"$stage5d_negative_stdout_log" 2>"$stage5d_negative_stderr_log"
stage5d_negative_exit_code="$?"
set -e
stage5d_negative_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if [[ "$stage5d_negative_exit_code" -ne 0 ]]; then
  cat "$stage5d_negative_stdout_log"
  cat "$stage5d_negative_stderr_log" >&2
  echo "Stage 5D negative gate failed before packaging." >&2
  exit "$stage5d_negative_exit_code"
fi
stage5d_negative_passed_cases="$(grep -c '^PASS ' "$stage5d_negative_stdout_log" || true)"
if [[ "$stage5d_negative_passed_cases" -ne 303 ]]; then
  echo "Stage 5D negative gate case-count mismatch: $stage5d_negative_passed_cases" >&2
  exit 1
fi
SOURCE_REF="$source_ref" \
NEGATIVE_STARTED_AT_UTC="$stage5d_negative_started_at_utc" \
NEGATIVE_FINISHED_AT_UTC="$stage5d_negative_finished_at_utc" \
NEGATIVE_EXIT_CODE="$stage5d_negative_exit_code" \
NEGATIVE_PASSED_CASES="$stage5d_negative_passed_cases" \
NEGATIVE_STDOUT_SHA256="$(shasum -a 256 "$stage5d_negative_stdout_log" | awk '{print $1}')" \
NEGATIVE_STDERR_SHA256="$(shasum -a 256 "$stage5d_negative_stderr_log" | awk '{print $1}')" \
python3 - "$stage5d_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "gate_id": "stage5d_additive_freeze_negative",
    "command": ["python3", "scripts/stage5d_additive_freeze_negative_harness.py"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["NEGATIVE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["NEGATIVE_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["NEGATIVE_EXIT_CODE"]),
    "passed_cases": int(os.environ["NEGATIVE_PASSED_CASES"]),
    "stdout_member": "handoff-stage5d-negative-stdout.txt",
    "stderr_member": "handoff-stage5d-negative-stderr.txt",
    "stdout_sha256": os.environ["NEGATIVE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["NEGATIVE_STDERR_SHA256"],
}, indent=2, sort_keys=True) + "\n")
PY

if [[ "$stage5f_enabled" -eq 1 ]]; then
  stage5f_negative_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  set +e
  (
    set -euo pipefail
    cd "$repo_root"
    python3 scripts/stage5f_atomic_hybrid_semantics_negative_harness.py
  ) >"$stage5f_negative_stdout_log" 2>"$stage5f_negative_stderr_log"
  stage5f_negative_exit_code="$?"
  set -e
  stage5f_negative_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  if [[ "$stage5f_negative_exit_code" -ne 0 ]]; then
    cat "$stage5f_negative_stdout_log"
    cat "$stage5f_negative_stderr_log" >&2
    echo "Stage 5F negative gate failed before packaging." >&2
    exit "$stage5f_negative_exit_code"
  fi
  stage5f_negative_passed_cases="$(grep -c '^PASS ' "$stage5f_negative_stdout_log" || true)"
  if [[ "$stage5f_negative_passed_cases" -ne 13 ]]; then
    echo "Stage 5F negative gate case-count mismatch: $stage5f_negative_passed_cases" >&2
    exit 1
  fi
  SOURCE_REF="$source_ref" \
  NEGATIVE_STARTED_AT_UTC="$stage5f_negative_started_at_utc" \
  NEGATIVE_FINISHED_AT_UTC="$stage5f_negative_finished_at_utc" \
  NEGATIVE_EXIT_CODE="$stage5f_negative_exit_code" \
  NEGATIVE_PASSED_CASES="$stage5f_negative_passed_cases" \
  NEGATIVE_STDOUT_SHA256="$(shasum -a 256 "$stage5f_negative_stdout_log" | awk '{print $1}')" \
  NEGATIVE_STDERR_SHA256="$(shasum -a 256 "$stage5f_negative_stderr_log" | awk '{print $1}')" \
  python3 - "$stage5f_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "gate_id": "stage5f_atomic_hybrid_semantics_negative",
    "command": ["python3", "scripts/stage5f_atomic_hybrid_semantics_negative_harness.py"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["NEGATIVE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["NEGATIVE_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["NEGATIVE_EXIT_CODE"]),
    "passed_cases": int(os.environ["NEGATIVE_PASSED_CASES"]),
    "stdout_member": "handoff-stage5f-negative-stdout.txt",
    "stderr_member": "handoff-stage5f-negative-stderr.txt",
    "stdout_sha256": os.environ["NEGATIVE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["NEGATIVE_STDERR_SHA256"],
}, indent=2, sort_keys=True) + "\n")
PY

  stage5f_ci_negative_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  set +e
  (
    set -euo pipefail
    cd "$repo_root"
    python3 scripts/stage5f_ci_snapshot_inheritance_negative_harness.py
  ) >"$stage5f_ci_negative_stdout_log" 2>"$stage5f_ci_negative_stderr_log"
  stage5f_ci_negative_exit_code="$?"
  set -e
  stage5f_ci_negative_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  if [[ "$stage5f_ci_negative_exit_code" -ne 0 ]]; then
    cat "$stage5f_ci_negative_stdout_log"
    cat "$stage5f_ci_negative_stderr_log" >&2
    echo "Stage 5F CI snapshot negative gate failed before packaging." >&2
    exit "$stage5f_ci_negative_exit_code"
  fi
  stage5f_ci_negative_passed_cases="$(grep -c '^PASS ' "$stage5f_ci_negative_stdout_log" || true)"
  if [[ "$stage5f_ci_negative_passed_cases" -ne 16 ]]; then
    echo "Stage 5F CI snapshot negative gate case-count mismatch: $stage5f_ci_negative_passed_cases" >&2
    exit 1
  fi
  SOURCE_REF="$source_ref" \
  NEGATIVE_STARTED_AT_UTC="$stage5f_ci_negative_started_at_utc" \
  NEGATIVE_FINISHED_AT_UTC="$stage5f_ci_negative_finished_at_utc" \
  NEGATIVE_EXIT_CODE="$stage5f_ci_negative_exit_code" \
  NEGATIVE_PASSED_CASES="$stage5f_ci_negative_passed_cases" \
  NEGATIVE_STDOUT_SHA256="$(shasum -a 256 "$stage5f_ci_negative_stdout_log" | awk '{print $1}')" \
  NEGATIVE_STDERR_SHA256="$(shasum -a 256 "$stage5f_ci_negative_stderr_log" | awk '{print $1}')" \
  python3 - "$stage5f_ci_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "gate_id": "stage5f_ci_snapshot_inheritance_negative",
    "command": ["python3", "scripts/stage5f_ci_snapshot_inheritance_negative_harness.py"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["NEGATIVE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["NEGATIVE_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["NEGATIVE_EXIT_CODE"]),
    "passed_cases": int(os.environ["NEGATIVE_PASSED_CASES"]),
    "stdout_member": "handoff-stage5f-ci-negative-stdout.txt",
    "stderr_member": "handoff-stage5f-ci-negative-stderr.txt",
    "stdout_sha256": os.environ["NEGATIVE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["NEGATIVE_STDERR_SHA256"],
}, indent=2, sort_keys=True) + "\n")
PY
fi

forbidden_negative_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
set +e
(
  set -euo pipefail
  cd "$repo_root"
  bash scripts/forbidden_surface_negative_harness.sh
) >"$forbidden_negative_stdout_log" 2>"$forbidden_negative_stderr_log"
forbidden_negative_exit_code="$?"
set -e
forbidden_negative_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if [[ "$forbidden_negative_exit_code" -ne 0 ]]; then
  cat "$forbidden_negative_stdout_log"
  cat "$forbidden_negative_stderr_log" >&2
  echo "Forbidden-surface negative gate failed before packaging." >&2
  exit "$forbidden_negative_exit_code"
fi
forbidden_negative_passed_cases="$(grep -c '^PASS ' "$forbidden_negative_stdout_log" || true)"
if [[ "$forbidden_negative_passed_cases" -ne 87 ]]; then
  echo "Forbidden-surface negative gate case-count mismatch: $forbidden_negative_passed_cases" >&2
  exit 1
fi
SOURCE_REF="$source_ref" \
NEGATIVE_STARTED_AT_UTC="$forbidden_negative_started_at_utc" \
NEGATIVE_FINISHED_AT_UTC="$forbidden_negative_finished_at_utc" \
NEGATIVE_EXIT_CODE="$forbidden_negative_exit_code" \
NEGATIVE_PASSED_CASES="$forbidden_negative_passed_cases" \
NEGATIVE_STDOUT_SHA256="$(shasum -a 256 "$forbidden_negative_stdout_log" | awk '{print $1}')" \
NEGATIVE_STDERR_SHA256="$(shasum -a 256 "$forbidden_negative_stderr_log" | awk '{print $1}')" \
python3 - "$forbidden_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "gate_id": "forbidden_surface_negative",
    "command": ["bash", "scripts/forbidden_surface_negative_harness.sh"],
    "source_ref": os.environ["SOURCE_REF"],
    "started_at_utc": os.environ["NEGATIVE_STARTED_AT_UTC"],
    "finished_at_utc": os.environ["NEGATIVE_FINISHED_AT_UTC"],
    "exit_code": int(os.environ["NEGATIVE_EXIT_CODE"]),
    "passed_cases": int(os.environ["NEGATIVE_PASSED_CASES"]),
    "stdout_member": "handoff-forbidden-negative-stdout.txt",
    "stderr_member": "handoff-forbidden-negative-stderr.txt",
    "stdout_sha256": os.environ["NEGATIVE_STDOUT_SHA256"],
    "stderr_sha256": os.environ["NEGATIVE_STDERR_SHA256"],
}, indent=2, sort_keys=True) + "\n")
PY

if [[ "$stage5f_enabled" -eq 1 ]]; then
  # Recheck every reviewer-pinned Stage 5F CI executable after all local
  # harnesses have run and immediately before the source-tree manifest is
  # calculated. This prevents a successful earlier check from being reused
  # after a later tool has changed the packaging tree.
  python3 "$repo_root/scripts/handoff_safety_check.py" --source-tree "$repo_root"
  stage5f_ci_workflow_sha256="$(shasum -a 256 "$repo_root/.github/workflows/ci.yml" | awk '{print $1}')"
  stage5f_b3f_snapshot_provenance_wrapper_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_b3f_snapshot_provenance_gate.sh" | awk '{print $1}')"
  stage5f_atomic_hybrid_semantics_gate_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_atomic_hybrid_semantics_gate.sh" | awk '{print $1}')"
  stage5f_ci_snapshot_inheritance_check_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_ci_snapshot_inheritance_check.py" | awk '{print $1}')"
  stage5f_atomic_hybrid_semantics_negative_harness_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_atomic_hybrid_semantics_negative_harness.py" | awk '{print $1}')"
  stage5f_ci_snapshot_inheritance_negative_harness_sha256="$(shasum -a 256 "$repo_root/scripts/stage5f_ci_snapshot_inheritance_negative_harness.py" | awk '{print $1}')"
  stage_generated_members_json='["handoff-commit.txt","handoff-cargo-gate-result.json","handoff-cargo-gate-stderr.txt","handoff-cargo-gate-stdout.txt","handoff-forbidden-negative-result.json","handoff-forbidden-negative-stderr.txt","handoff-forbidden-negative-stdout.txt","handoff-manifest.json","handoff-provenance-negative-result.json","handoff-provenance-negative-stderr.txt","handoff-provenance-negative-stdout.txt","handoff-stage5d-negative-result.json","handoff-stage5d-negative-stderr.txt","handoff-stage5d-negative-stdout.txt","handoff-stage5f-ci-negative-result.json","handoff-stage5f-ci-negative-stderr.txt","handoff-stage5f-ci-negative-stdout.txt","handoff-stage5f-gate-result.json","handoff-stage5f-gate-stderr.txt","handoff-stage5f-gate-stdout.txt","handoff-stage5f-negative-result.json","handoff-stage5f-negative-stderr.txt","handoff-stage5f-negative-stdout.txt","handoff-source-tree-manifest.json"]'
else
  stage_generated_members_json='["handoff-commit.txt","handoff-cargo-gate-result.json","handoff-cargo-gate-stderr.txt","handoff-cargo-gate-stdout.txt","handoff-forbidden-negative-result.json","handoff-forbidden-negative-stderr.txt","handoff-forbidden-negative-stdout.txt","handoff-manifest.json","handoff-provenance-negative-result.json","handoff-provenance-negative-stderr.txt","handoff-provenance-negative-stdout.txt","handoff-stage5d-negative-result.json","handoff-stage5d-negative-stderr.txt","handoff-stage5d-negative-stdout.txt","handoff-stage5e-gate-result.json","handoff-stage5e-gate-stderr.txt","handoff-stage5e-gate-stdout.txt","handoff-source-tree-manifest.json"]'
fi

SOURCE_REF="$source_ref" \
HEAD_TREE="$(git -C "$repo_root" rev-parse HEAD^{tree})" \
BASELINE_REF="$design_baseline_ref" \
CHANGED_PATHS_JSON="$design_changed_paths_json" \
GENERATED_MEMBERS_JSON="$stage_generated_members_json" \
python3 - "$repo_root" "$source_tree_manifest" <<'PY'
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
out = Path(sys.argv[2])
excluded_parts = {".git", "target", "tmp", "reports", "__pycache__", "__MACOSX"}
forbidden_name_patterns = [
    re.compile(r"^\.env$"),
    re.compile(r"^\.env\.(?!example$).+"),
    re.compile(r".*\.log$"),
    re.compile(r".*\.local\..*$"),
]

def path_is_excluded(path: str) -> bool:
    parts = path.split("/")
    name = parts[-1]
    return (
        any(part in excluded_parts for part in parts)
        or any(pattern.fullmatch(name) for pattern in forbidden_name_patterns)
        or name == ".DS_Store"
    )

index_lines = subprocess.check_output(["git", "ls-files", "-s"], cwd=root, text=True).splitlines()
members = []
for line in sorted(index_lines, key=lambda item: item.split("\t", 1)[1]):
    meta, rel = line.split("\t", 1)
    if path_is_excluded(rel):
        continue
    mode = meta.split()[0]
    payload = (root / rel).read_bytes()
    members.append({"git_mode": mode, "path": rel, "sha256": hashlib.sha256(payload).hexdigest()})
manifest = {
    "schema_version": 1,
    "source_ref": os.environ["SOURCE_REF"],
    "head_tree": os.environ["HEAD_TREE"],
    "baseline_ref": os.environ["BASELINE_REF"],
    "changed_paths": json.loads(os.environ["CHANGED_PATHS_JSON"]),
    "excluded_generated_members": json.loads(os.environ["GENERATED_MEMBERS_JSON"]),
    "members": members,
}
out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
PY
source_tree_manifest_sha256="$(shasum -a 256 "$source_tree_manifest" | awk '{print $1}')"
SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
SOURCE_TREE_MEMBER_COUNT="$(python3 - "$source_tree_manifest" <<'PY'
import json
import sys
print(len(json.loads(open(sys.argv[1]).read())["members"]))
PY
)" \
python3 - "$cargo_gate_result" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text())
payload["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
payload["source_tree_member_count"] = int(os.environ["SOURCE_TREE_MEMBER_COUNT"])
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
if [[ -f "$stage5e_gate_result" ]]; then
  SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
  python3 - "$stage5e_gate_result" "$source_tree_manifest" <<'PY'
import json
import os
import sys
from pathlib import Path

gate_path = Path(sys.argv[1])
source_manifest_path = Path(sys.argv[2])
gate = json.loads(gate_path.read_text())
source_manifest = json.loads(source_manifest_path.read_text())
gate["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
gate["source_tree_member_count"] = len(source_manifest["members"])
gate_path.write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")
PY
  stage5e_gate_result_sha256="$(shasum -a 256 "$stage5e_gate_result" | awk '{print $1}')"
fi
if [[ -f "$stage5f_gate_result" ]]; then
  SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
  python3 - "$stage5f_gate_result" "$source_tree_manifest" <<'PY'
import json
import os
import sys
from pathlib import Path

gate_path = Path(sys.argv[1])
source_manifest_path = Path(sys.argv[2])
gate = json.loads(gate_path.read_text())
source_manifest = json.loads(source_manifest_path.read_text())
gate["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
gate["source_tree_member_count"] = len(source_manifest["members"])
gate_path.write_text(json.dumps(gate, indent=2, sort_keys=True) + "\n")
PY
  stage5f_gate_result_sha256="$(shasum -a 256 "$stage5f_gate_result" | awk '{print $1}')"
fi
SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
SOURCE_TREE_MEMBER_COUNT="$(python3 - "$source_tree_manifest" <<'PY'
import json
import sys
print(len(json.loads(open(sys.argv[1]).read())["members"]))
PY
)" \
python3 - "$provenance_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text())
payload["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
payload["source_tree_member_count"] = int(os.environ["SOURCE_TREE_MEMBER_COUNT"])
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
SOURCE_TREE_MEMBER_COUNT="$(python3 - "$source_tree_manifest" <<'PY'
import json
import sys
print(len(json.loads(open(sys.argv[1]).read())["members"]))
PY
)" \
python3 - "$stage5d_negative_result" "$forbidden_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

for raw_path in sys.argv[1:]:
    path = Path(raw_path)
    payload = json.loads(path.read_text())
    payload["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
    payload["source_tree_member_count"] = int(os.environ["SOURCE_TREE_MEMBER_COUNT"])
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
if [[ -f "$stage5f_negative_result" ]]; then
  SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
  SOURCE_TREE_MEMBER_COUNT="$(python3 - "$source_tree_manifest" <<'PY'
import json
import sys
print(len(json.loads(open(sys.argv[1]).read())["members"]))
PY
)" \
  python3 - "$stage5f_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text())
payload["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
payload["source_tree_member_count"] = int(os.environ["SOURCE_TREE_MEMBER_COUNT"])
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
  stage5f_negative_result_sha256="$(shasum -a 256 "$stage5f_negative_result" | awk '{print $1}')"
fi
if [[ -f "$stage5f_ci_negative_result" ]]; then
  SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
  SOURCE_TREE_MEMBER_COUNT="$(python3 - "$source_tree_manifest" <<'PY'
import json
import sys
print(len(json.loads(open(sys.argv[1]).read())["members"]))
PY
)" \
  python3 - "$stage5f_ci_negative_result" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
payload = json.loads(path.read_text())
payload["source_tree_manifest_sha256"] = os.environ["SOURCE_TREE_MANIFEST_SHA256"]
payload["source_tree_member_count"] = int(os.environ["SOURCE_TREE_MEMBER_COUNT"])
path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
PY
  stage5f_ci_negative_result_sha256="$(shasum -a 256 "$stage5f_ci_negative_result" | awk '{print $1}')"
fi
provenance_negative_result_sha256="$(shasum -a 256 "$provenance_negative_result" | awk '{print $1}')"
stage5d_negative_result_sha256="$(shasum -a 256 "$stage5d_negative_result" | awk '{print $1}')"
forbidden_negative_result_sha256="$(shasum -a 256 "$forbidden_negative_result" | awk '{print $1}')"

created_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

SOURCE_COMMIT="$source_commit" \
SOURCE_REF="$source_ref" \
ARCHIVE_NAME="$archive_name" \
CREATED_AT_UTC="$created_at_utc" \
STAGE5C_CHECKER_SHA256="$stage5c_checker_sha256" \
STAGE5D_CHECKER_SHA256="$stage5d_checker_sha256" \
STAGE5D_MANIFEST_SHA256="$stage5d_manifest_sha256" \
STAGE5E_CHECKER_SHA256="$stage5e_checker_sha256" \
STAGE5E_INVENTORY_SHA256="$stage5e_inventory_sha256" \
STAGE5E_PLAN_SHA256="$stage5e_plan_sha256" \
STAGE5E_GATE_RESULT_SHA256="$stage5e_gate_result_sha256" \
STAGE5E_DESIGN_SCOPE_SHA256="$stage5e_design_scope_sha256" \
STAGE5F_ENABLED="$stage5f_enabled" \
STAGE5F_CHECKER_SHA256="$stage5f_checker_sha256" \
STAGE5F_INVENTORY_SHA256="$stage5f_inventory_sha256" \
STAGE5F_PLAN_SHA256="$stage5f_plan_sha256" \
STAGE5F_ACTIVE_DESCRIPTOR_SHA256="$stage5f_active_descriptor_sha256" \
STAGE5F_DESCRIPTOR_REGISTRY_SHA256="$stage5f_descriptor_registry_sha256" \
STAGE5F_GATE_RESULT_SHA256="$stage5f_gate_result_sha256" \
STAGE5F_NEGATIVE_RESULT_SHA256="$stage5f_negative_result_sha256" \
STAGE5F_CI_NEGATIVE_RESULT_SHA256="$stage5f_ci_negative_result_sha256" \
STAGE5F_CI_WORKFLOW_SHA256="$stage5f_ci_workflow_sha256" \
STAGE5F_B3F_SNAPSHOT_PROVENANCE_WRAPPER_SHA256="$stage5f_b3f_snapshot_provenance_wrapper_sha256" \
STAGE5F_ATOMIC_HYBRID_SEMANTICS_GATE_SHA256="$stage5f_atomic_hybrid_semantics_gate_sha256" \
STAGE5F_CI_SNAPSHOT_INHERITANCE_CHECK_SHA256="$stage5f_ci_snapshot_inheritance_check_sha256" \
STAGE5F_ATOMIC_HYBRID_SEMANTICS_NEGATIVE_HARNESS_SHA256="$stage5f_atomic_hybrid_semantics_negative_harness_sha256" \
STAGE5F_CI_SNAPSHOT_INHERITANCE_NEGATIVE_HARNESS_SHA256="$stage5f_ci_snapshot_inheritance_negative_harness_sha256" \
STAGE5F_DESIGN_SCOPE_SHA256="$stage5f_design_scope_sha256" \
SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
CARGO_GATE_RESULT_SHA256="$(shasum -a 256 "$cargo_gate_result" | awk '{print $1}')" \
PROVENANCE_NEGATIVE_RESULT_SHA256="$provenance_negative_result_sha256" \
STAGE5D_NEGATIVE_RESULT_SHA256="$stage5d_negative_result_sha256" \
FORBIDDEN_NEGATIVE_RESULT_SHA256="$forbidden_negative_result_sha256" \
REVIEW_STAGE="$review_stage" \
CURRENT_REVIEW_STAGE="$current_review_stage" \
HANDOFF_MANIFEST="$handoff_manifest" \
python3 - <<'PY'
import json
import os
from pathlib import Path

manifest = {
    "schema_version": 1,
    "current_review_stage": os.environ["CURRENT_REVIEW_STAGE"],
    "review_stage": os.environ["REVIEW_STAGE"],
    "source_commit": os.environ["SOURCE_COMMIT"],
    "source_ref": os.environ["SOURCE_REF"],
    "archive_name": os.environ["ARCHIVE_NAME"],
    "created_at_utc": os.environ["CREATED_AT_UTC"],
    "stage5c_checker_sha256": os.environ["STAGE5C_CHECKER_SHA256"],
    "stage5d_checker_sha256": os.environ["STAGE5D_CHECKER_SHA256"],
    "stage5d_manifest_sha256": os.environ["STAGE5D_MANIFEST_SHA256"],
    "source_tree_manifest_sha256": os.environ["SOURCE_TREE_MANIFEST_SHA256"],
    "cargo_gate_result_sha256": os.environ["CARGO_GATE_RESULT_SHA256"],
    "provenance_negative_result_sha256": os.environ["PROVENANCE_NEGATIVE_RESULT_SHA256"],
    "stage5d_negative_result_sha256": os.environ["STAGE5D_NEGATIVE_RESULT_SHA256"],
    "forbidden_negative_result_sha256": os.environ["FORBIDDEN_NEGATIVE_RESULT_SHA256"],
}
if os.environ["STAGE5F_ENABLED"] == "1":
    manifest.update({
        "stage5f_checker_sha256": os.environ["STAGE5F_CHECKER_SHA256"],
        "stage5f_inventory_sha256": os.environ["STAGE5F_INVENTORY_SHA256"],
        "stage5f_plan_sha256": os.environ["STAGE5F_PLAN_SHA256"],
        "stage5f_active_descriptor_sha256": os.environ["STAGE5F_ACTIVE_DESCRIPTOR_SHA256"],
        "stage5f_descriptor_registry_sha256": os.environ["STAGE5F_DESCRIPTOR_REGISTRY_SHA256"],
        "stage5f_gate_result_sha256": os.environ["STAGE5F_GATE_RESULT_SHA256"],
        "stage5f_negative_result_sha256": os.environ["STAGE5F_NEGATIVE_RESULT_SHA256"],
        "stage5f_ci_negative_result_sha256": os.environ["STAGE5F_CI_NEGATIVE_RESULT_SHA256"],
        "stage5f_ci_workflow_sha256": os.environ["STAGE5F_CI_WORKFLOW_SHA256"],
        "stage5f_b3f_snapshot_provenance_wrapper_sha256": os.environ["STAGE5F_B3F_SNAPSHOT_PROVENANCE_WRAPPER_SHA256"],
        "stage5f_atomic_hybrid_semantics_gate_sha256": os.environ["STAGE5F_ATOMIC_HYBRID_SEMANTICS_GATE_SHA256"],
        "stage5f_ci_snapshot_inheritance_check_sha256": os.environ["STAGE5F_CI_SNAPSHOT_INHERITANCE_CHECK_SHA256"],
        "stage5f_atomic_hybrid_semantics_negative_harness_sha256": os.environ["STAGE5F_ATOMIC_HYBRID_SEMANTICS_NEGATIVE_HARNESS_SHA256"],
        "stage5f_ci_snapshot_inheritance_negative_harness_sha256": os.environ["STAGE5F_CI_SNAPSHOT_INHERITANCE_NEGATIVE_HARNESS_SHA256"],
        "stage5f_design_scope_sha256": os.environ["STAGE5F_DESIGN_SCOPE_SHA256"],
        "required_gate_names": [
            "stage5f_atomic_hybrid_semantics",
            "stage5f_atomic_hybrid_semantics_negative",
            "stage5f_ci_snapshot_inheritance_negative",
            "stage5f_b3f_snapshot_provenance",
            "stage5c_api_freeze",
            "stage5d_additive_freeze",
            "forbidden_surface",
            "forbidden_surface_negative",
            "stage5d_negative",
            "handoff_provenance_negative",
            "no_redis_smoke",
            "python_syntax",
            "fixture_parse",
            "handoff_source_safety",
            "handoff_archive_safety",
            "checker_input_completeness",
            "cargo_fmt",
            "cargo_test_all_targets",
            "cargo_test_docs",
            "cargo_clippy",
        ],
    })
else:
    manifest.update({
        "stage5e_checker_sha256": os.environ["STAGE5E_CHECKER_SHA256"],
        "stage5e_inventory_sha256": os.environ["STAGE5E_INVENTORY_SHA256"],
        "stage5e_plan_sha256": os.environ["STAGE5E_PLAN_SHA256"],
        "stage5e_gate_result_sha256": os.environ["STAGE5E_GATE_RESULT_SHA256"],
        "stage5e_design_scope_sha256": os.environ["STAGE5E_DESIGN_SCOPE_SHA256"],
        "required_gate_names": [
            "stage5e_lifecycle_event_time",
            "stage5c_api_freeze",
            "stage5d_additive_freeze",
            "forbidden_surface",
            "forbidden_surface_negative",
            "stage5d_negative",
            "handoff_provenance_negative",
            "no_redis_smoke",
            "python_syntax",
            "fixture_parse",
            "handoff_source_safety",
            "handoff_archive_safety",
            "checker_input_completeness",
            "cargo_fmt",
            "cargo_test_all_targets",
            "cargo_test_docs",
            "cargo_clippy",
        ],
    })
Path(os.environ["HANDOFF_MANIFEST"]).write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n"
)
PY

python3 - "$repo_root" "$archive_path" "$source_tree_manifest" <<'PY'
import json
import sys
import zipfile
from pathlib import Path

repo_root = Path(sys.argv[1])
archive_path = Path(sys.argv[2])
source_manifest = json.loads(Path(sys.argv[3]).read_text())
members = [row["path"] for row in source_manifest["members"]]
members.extend(source_manifest["excluded_generated_members"])

with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for rel in members:
        archive.write(repo_root / rel, rel)
PY

python3 "$repo_root/scripts/handoff_safety_check.py" --archive "$archive_path"
(
  cd "$archive_dir"
  shasum -a 256 "$archive_name"
) >"$sha_path"

completed=1
echo "$archive_path"
echo "$sha_path"
