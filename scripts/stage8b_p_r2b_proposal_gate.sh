#!/usr/bin/env bash
set -euo pipefail

python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_r2a8_review_closure_check.py
python3 scripts/stage8b_p_r2b_proposal_check.py
python3 scripts/stage8b_p_r2b_proposal_negative_harness.py

echo "stage8b-p-r2b-proposal-gate: PASS proposal=true rows=30 negatives=30 authorization=NOT_ISSUED network=false order_post_delete=false runtime_live=false"
