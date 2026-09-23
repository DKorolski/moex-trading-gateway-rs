#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 -m json.tool docs/stage-8/stage8b-p1e-i1-aggregate-closure-inventory.json >/dev/null
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_aggregate_readiness_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_aggregate_readiness_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_check.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_process_supervision_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_transaction_v5_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1_committed_restart_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1e_i1a_source_negative_harness.py
PYTHONPATH=scripts python3 scripts/stage8b_p1d4_source_negative_harness.py
git diff --check HEAD --

echo "PASS stage8b-p1e-i1-aggregate-readiness-gate"
