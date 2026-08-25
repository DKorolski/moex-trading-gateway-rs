#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# Preserve the complete R1 package and its 48 fail-closed mutations. This also
# refreshes only unauthenticated public FINAM documentation and live governance;
# it performs no account or broker request.
bash scripts/stage8b_p_r1_authorization_gate.sh

# Validate the additive R1A contract and all 50 corrective mutations.
python3 scripts/stage8b_p_r1a_authorization_check.py
python3 scripts/stage8b_p_r1a_authorization_negative_harness.py
python3 -m json.tool docs/stage-8/stage8b-p-r1a-authorization-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r1a-freshness-budget-authority.json >/dev/null
python3 -m json.tool docs/stage-8/stage8b-p-r1a-network-policy-authority.json >/dev/null
python3 -m py_compile \
  scripts/stage8b_p_r1a_authorization_check.py \
  scripts/stage8b_p_r1a_authorization_negative_harness.py
cargo fmt --all -- --check
git diff --check

echo "stage8b-p-r1a-authorization-gate: PASS rows=64 new_negatives=50 inherited=48 total=98 authorization=NOT_ISSUED broker_get=false arm=false transport=false finam=false"
