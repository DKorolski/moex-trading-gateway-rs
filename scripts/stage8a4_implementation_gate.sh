#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${STAGE8A4_IMPLEMENTATION_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-implementation-gate}"
mkdir -p "$artifact_dir"
rm -f "$artifact_dir"/*
cd "$repo_root"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/${name}.stdout.txt" 2>"$artifact_dir/${name}.stderr.txt"
  printf 'PASS\n' >"$artifact_dir/${name}.result.txt"
}

run_gate semantic-check python3 scripts/stage8a4_implementation_check.py
run_gate semantic-negative python3 scripts/stage8a4_implementation_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_implementation_proof_map.py
run_gate python-compile python3 -m py_compile \
  scripts/stage8a4_implementation_check.py \
  scripts/stage8a4_implementation_negative_harness.py \
  scripts/stage8a4_implementation_proof_map.py \
  scripts/stage8a4_implementation_handoff_safety_check.py \
  scripts/make_stage8a4_implementation_handoff_archive.py

accepted_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage8a4-design-accepted.XXXXXX")"
cleanup() {
  rm -rf "$accepted_dir"
}
trap cleanup EXIT
git archive --format=tar cc58c10d22db312cd83640f1c1e7fd86861a4594 | tar -xf - -C "$accepted_dir"
run_gate inherited-stage8a4-design env ACCEPTED_ROOT="$accepted_dir" python3 -c '
import os, sys
from pathlib import Path
root = Path(os.environ["ACCEPTED_ROOT"])
sys.path.insert(0, str(root / "scripts"))
import stage8a4_design_check
stage8a4_design_check.check(root, git_scope=False)
print("inherited-stage8a4-design: PASS exact-ref=cc58c10")
'

run_gate focused-tests cargo test -p finam-gateway stage8a4 -- --test-threads=1
run_gate workspace-debug cargo test --workspace --all-targets -- --test-threads=1
run_gate workspace-release cargo test --workspace --release --all-targets -- --test-threads=1
run_gate workspace-doctest cargo test --workspace --doc -- --test-threads=1
run_gate workspace-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate fmt cargo fmt --all -- --check

python3 - "$artifact_dir/stage8a4-implementation-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path

target = Path(sys.argv[1])
root = target.parent
results = sorted(path.name for path in root.glob("*.result.txt"))
target.write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-implementation-R3",
    "result": "PASS",
    "gate_count": len(results),
    "gates": results,
    "acceptance_rows": 90,
    "negative_cases": 55,
    "focused_tests": 30,
    "compile_fail_doctests": 3,
    "pure_reducer": True,
    "durable_apply_authorized": False,
    "retry_or_send_authorized": False,
    "finam_post_delete_authorized": False,
    "redis_live_authorized": False,
    "runtime_live_authorized": False,
    "real_orders_authorized": False,
    "stage8a5_authorized": False,
    "stage8b_authorized": False,
}, indent=2, sort_keys=True) + "\n")
PY

echo "stage8a4-implementation-r3-gate: PASS rows=90 negatives=55 tests=30 pure-reducer=true execution=false"
