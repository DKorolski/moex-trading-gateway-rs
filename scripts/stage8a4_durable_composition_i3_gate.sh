#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
artifact_dir="${STAGE8A4_I3_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-i3-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/$name.txt" 2>&1
}

run_gate inherited-i2 python3 scripts/stage8a4_durable_composition_i2_check.py --no-git
run_gate semantic-check python3 scripts/stage8a4_durable_composition_i3_check.py
run_gate semantic-negative python3 scripts/stage8a4_durable_composition_i3_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_durable_composition_i3_proof_map.py
run_gate fmt cargo fmt --all -- --check
run_gate core-focused cargo test -p strategy-runtime-core stage8a4_i3_ -- --test-threads=1
run_gate runtime-focused cargo test -p runtime-durable-service stage8a4_i3_ -- --test-threads=1
run_gate gateway-focused cargo test -p finam-gateway stage8a4 -- --test-threads=1
run_gate stage8a1-focused cargo test -p finam-gateway stage8a1_execution_capability -- --test-threads=1
run_gate core-tests cargo test -p strategy-runtime-core --no-fail-fast
run_gate runtime-tests cargo test -p runtime-durable-service --no-fail-fast -- --test-threads=1
run_gate gateway-tests cargo test -p finam-gateway --no-fail-fast
run_gate clippy cargo clippy -p strategy-runtime-core -p runtime-durable-service -p finam-gateway --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-durable-composition-i3-gate-summary.json" <<'PY'
import json
import sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-durable-composition-I3-R2",
    "result": "PASS",
    "acceptance_rows": 60,
    "negative_cases": 48,
    "sealed_linear_writer_authority": True,
    "exact_request_truth_control_binding": True,
    "post_write_sticky_fail_stop": True,
    "v2_durable_append_enabled": True,
    "four_field_cas_enabled": True,
    "covering_seal_writer_enabled": True,
    "ack_readiness_enabled": False,
    "redis_live_enabled": False,
    "finam_post_delete_enabled": False,
    "runtime_live_enabled": False,
    "real_orders_enabled": False
}, indent=2) + "\n", encoding="utf-8")
PY

echo "stage8a4-durable-composition-i3-gate: PASS rows=60 negatives=48 opaque=true sticky=true ack=false execution=false"
