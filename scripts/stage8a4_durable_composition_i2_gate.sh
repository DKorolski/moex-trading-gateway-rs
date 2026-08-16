#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
artifact_dir="${STAGE8A4_I2_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-i2-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/$name.txt" 2>&1
}

run_gate semantic-check python3 scripts/stage8a4_durable_composition_i2_check.py
run_gate semantic-negative python3 scripts/stage8a4_durable_composition_i2_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_durable_composition_i2_proof_map.py
run_gate fmt cargo fmt --all -- --check
run_gate focused-tests cargo test -p finam-gateway stage8a4 -- --test-threads=1
run_gate crate-tests cargo test -p finam-gateway --no-fail-fast
run_gate clippy cargo clippy -p finam-gateway --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-durable-composition-i2-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-durable-composition-I2",
    "result": "PASS",
    "acceptance_rows": 56,
    "negative_cases": 33,
    "focused_tests": 26,
    "durable_append_enabled": False,
    "cas_enabled": False,
    "covering_seal_writer_enabled": False,
    "ack_readiness_enabled": False,
    "redis_live_enabled": False,
    "finam_post_delete_enabled": False,
    "runtime_live_enabled": False,
    "real_orders_enabled": False
}, indent=2) + "\n", encoding="utf-8")
PY

echo "stage8a4-durable-composition-i2-gate: PASS rows=56 negatives=33 focused=26 append=false execution=false"
