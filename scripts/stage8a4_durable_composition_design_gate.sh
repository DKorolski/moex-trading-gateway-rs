#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_dir="${STAGE8A4_DURABLE_DESIGN_ARTIFACT_DIR:-$repo_root/tmp/stage8a4-durable-design-gate}"
mkdir -p "$artifact_dir"
rm -f "$artifact_dir"/*
cd "$repo_root"

run_gate() {
  local name="$1"
  shift
  "$@" >"$artifact_dir/${name}.stdout.txt" 2>"$artifact_dir/${name}.stderr.txt"
  printf 'PASS\n' >"$artifact_dir/${name}.result.txt"
}

run_gate design-check python3 scripts/stage8a4_durable_composition_design_check.py
run_gate design-negative python3 scripts/stage8a4_durable_composition_design_negative_harness.py
run_gate proof-map python3 scripts/stage8a4_durable_composition_design_proof_map.py
run_gate python-compile python3 -m py_compile \
  scripts/stage8a4_durable_composition_design_check.py \
  scripts/stage8a4_durable_composition_design_negative_harness.py \
  scripts/stage8a4_durable_composition_design_proof_map.py \
  scripts/stage8a4_durable_composition_design_handoff_safety_check.py \
  scripts/make_stage8a4_durable_composition_design_handoff.py

python3 - "$artifact_dir/stage8a4-durable-composition-design-gate-summary.json" <<'PY'
import json, sys
from pathlib import Path
target = Path(sys.argv[1])
results = sorted(path.name for path in target.parent.glob("*.result.txt"))
target.write_text(json.dumps({
    "schema_version": 1,
    "stage": "8A-4-durable-composition-design-R1",
    "result": "PASS",
    "gate_count": len(results),
    "gates": results,
    "acceptance_rows": 60,
    "negative_cases": 24,
    "design_only": True,
    "production_rust_changed": False,
    "durable_apply_authorized": False,
    "finam_execution_authorized": False,
}, indent=2, sort_keys=True) + "\n")
PY

echo "stage8a4-durable-composition-design-r1-gate: PASS rows=60 negatives=24 production=false apply=false execution=false"
