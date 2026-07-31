#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
python_bin="${PYTHON:-python3}"

cd "$repo_root"

bash scripts/stage5f_inherited_b1_snapshot_gate.sh
if [[ -f docs/stage-5/stage5f-c-candidate-results.json ]]; then
  "$python_bin" scripts/stage5f_controlled_characterization_check.py
  "$python_bin" scripts/stage5f_controlled_characterization_negative_harness.py
  cargo test -p strategy-runtime-core stage5f_
else
  "$python_bin" scripts/stage5f_b0_source_reachability_check.py
  "$python_bin" scripts/stage5f_fixture_contract_check.py
  "$python_bin" scripts/stage5f_fixture_contract_negative_harness.py
fi
if [[ "${STAGE5F_FULL_INHERITED_GATE:-0}" == "1" ]]; then
  bash scripts/stage5f_b3f_snapshot_provenance_gate.sh
fi

echo "stage5f-functional-development-gate: ok inherited_b3f=${STAGE5F_FULL_INHERITED_GATE:-0}"
