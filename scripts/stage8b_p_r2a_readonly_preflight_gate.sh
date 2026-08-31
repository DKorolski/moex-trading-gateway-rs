#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# R2A inherits the complete accepted R1/R1A/R1B identity and boundary matrix.
bash scripts/stage8b_p_r1b_identity_gate.sh
python3 scripts/stage8b_p_r2a_readonly_preflight_check.py
python3 scripts/stage8b_p_r2a_readonly_preflight_negative_harness.py
python3 scripts/stage8b_p_r2a_prepare.py --self-test
python3 -m json.tool docs/stage-8/stage8b-p-r2a-readonly-preflight-authority.json >/dev/null
python3 -m py_compile \
  scripts/stage8b_p_r2a_prepare.py \
  scripts/stage8b_p_r2a_readonly_preflight_check.py \
  scripts/stage8b_p_r2a_readonly_preflight_negative_harness.py
cargo fmt --all -- --check
git diff --check

echo "stage8b-p-r2a-gate: PASS rows=48 negatives=40 inherited=134 plan_only=true broker_get=false arm=false attempt=false effect_transport=false finam_post_delete=false authorization=NOT_ISSUED"
