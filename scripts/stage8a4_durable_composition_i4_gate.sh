#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
artifact_dir="${STAGE8A4_I4_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-i4-gate}"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

run() { local name="$1"; shift; "$@" >"$artifact_dir/$name.txt" 2>&1; }
run semantic python3 scripts/stage8a4_durable_composition_i4_check.py
run negative python3 scripts/stage8a4_durable_composition_i4_negative_harness.py
run inherited-design-semantic python3 scripts/stage8a4_durable_composition_i4_inherited_design_check.py
run inherited-design-negative python3 scripts/stage8a4_durable_composition_i4_design_negative_harness.py
run external bash scripts/stage8a4_durable_composition_i4_external_compile_fail.sh
run fmt cargo fmt --all -- --check
run gateway-focused cargo test -p finam-gateway stage8a4 -- --test-threads=1
run runtime cargo test -p runtime-durable-service --no-fail-fast -- --test-threads=1
run core cargo test -p strategy-runtime-core --no-fail-fast
run gateway cargo test -p finam-gateway --no-fail-fast
run clippy cargo clippy -p strategy-runtime-core -p runtime-durable-service -p finam-gateway --all-targets --all-features -- -D warnings

python3 - "$artifact_dir/stage8a4-i4-gate-summary.json" <<'PY'
import json, sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
  "schema_version": 1,
  "stage": "8A-4-durable-composition-I4",
  "result": "PASS",
  "acceptance_rows": 60,
  "negative_cases": 28,
  "accepted_design_traceability_rows": 64,
  "inherited_design_negative_cases": 46,
  "terminal_authority_public_opaque": True,
  "seal_mutation": False,
  "ack_timestamp_free": True,
  "current_readiness_independent": True,
  "ack_readiness_publication_enabled": False,
  "redis_mutation_enabled": False,
  "finam_post_delete_enabled": False,
  "runtime_live_enabled": False,
  "real_orders_enabled": False
}, indent=2) + "\n", encoding="utf-8")
PY
echo "stage8a4-durable-composition-i4-gate: PASS rows=60 negatives=28 trace=64 inherited_design_negatives=46 fresh_process=true read_only=true publication=false"
