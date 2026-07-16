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
completed=0

cleanup() {
  local status=$?
  rm -f "$commit_marker" "$handoff_manifest"
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
created_at_utc="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

SOURCE_COMMIT="$source_commit" \
SOURCE_REF="$source_ref" \
ARCHIVE_NAME="$archive_name" \
CREATED_AT_UTC="$created_at_utc" \
STAGE5C_CHECKER_SHA256="$stage5c_checker_sha256" \
STAGE5D_CHECKER_SHA256="$stage5d_checker_sha256" \
STAGE5D_MANIFEST_SHA256="$stage5d_manifest_sha256" \
HANDOFF_MANIFEST="$handoff_manifest" \
python3 - <<'PY'
import json
import os
from pathlib import Path

manifest = {
    "schema_version": 1,
    "review_stage": "5D-b2b-c1",
    "source_commit": os.environ["SOURCE_COMMIT"],
    "source_ref": os.environ["SOURCE_REF"],
    "archive_name": os.environ["ARCHIVE_NAME"],
    "created_at_utc": os.environ["CREATED_AT_UTC"],
    "stage5c_checker_sha256": os.environ["STAGE5C_CHECKER_SHA256"],
    "stage5d_checker_sha256": os.environ["STAGE5D_CHECKER_SHA256"],
    "stage5d_manifest_sha256": os.environ["STAGE5D_MANIFEST_SHA256"],
    "required_gate_names": [
        "stage5c_api_freeze",
        "stage5d_additive_freeze",
        "forbidden_surface",
        "forbidden_surface_negative",
        "stage5d_negative",
        "no_redis_smoke",
        "python_syntax",
        "fixture_parse",
        "handoff_source_safety",
        "handoff_archive_safety",
        "checker_input_completeness",
        "cargo_fmt",
        "cargo_test",
        "cargo_clippy",
    ],
}
Path(os.environ["HANDOFF_MANIFEST"]).write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n"
)
PY

(
  cd "$repo_root"
  zip -qr "$archive_path" . \
    -x '.git/*' \
    -x 'target/*' \
    -x 'tmp/*' \
    -x 'reports/*' \
    -x '.env' \
    -x '.env.*' \
    -x '*.log' \
    -x '*.local.*' \
    -x '__pycache__/*' \
    -x '__MACOSX/*' \
    -x '.DS_Store'
)

python3 "$repo_root/scripts/handoff_safety_check.py" --archive "$archive_path"
(
  cd "$archive_dir"
  shasum -a 256 "$archive_name"
) >"$sha_path"

completed=1
echo "$archive_path"
echo "$sha_path"
