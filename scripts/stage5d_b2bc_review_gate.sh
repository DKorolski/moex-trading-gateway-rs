#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
gate_tmp="$(mktemp -d "${TMPDIR:-/tmp}/stage5d-review-gate.XXXXXX")"
trap 'rm -rf "$gate_tmp"' EXIT

python_with_tomllib=""
for candidate in python3 python3.14 python3.13 python3.12 python3.11; do
  if command -v "$candidate" >/dev/null 2>&1 \
    && "$candidate" -c 'import tomllib' >/dev/null 2>&1; then
    python_with_tomllib="$candidate"
    break
  fi
done
if [[ -z "$python_with_tomllib" ]]; then
  echo "stage5d review gate: Python 3.11+ with stdlib tomllib is required" >&2
  exit 1
fi

run_gate() {
  local name="$1"
  shift
  echo "GATE START $name"
  "$@"
  echo "GATE PASS  $name"
}

check_copied_baseline() {
  python3 scripts/copy_review_baseline.py "$repo_root" "$gate_tmp/baseline"
  (cd "$gate_tmp/baseline" && bash scripts/forbidden_surface_scan.sh)
}

check_archive_safety() {
  local fixture_root="$gate_tmp/archive-fixture"
  local archive_name="moex-trading-project-0000000.zip"
  local archive_path="$gate_tmp/$archive_name"
  python3 scripts/copy_review_baseline.py "$repo_root" "$fixture_root" >/dev/null
  local review_stage
  local stage5c_checker_sha256
  local stage5d_checker_sha256
  local stage5d_manifest_sha256
  review_stage="$("$python_with_tomllib" - "$fixture_root/docs/stage-5/stage-5d-additive-freeze-manifest.json" <<'PY'
import json
import sys
print(json.loads(open(sys.argv[1]).read())["stage"])
PY
)"
  stage5c_checker_sha256="$(shasum -a 256 "$fixture_root/scripts/stage5c_api_freeze_check.py" | awk '{print $1}')"
  stage5d_checker_sha256="$(shasum -a 256 "$fixture_root/scripts/stage5d_additive_freeze_check.py" | awk '{print $1}')"
  stage5d_manifest_sha256="$(shasum -a 256 "$fixture_root/docs/stage-5/stage-5d-additive-freeze-manifest.json" | awk '{print $1}')"
  printf '%s\n' \
    "source_commit=0000000" \
    "source_ref=0000000000000000000000000000000000000000" \
    "archive_name=$archive_name" >"$fixture_root/handoff-commit.txt"
  REVIEW_STAGE="$review_stage" \
  STAGE5C_CHECKER_SHA256="$stage5c_checker_sha256" \
  STAGE5D_CHECKER_SHA256="$stage5d_checker_sha256" \
  STAGE5D_MANIFEST_SHA256="$stage5d_manifest_sha256" \
  python3 - "$fixture_root/handoff-manifest.json" "$archive_name" <<'PY'
import json
import os
import sys
from pathlib import Path

Path(sys.argv[1]).write_text(
    json.dumps(
        {
            "schema_version": 1,
            "review_stage": os.environ["REVIEW_STAGE"],
            "source_commit": "0000000",
            "source_ref": "0000000000000000000000000000000000000000",
            "archive_name": sys.argv[2],
            "stage5c_checker_sha256": os.environ["STAGE5C_CHECKER_SHA256"],
            "stage5d_checker_sha256": os.environ["STAGE5D_CHECKER_SHA256"],
            "stage5d_manifest_sha256": os.environ["STAGE5D_MANIFEST_SHA256"],
        },
        indent=2,
        sort_keys=True,
    )
    + "\n"
)
PY
  (cd "$fixture_root" && zip -qr "$archive_path" .)
  python3 scripts/handoff_safety_check.py --archive "$archive_path"
}

run_gate stage5c_api_freeze python3 scripts/stage5c_api_freeze_check.py
run_gate stage5d_additive_freeze python3 scripts/stage5d_additive_freeze_check.py
run_gate forbidden_surface bash scripts/forbidden_surface_scan.sh
run_gate forbidden_surface_negative bash scripts/forbidden_surface_negative_harness.sh
run_gate stage5d_negative python3 scripts/stage5d_additive_freeze_negative_harness.py
run_gate handoff_provenance_negative python3 scripts/handoff_provenance_negative_harness.py
run_gate no_redis_smoke bash scripts/test_m4_3x_evidence_no_redis.sh
run_gate python_syntax python3 -c 'import pathlib; paths=sorted(pathlib.Path("scripts").glob("*.py")); [compile(p.read_bytes(), str(p), "exec") for p in paths]; print(f"python-syntax: ok files={len(paths)}")'
run_gate fixture_parse "$python_with_tomllib" -c 'import json, pathlib, tomllib; root=pathlib.Path("."); [json.loads(p.read_text()) for p in root.rglob("*.json") if not any(x in p.parts for x in ("target","tmp","reports",".git"))]; [tomllib.loads(p.read_text()) for p in root.rglob("*.toml") if not any(x in p.parts for x in ("target","tmp","reports",".git"))]; print("fixture-parse: ok")'
run_gate handoff_source_safety python3 scripts/handoff_safety_check.py --source-tree "$repo_root"
run_gate handoff_archive_safety check_archive_safety
run_gate checker_input_completeness check_copied_baseline
run_gate cargo_fmt cargo fmt --all --check
run_gate cargo_test_all_targets cargo test --workspace --all-targets
run_gate cargo_test_docs cargo test --workspace --doc
run_gate cargo_clippy cargo clippy --workspace --all-targets -- -D warnings

echo "stage5d-b2bc-review-gate: all required gates passed"
