#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Recheck the accepted current-tree and live governance controls without
# rewriting their immutable evidence. Public FINAM documentation refresh is
# unauthenticated and verifies exact accepted bytes only.
python3 scripts/current_tree_authority_check.py
python3 scripts/current_tree_authority_negative_harness.py
python3 scripts/stage8b_p_governance_refresh.py
python3 scripts/stage8b_p_contract_refresh.py \
  --snapshot docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json
python3 scripts/stage8b_p_preconditions_check.py --no-git

# Validate the design-only R1 authority and every declared fail-closed
# mutation. R1 issues no arm, records no dispatch attempt and performs no
# broker/account request.
python3 scripts/stage8b_p_r1_authorization_check.py
python3 scripts/stage8b_p_r1_authorization_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-finam-contract-snapshot-2026-08-24.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r1-authorization-authority.json >/dev/null
python3 -m py_compile \
  scripts/stage8b_p_contract_refresh.py \
  scripts/make_stage8b_p_r1_authorization_handoff.py \
  scripts/stage8b_p_r1_authorization_check.py \
  scripts/stage8b_p_r1_authorization_handoff_safety_check.py \
  scripts/stage8b_p_r1_authorization_negative_harness.py
cargo fmt --all -- --check
git diff --check

echo "stage8b-p-r1-authorization-gate: PASS rows=55 negatives=48 contract=7/7 governance=accepted authorization=NOT_ISSUED broker_get=false finam=false stage8b_p=false"
