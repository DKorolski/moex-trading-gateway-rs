#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${STAGE8A2_ARTIFACT_DIR:-$repo_root/tmp/stage8a2-gate}"
mkdir -p "$artifact_dir"
rm -f "$artifact_dir"/*
cd "$repo_root"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/${name}.stdout.txt" 2>"$artifact_dir/${name}.stderr.txt"
  printf 'PASS\n' >"$artifact_dir/${name}.result.txt"
}

run_gate semantic-check python3 scripts/stage8a2_check.py
run_gate semantic-negative python3 scripts/stage8a2_negative_harness.py

predecessor_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage8a1-accepted.XXXXXX")"
cleanup() {
  rm -rf "$predecessor_dir"
}
trap cleanup EXIT
git archive --format=tar 1ff04154ba4b7a5ee060a73b853ce89bd7442f44 | tar -xf - -C "$predecessor_dir"
run_gate inherited-stage8a1 env ACCEPTED_ROOT="$predecessor_dir" python3 -c '
import os, sys
from pathlib import Path
root = Path(os.environ["ACCEPTED_ROOT"])
sys.path.insert(0, str(root / "scripts"))
import stage8a1_check
stage8a1_check.check(root, git_scope=False, pin_hashes=True)
print("inherited-stage8a1: PASS exact-ref=1ff0415")
'

run_gate fmt cargo fmt --all -- --check
run_gate focused-test cargo test -p finam-gateway stage8a2 -- --nocapture
run_gate focused-doctest cargo test -p finam-gateway --doc -- --test-threads=1
run_gate focused-clippy cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
run_gate workspace-debug cargo test --workspace --all-targets -- --test-threads=1
run_gate workspace-release cargo test --workspace --release --all-targets -- --test-threads=1
run_gate workspace-doctest cargo test --workspace --doc -- --test-threads=1
run_gate workspace-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate proof-map python3 scripts/stage8a2_proof_map.py "$artifact_dir/stage8a2-proof-map.json"

python3 - "$artifact_dir/stage8a2-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1]).parent
results = sorted(path.name for path in root.glob("*.result.txt"))
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-2-R1",
    "result": "PASS",
    "gate_count": len(results),
    "gates": results,
    "acceptance_rows": 50,
    "negative_cases": 37,
    "existing_place_builder_only": True,
    "existing_cancel_builder_only": True,
    "place_comment_none": True,
    "in_memory_no_send": True,
    "finam_post_delete_authorized": False,
    "stage8a3_authorized": False,
    "runtime_live_authorized": False,
}, indent=2, sort_keys=True) + "\n")
PY

echo "stage8a2-r1-gate: PASS rows=50 negatives=37 builder-only=true comment-none=true no-send=true next=8A-3-pending"
