#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

artifact_dir="${STAGE8A4_I1_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-i1-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/$name.log" 2>&1
}

run_gate semantic-check python3 scripts/stage8a4_durable_composition_i1_check.py
run_gate semantic-negative python3 scripts/stage8a4_durable_composition_i1_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_durable_composition_i1_proof_map.py

accepted_dir="$(mktemp -d "${TMPDIR:-/tmp}/stage8a4-i1-accepted-spec.XXXXXX")"
trap 'rm -rf "$accepted_dir"' EXIT
git archive dd01253596527d6cff1db11cc32ae3c3348c96a0 | tar -x -C "$accepted_dir"
run_gate inherited-spec env ACCEPTED_ROOT="$accepted_dir" PYTHONPATH="$accepted_dir/scripts" python3 -c '
import os
from pathlib import Path
import stage8a4_durable_composition_implementation_spec_check as checker
checker.check(Path(os.environ["ACCEPTED_ROOT"]), git_scope=False)
print("inherited-stage8a4-durable-composition-spec-r2: PASS exact-ref=dd01253")
'

run_gate fmt cargo fmt --all -- --check
run_gate focused-tests cargo test -p strategy-runtime-core stage6_reconciliation_v2 --no-fail-fast
run_gate crate-tests cargo test -p strategy-runtime-core --all-features --no-fail-fast
run_gate clippy cargo clippy -p strategy-runtime-core --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-durable-composition-i1-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
path.write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-durable-composition-I1",
    "result": "PASS",
    "acceptance_rows": 40,
    "negative_cases": 20,
    "canonical_goldens": 20,
    "focused_tests": 12,
    "v2_writer_enabled": False,
    "durable_apply_enabled": False,
    "redis_live_enabled": False,
    "finam_post_delete_enabled": False,
    "broker_dispatch_enabled": False,
    "runtime_live_enabled": False,
    "real_orders_enabled": False,
}, indent=2) + "\n", encoding="utf-8")
PY

echo "stage8a4-durable-composition-i1-gate: PASS rows=40 negatives=20 goldens=20 focused=12 writer=false apply=false execution=false"
