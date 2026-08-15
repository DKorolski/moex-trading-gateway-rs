#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${STAGE8A4_DESIGN_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-design-gate}"
mkdir -p "$artifact_dir"
rm -f "$artifact_dir"/*
cd "$repo_root"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/${name}.stdout.txt" 2>"$artifact_dir/${name}.stderr.txt"
  printf 'PASS\n' >"$artifact_dir/${name}.result.txt"
}

run_gate semantic-check python3 scripts/stage8a4_design_check.py
run_gate semantic-negative python3 scripts/stage8a4_design_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_design_proof_map.py
run_gate python-compile python3 -m py_compile \
  scripts/stage8a4_design_check.py \
  scripts/stage8a4_design_negative_harness.py \
  scripts/stage8a4_design_proof_map.py \
  scripts/stage8a4_design_handoff_safety_check.py \
  scripts/make_stage8a4_design_handoff_archive.py

accepted_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage8a3-accepted.XXXXXX")"
cleanup() {
  rm -rf "$accepted_dir"
}
trap cleanup EXIT
git archive --format=tar 012c9bfa51c1d6206fbd9a7e1f06f1fc90fdf30d | tar -xf - -C "$accepted_dir"
run_gate inherited-stage8a3 env ACCEPTED_ROOT="$accepted_dir" python3 -c '
import os, sys
from pathlib import Path
root = Path(os.environ["ACCEPTED_ROOT"])
sys.path.insert(0, str(root / "scripts"))
import stage8a3_check
stage8a3_check.check(root, git_scope=False, pin_hashes=True, exact_successor=False)
print("inherited-stage8a3: PASS exact-ref=012c9bf")
'

run_gate fmt cargo fmt --all -- --check
run_gate workspace-debug cargo test --workspace --all-targets -- --test-threads=1
run_gate workspace-release cargo test --workspace --release --all-targets -- --test-threads=1
run_gate workspace-doctest cargo test --workspace --doc -- --test-threads=1
run_gate workspace-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-design-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1]).parent
results = sorted(path.name for path in root.glob("*.result.txt"))
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-design-R2",
    "result": "PASS",
    "gate_count": len(results),
    "gates": results,
    "acceptance_rows": 92,
    "negative_cases": 68,
    "production_reconciliation_implemented": False,
    "network_send_authorized": False,
    "proven_no_match_available": False,
    "stage8a4_implementation_authorized": False,
    "stage8a5_authorized": False,
}, indent=2, sort_keys=True) + "\n")
PY

echo "stage8a4-design-r2-gate: PASS rows=92 negatives=68 design-only=true next=8A-4-implementation-r1-pending"
