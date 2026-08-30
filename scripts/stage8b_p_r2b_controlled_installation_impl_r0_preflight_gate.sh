#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2b_controlled_installation_r0_check.py
python3 scripts/stage8b_p_r2b_controlled_installation_r0_negative_harness.py
python3 scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py
python3 scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_negative_harness.py

python3 -m py_compile \
  scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_check.py \
  scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_negative_harness.py \
  scripts/stage8b_p_r2b_controlled_installation_impl_r0_preflight_handoff_safety_check.py \
  scripts/make_stage8b_p_r2b_controlled_installation_impl_r0_preflight_handoff.py
python3 -m json.tool docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-preflight-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-staging-inventory.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-canary-ceremony.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r2b-controlled-installation-impl-r0-reset-uninstall.json >/dev/null

git diff --check
echo "stage8b-p-r2b-controlled-installation-impl-r0-preflight-gate: PASS binaries=12 units=18 phases=6 services=31 negatives=31 execution=false authorization=NOT_ISSUED finam=false"
