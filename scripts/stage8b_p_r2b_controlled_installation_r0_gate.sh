#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_issuance_systemd_check.py
python3 scripts/stage8b_p_r2b_implementation_r0_r1_check.py
python3 scripts/stage8b_p_r2b_controlled_installation_r0_check.py
python3 scripts/stage8b_p_r2b_controlled_installation_r0_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_controlled_installation_r0_check.py \
  scripts/stage8b_p_r2b_controlled_installation_r0_negative_harness.py \
  scripts/stage8b_p_r2b_controlled_installation_r0_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_controlled_installation_r0_handoff.py

python3 -m json.tool docs/stage-8/stage8b-p-r2b-preproduction-supersession.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-implementation-transaction-contract.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-controlled-installation-r0-authority.json >/dev/null

git diff --check
echo "stage8b-p-r2b-controlled-installation-r0-gate: PASS predecessor=6672819e357a3c2a2c1e73e5408c393da01913a1 supersession=recorded phases=6 services=31 rows=24 negative_mutations=20 installed=false enabled=false started=false authorization=NOT_ISSUED finam_requests=0"
