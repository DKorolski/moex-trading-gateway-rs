#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
archive_dir="$repo_root/reports/handoff"
stage="5F-d-complete-atomic-hybrid-matrix"
mkdir -p "$archive_dir"

if [[ -n "$(git -C "$repo_root" status --porcelain --untracked-files=all)" ]]; then
  echo "Refusing to build Stage 5F-d handoff: source tree is dirty." >&2
  git -C "$repo_root" status --short >&2
  exit 1
fi

object_format="$(git -C "$repo_root" rev-parse --show-object-format)"
if [[ "$object_format" != "sha1" ]]; then
  echo "Unsupported Git object format: $object_format (expected sha1)." >&2
  exit 1
fi

source_ref="$(git -C "$repo_root" rev-parse HEAD)"
parent_ref="$(git -C "$repo_root" rev-parse HEAD^)"
source_commit="$(git -C "$repo_root" rev-parse --short=7 HEAD)"
head_tree="$(git -C "$repo_root" rev-parse 'HEAD^{tree}')"
archive_name="moex-trading-project-${source_commit}.zip"
archive_path="$archive_dir/$archive_name"
sha_path="$archive_path.sha256"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage5f-d-handoff.XXXXXX")"
tmp_archive="$tmp_dir/$archive_name"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

run_gate() {
  local label="$1"
  shift
  local stdout_member="stage5f-d-${label}-stdout.txt"
  local stderr_member="stage5f-d-${label}-stderr.txt"
  local result_member="stage5f-d-${label}-result.json"
  local stdout_path="$tmp_dir/$stdout_member"
  local stderr_path="$tmp_dir/$stderr_member"
  local result_path="$tmp_dir/$result_member"
  local started_at_utc
  local finished_at_utc
  local exit_code

  started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  set +e
  (
    cd "$repo_root"
    "$@"
  ) >"$stdout_path" 2>"$stderr_path"
  exit_code="$?"
  set -e
  finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

  if [[ "$exit_code" -ne 0 ]]; then
    cat "$stdout_path"
    cat "$stderr_path" >&2
    echo "Stage 5F-d gate failed: $label" >&2
    exit "$exit_code"
  fi

  python3 - "$result_path" "$label" "$source_ref" "$started_at_utc" \
    "$finished_at_utc" "$exit_code" "$stdout_member" "$stdout_path" \
    "$stderr_member" "$stderr_path" "$@" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

(
    result_path,
    label,
    source_ref,
    started_at_utc,
    finished_at_utc,
    exit_code,
    stdout_member,
    stdout_path,
    stderr_member,
    stderr_path,
    *command,
) = sys.argv[1:]

def sha256(path: str) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

payload = {
    "command": command,
    "exit_code": int(exit_code),
    "finished_at_utc": finished_at_utc,
    "label": label,
    "schema_version": 1,
    "source_ref": source_ref,
    "stage": "5F-d-complete-atomic-hybrid-matrix",
    "started_at_utc": started_at_utc,
    "stderr_member": stderr_member,
    "stderr_sha256": sha256(stderr_path),
    "stdout_member": stdout_member,
    "stdout_sha256": sha256(stdout_path),
}
Path(result_path).write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

run_gate fmt cargo fmt --all -- --check
run_gate workspace-tests cargo test --workspace --all-targets
run_gate doctests cargo test --workspace --doc
run_gate clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate matrix-checker python3 scripts/stage5f_d_atomic_matrix_check.py
run_gate matrix-negative python3 scripts/stage5f_d_atomic_matrix_negative_harness.py
run_gate matrix-debug cargo test -p strategy-runtime-core stage5f_ -- --test-threads=1
run_gate matrix-release cargo test --release -p strategy-runtime-core stage5f_ -- --test-threads=1
run_gate matrix-determinism cargo test -p strategy-runtime-core   stage5f_d_full_matrix_repeat_is_byte_identical -- --nocapture --test-threads=1
run_gate r3-snapshot bash scripts/stage5f_r3_snapshot_gate.sh
run_gate inherited-b1 bash scripts/stage5f_inherited_b1_snapshot_gate.sh
run_gate inherited-b3f bash scripts/stage5f_b3f_snapshot_provenance_gate.sh
run_gate functional bash scripts/stage5f_functional_development_gate.sh

python3 - "$repo_root" "$tmp_dir/stage5f-d-source-tree-manifest.json" \
  "$source_ref" "$parent_ref" "$source_commit" "$head_tree" <<'PY'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
output = Path(sys.argv[2])
source_ref = sys.argv[3]
parent_ref = sys.argv[4]
source_commit = sys.argv[5]
head_tree = sys.argv[6]

raw = subprocess.check_output(
    ["git", "ls-tree", "-r", "-z", source_ref],
    cwd=root,
)
members = []
for record in raw.split(b"\0"):
    if not record:
        continue
    metadata, path_raw = record.split(b"\t", 1)
    mode, kind, object_id = metadata.decode("ascii").split()
    path = path_raw.decode("utf-8")
    if kind != "blob" or mode not in {"100644", "100755"}:
        raise SystemExit(f"unsupported tracked entry: {mode} {kind} {path}")
    body = subprocess.check_output(
        ["git", "cat-file", "blob", object_id],
        cwd=root,
    )
    members.append({
        "git_mode": mode,
        "path": path,
        "sha256": hashlib.sha256(body).hexdigest(),
    })

members.sort(key=lambda item: item["path"])
payload = {
    "head_tree": head_tree,
    "members": members,
    "parent_ref": parent_ref,
    "schema_version": 1,
    "source_commit": source_commit,
    "source_ref": source_ref,
    "stage": "5F-d-complete-atomic-hybrid-matrix",
}
output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

git -C "$repo_root" cat-file commit "$source_ref" \
  >"$tmp_dir/stage5f-d-commit-object.txt"

python3 - "$tmp_dir/handoff-commit.txt" "$archive_name" "$source_commit" "$source_ref" "$parent_ref" <<'PY'
import sys
from pathlib import Path

output, archive_name, source_commit, source_ref, parent_ref = sys.argv[1:]
Path(output).write_text(
    f"archive_name={archive_name}\n"
    f"source_commit={source_commit}\n"
    f"source_ref={source_ref}\n"
    f"parent_ref={parent_ref}\n",
    encoding="utf-8",
)
PY

rustc_version="$(rustc --version)"
cargo_version="$(cargo --version)"
python3 - "$tmp_dir" "$archive_name" "$source_commit" "$source_ref" \
  "$parent_ref" "$head_tree" "$rustc_version" "$cargo_version" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
archive_name, source_commit, source_ref, parent_ref, head_tree, rustc_version, cargo_version = sys.argv[2:]
labels = [
    "fmt",
    "workspace-tests",
    "doctests",
    "clippy",
    "matrix-checker",
    "matrix-negative",
    "matrix-debug",
    "matrix-release",
    "matrix-determinism",
    "r3-snapshot",
    "inherited-b1",
    "inherited-b3f",
    "functional",
]

def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

gates = []
for label in labels:
    member = f"stage5f-d-{label}-result.json"
    gates.append({
        "label": label,
        "result_member": member,
        "result_sha256": sha256(root / member),
    })

payload = {
    "archive_name": archive_name,
    "cargo_version": cargo_version,
    "closed_surfaces": {
        "broker_execution": False,
        "dispatch": False,
        "feedback_lifecycle": False,
        "finam_transport": False,
        "http_post_delete": False,
        "protective_orders": False,
        "redis": False,
        "runtime_live": False,
        "stage5g_lifecycle": False,
    },
    "commit_object_sha256": sha256(root / "stage5f-d-commit-object.txt"),
    "gates": gates,
    "head_tree": head_tree,
    "parent_ref": parent_ref,
    "rustc_version": rustc_version,
    "schema_version": 1,
    "source_commit": source_commit,
    "source_ref": source_ref,
    "source_tree_manifest_sha256": sha256(root / "stage5f-d-source-tree-manifest.json"),
    "stage": "5F-d-complete-atomic-hybrid-matrix",
    "status": "review_required_before_5f_e",
}
(root / "stage5f-d-evidence-manifest.json").write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

rm -f "$tmp_archive"
git -C "$repo_root" archive --format=zip --output="$tmp_archive" "$source_ref"

python3 - "$tmp_archive" "$tmp_dir" <<'PY'
import sys
import zipfile
from pathlib import Path

archive = Path(sys.argv[1])
root = Path(sys.argv[2])
labels = [
    "fmt",
    "workspace-tests",
    "doctests",
    "clippy",
    "matrix-checker",
    "matrix-negative",
    "matrix-debug",
    "matrix-release",
    "matrix-determinism",
    "r3-snapshot",
    "inherited-b1",
    "inherited-b3f",
    "functional",
]
members = [
    "handoff-commit.txt",
    "stage5f-d-commit-object.txt",
    "stage5f-d-source-tree-manifest.json",
    "stage5f-d-evidence-manifest.json",
]
for label in labels:
    members.extend([
        f"stage5f-d-{label}-result.json",
        f"stage5f-d-{label}-stdout.txt",
        f"stage5f-d-{label}-stderr.txt",
    ])

with zipfile.ZipFile(archive, "a", compression=zipfile.ZIP_DEFLATED) as handle:
    for member in members:
        handle.write(root / member, member)
PY

preseal_stdout_member="stage5f-d-archive-safety-stdout.txt"
preseal_stderr_member="stage5f-d-archive-safety-stderr.txt"
preseal_started_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
set +e
(
  cd "$repo_root"
  python3 scripts/stage5f_d_handoff_safety_check.py \
    --archive "$tmp_archive" --allow-missing-final-safety
) >"$tmp_dir/$preseal_stdout_member" 2>"$tmp_dir/$preseal_stderr_member"
preseal_exit_code="$?"
set -e
preseal_finished_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
if [[ "$preseal_exit_code" -ne 0 ]]; then
  cat "$tmp_dir/$preseal_stdout_member"
  cat "$tmp_dir/$preseal_stderr_member" >&2
  echo "Stage 5F-d preseal archive safety failed." >&2
  exit "$preseal_exit_code"
fi

python3 - "$tmp_dir" "$source_ref" "$archive_name" \
  "$preseal_started_at_utc" "$preseal_finished_at_utc" "$preseal_exit_code" \
  "$preseal_stdout_member" "$preseal_stderr_member" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
source_ref = sys.argv[2]
archive_name = sys.argv[3]
started_at_utc = sys.argv[4]
finished_at_utc = sys.argv[5]
exit_code = int(sys.argv[6])
stdout_member = sys.argv[7]
stderr_member = sys.argv[8]

def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

payload = {
    "checked_evidence_manifest_sha256": sha256(root / "stage5f-d-evidence-manifest.json"),
    "checked_source_tree_manifest_sha256": sha256(root / "stage5f-d-source-tree-manifest.json"),
    "command": [
        "python3",
        "scripts/stage5f_d_handoff_safety_check.py",
        "--archive",
        archive_name,
        "--allow-missing-final-safety",
    ],
    "finished_at_utc": finished_at_utc,
    "preseal_exit_code": exit_code,
    "schema_version": 1,
    "source_ref": source_ref,
    "stage": "5F-d-complete-atomic-hybrid-matrix",
    "started_at_utc": started_at_utc,
    "stderr_member": stderr_member,
    "stderr_sha256": sha256(root / stderr_member),
    "stdout_member": stdout_member,
    "stdout_sha256": sha256(root / stdout_member),
}
(root / "stage5f-d-archive-safety-result.json").write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

python3 - "$tmp_archive" "$tmp_dir" <<'PY'
import sys
import zipfile
from pathlib import Path

archive = Path(sys.argv[1])
root = Path(sys.argv[2])
members = [
    "stage5f-d-archive-safety-result.json",
    "stage5f-d-archive-safety-stdout.txt",
    "stage5f-d-archive-safety-stderr.txt",
]
with zipfile.ZipFile(archive, "a", compression=zipfile.ZIP_DEFLATED) as handle:
    for member in members:
        handle.write(root / member, member)
PY

python3 "$repo_root/scripts/stage5f_d_handoff_safety_check.py" \
  --archive "$tmp_archive"

mv -f "$tmp_archive" "$archive_path"
python3 - "$archive_path" "$sha_path" <<'PY'
import hashlib
import sys
from pathlib import Path

archive = Path(sys.argv[1])
output = Path(sys.argv[2])
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
output.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
PY

echo "Stage 5F-d handoff archive: $archive_path"
echo "Stage 5F-d handoff SHA-256: $sha_path"
cat "$sha_path"
