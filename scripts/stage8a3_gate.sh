#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${STAGE8A3_ARTIFACT_DIR:-$repo_root/tmp/stage8a3-gate}"
mkdir -p "$artifact_dir"
rm -f "$artifact_dir"/*
cd "$repo_root"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/${name}.stdout.txt" 2>"$artifact_dir/${name}.stderr.txt"
  printf 'PASS\n' >"$artifact_dir/${name}.result.txt"
}

run_gate semantic-check python3 scripts/stage8a3_check.py
run_gate semantic-negative python3 scripts/stage8a3_negative_harness.py

predecessor_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage8a2-accepted.XXXXXX")"
cleanup() {
  rm -rf "$predecessor_dir"
}
trap cleanup EXIT
git archive --format=tar 16180ac4f8eab761b3b055c1f5515f62cd94bfb9 | tar -xf - -C "$predecessor_dir"
run_gate inherited-stage8a2 env ACCEPTED_ROOT="$predecessor_dir" python3 -c '
import os, sys
from pathlib import Path
root = Path(os.environ["ACCEPTED_ROOT"])
sys.path.insert(0, str(root / "scripts"))
import stage8a2_check
stage8a2_check.check(root, git_scope=False, pin_hashes=True, exact_parent_delta=False)
print("inherited-stage8a2: PASS exact-ref=16180ac")
'

run_gate fmt cargo fmt --all -- --check
run_gate focused-test cargo test -p finam-gateway stage8a3 -- --test-threads=1
run_gate focused-doctest cargo test -p finam-gateway --doc stage8a3 -- --test-threads=1
run_gate focused-clippy cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings
run_gate workspace-debug cargo test --workspace --all-targets -- --test-threads=1
run_gate workspace-release cargo test --workspace --release --all-targets -- --test-threads=1
run_gate workspace-doctest cargo test --workspace --doc -- --test-threads=1
run_gate workspace-clippy cargo clippy --workspace --all-targets --all-features -- -D warnings
run_gate proof-map python3 scripts/stage8a3_proof_map.py

python3 - "$artifact_dir/stage8a3-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1]).parent
results = sorted(path.name for path in root.glob("*.result.txt"))
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-3-R1",
    "result": "PASS",
    "gate_count": len(results),
    "gates": results,
    "acceptance_rows": 64,
    "negative_cases": 42,
    "endpoint_context_explicit": True,
    "historical_classifier_authoritative": False,
    "retry_authority_available": False,
    "network_send_authorized": False,
    "reconciliation_implemented": False,
    "stage8a4_authorized": False,
    "runtime_live_authorized": False,
}, indent=2, sort_keys=True) + "\n")
PY

echo "stage8a3-r1-gate: PASS rows=64 negatives=42 endpoint-specific=true no-send=true next=8A-4-pending"
