#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 -m json.tool docs/stage-8/stage8b-p1f-target-vps-baseline.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p1f-isolated-operational-acceptance-design.json >/dev/null
PYTHONPATH=scripts python3 scripts/stage8b_p1f_design_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1f_design_negative_harness.py

closure_archive="reports/handoff/moex-trading-project-3f171d9-stage8b-p1e-i1-governance-closure.zip"
test -f "$closure_archive"
actual_sha="$(shasum -a 256 "$closure_archive" | awk '{print $1}')"
test "$actual_sha" = "b5920849fac4294f955c35847a342be4c7d572810f799c68f7f598c551a49cb1"
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_governance_closure_handoff_safety_check.py "$closure_archive"

git diff --check 3f171d997de5616cb9a07311d7776e446456c0c1 --
echo "PASS stage8b-p1f-design-gate rows=36 negatives=23 phases=7 remote_mutation=false activation=false"
