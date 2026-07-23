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
source_tree_manifest="$repo_root/handoff-source-tree-manifest.json"
stage5e_gate_stdout_log="$repo_root/handoff-stage5e-gate-stdout.txt"
stage5e_gate_stderr_log="$repo_root/handoff-stage5e-gate-stderr.txt"
completed=0

cleanup() {
  local status=$?
  rm -f "$commit_marker" "$handoff_manifest" "$stage5e_gate_result" "$source_tree_manifest" \
    "$stage5e_gate_stdout_log" "$stage5e_gate_stderr_log"
  if [[ "$completed" -ne 1 ]]; then
    rm -f "$archive_path" "$sha_path"
  fi
  exit "$status"
}
trap cleanup EXIT

rm -f "$archive_path" "$sha_path"
python3 "$repo_root/scripts/handoff_safety_check.py" --source-tree "$repo_root"

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
stage5e_gate_result_sha256=""
stage5e_design_scope_sha256=""
source_tree_manifest_sha256=""
current_review_stage="$review_stage"
stage5e_inventory_rel="docs/stage-5/stage5e-lifecycle-event-time-attachment-inventory.json"
stage5e_plan_rel="docs/stage-5/5e-a-lifecycle-event-time-attachment-plan.md"
stage5e_checker_rel="scripts/stage5e_lifecycle_event_time_freeze_check.py"
if [[ -f "$repo_root/docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json" ]]; then
  stage5e_inventory_rel="docs/stage-5/stage5e-b-no-io-lifecycle-inventory.json"
  stage5e_plan_rel="docs/stage-5/5e-b-no-io-lifecycle-capability-plan.md"
  stage5e_checker_rel="scripts/stage5e_b_no_io_lifecycle_check.py"
fi
if [[ -f "$repo_root/$stage5e_checker_rel" ]] && [[ -f "$repo_root/$stage5e_inventory_rel" ]]; then
  stage5e_checker_sha256="$(shasum -a 256 "$repo_root/$stage5e_checker_rel" | awk '{print $1}')"
  stage5e_inventory_sha256="$(shasum -a 256 "$repo_root/$stage5e_inventory_rel" | awk '{print $1}')"
  stage5e_plan_sha256="$(shasum -a 256 "$repo_root/$stage5e_plan_rel" | awk '{print $1}')"
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

if [[ -n "$(git -C "$repo_root" status --porcelain --untracked-files=no)" ]]; then
  echo "Refusing to build review handoff: tracked source tree changed after gate." >&2
  git -C "$repo_root" status --short --untracked-files=no >&2
  exit 1
fi

SOURCE_REF="$source_ref" \
HEAD_TREE="$(git -C "$repo_root" rev-parse HEAD^{tree})" \
BASELINE_REF="${stage5e_baseline_ref:-}" \
CHANGED_PATHS_JSON="${stage5e_changed_paths_json:-[]}" \
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
    "excluded_generated_members": [
        "handoff-commit.txt",
        "handoff-manifest.json",
        "handoff-stage5e-gate-stderr.txt",
        "handoff-stage5e-gate-result.json",
        "handoff-stage5e-gate-stdout.txt",
        "handoff-source-tree-manifest.json",
    ],
    "members": members,
}
out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
PY
source_tree_manifest_sha256="$(shasum -a 256 "$source_tree_manifest" | awk '{print $1}')"
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
SOURCE_TREE_MANIFEST_SHA256="$source_tree_manifest_sha256" \
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
    "stage5e_checker_sha256": os.environ["STAGE5E_CHECKER_SHA256"],
    "stage5e_inventory_sha256": os.environ["STAGE5E_INVENTORY_SHA256"],
    "stage5e_plan_sha256": os.environ["STAGE5E_PLAN_SHA256"],
    "stage5e_gate_result_sha256": os.environ["STAGE5E_GATE_RESULT_SHA256"],
    "stage5e_design_scope_sha256": os.environ["STAGE5E_DESIGN_SCOPE_SHA256"],
    "source_tree_manifest_sha256": os.environ["SOURCE_TREE_MANIFEST_SHA256"],
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
}
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
